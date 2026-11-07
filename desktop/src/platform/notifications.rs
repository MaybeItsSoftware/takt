//! Notifications through `UNUserNotificationCenter`, as the Swift app sent
//! them: shown while the app is in front too, and a click on one about a
//! task opens it. Only a bundled app may use the centre (it asks the bundle
//! who it is), so a bare binary falls back to `osascript`.

use super::{Event, ns_string, push};
use block::{Block, ConcreteBlock};
use objc::declare::ClassDecl;
use objc::runtime::{Class, Object, Sel};
use std::sync::Once;

#[link(name = "UserNotifications", kind = "framework")]
unsafe extern "C" {}

type Id = *mut Object;

/// Banner, list and sound: `UNNotificationPresentationOptions`.
const PRESENT: u64 = (1 << 4) | (1 << 3) | (1 << 1);

extern "C" fn will_present(_: &Object, _: Sel, _center: Id, _notification: Id, handler: Id) {
    let handler = handler as *mut Block<(u64,), ()>;
    if !handler.is_null() {
        unsafe { (*handler).call((PRESENT,)) };
    }
}

extern "C" fn did_receive(_: &Object, _: Sel, _center: Id, response: Id, handler: Id) {
    let handler = handler as *mut Block<(), ()>;
    unsafe {
        let notification: Id = msg_send![response, notification];
        let request: Id = msg_send![notification, request];
        let content: Id = msg_send![request, content];
        let info: Id = msg_send![content, userInfo];
        let task: Id = msg_send![info, objectForKey: ns_string("task")];
        match super::rust_string(task) {
            Some(task) => push(Event::Menu(format!("open:{task}"))),
            None => push(Event::ShowWindow),
        }
        if !handler.is_null() {
            (*handler).call(());
        }
    }
}

fn center() -> Option<Id> {
    super::bundle_id()?;
    let class = Class::get("UNUserNotificationCenter")?;
    static SETUP: Once = Once::new();
    let center: Id = unsafe { msg_send![class, currentNotificationCenter] };
    SETUP.call_once(|| unsafe {
        let mut decl = ClassDecl::new("TaktNotificationDelegate", class!(NSObject))
            .expect("declare the notification delegate");
        decl.add_method(
            sel!(userNotificationCenter:willPresentNotification:withCompletionHandler:),
            will_present as extern "C" fn(&Object, Sel, Id, Id, Id),
        );
        decl.add_method(
            sel!(userNotificationCenter:didReceiveNotificationResponse:withCompletionHandler:),
            did_receive as extern "C" fn(&Object, Sel, Id, Id, Id),
        );
        let delegate_class = decl.register();
        let delegate: Id = msg_send![delegate_class, new];
        let _: () = msg_send![center, setDelegate: delegate];
        // Alert, sound, badge.
        let asked = ConcreteBlock::new(|_granted: bool, _error: Id| {}).copy();
        let _: () =
            msg_send![center, requestAuthorizationWithOptions: 7u64 completionHandler: &*asked];
    });
    Some(center)
}

/// Shows `title` now. `id` replaces an earlier one of the same id; `task`
/// is opened when it is clicked.
pub fn post(id: &str, title: &str, subtitle: &str, body: &str, sound: bool, task: Option<&str>) {
    let Some(center) = center() else {
        return osascript(title, subtitle, body, sound);
    };
    unsafe {
        let content: Id = msg_send![class!(UNMutableNotificationContent), new];
        let _: () = msg_send![content, setTitle: ns_string(title)];
        if !subtitle.is_empty() {
            let _: () = msg_send![content, setSubtitle: ns_string(subtitle)];
        }
        let _: () = msg_send![content, setBody: ns_string(body)];
        if sound {
            let chime: Id = msg_send![class!(UNNotificationSound), defaultSound];
            let _: () = msg_send![content, setSound: chime];
        }
        if let Some(task) = task {
            let info: Id = msg_send![class!(NSDictionary),
                dictionaryWithObject: ns_string(task)
                forKey: ns_string("task")];
            let _: () = msg_send![content, setUserInfo: info];
        }
        let nothing: Id = std::ptr::null_mut();
        let request: Id = msg_send![class!(UNNotificationRequest),
            requestWithIdentifier: ns_string(id)
            content: content
            trigger: nothing];
        let _: () =
            msg_send![center, addNotificationRequest: request withCompletionHandler: nothing];
        let _: () = msg_send![content, release];
    }
}

fn osascript(title: &str, subtitle: &str, body: &str, sound: bool) {
    let quote = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
    let mut script = format!(
        "display notification \"{}\" with title \"{}\" subtitle \"{}\"",
        quote(body),
        quote(title),
        quote(subtitle)
    );
    if sound {
        script.push_str(" sound name \"Glass\"");
    }
    let _ = std::process::Command::new("/usr/bin/osascript")
        .args(["-e", &script])
        .spawn();
}
