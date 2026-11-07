//! The Mac's own glue: what AppKit and the system services give that GPUI
//! does not. Each piece is its own file, reached through plain functions so
//! the state never sees an Objective-C type.
//!
//! AppKit's callbacks (a hotkey, a menu item) can't reach GPUI's context,
//! so they leave an [`Event`] in a queue the window drains every few
//! milliseconds.

pub mod hotkeys;
pub mod notifications;
pub mod status_item;

use std::sync::Mutex;

/// Something the system asked for, for the window to act on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// Bring the main window forward, opening it if it was closed.
    ShowWindow,
    /// Toggle the quick capture window.
    QuickCapture,
    /// Toggle the floating focus panel.
    FocusPanel,
    /// A status item menu entry, by its action.
    Menu(String),
    /// Sync's status moved; `workspace` when a pull changed the data.
    Sync { workspace: bool },
    /// A `takt://` URL opened: the sign-in coming back from the browser.
    Url(String),
}

static QUEUE: Mutex<Vec<Event>> = Mutex::new(Vec::new());

pub fn push(event: Event) {
    if let Ok(mut queue) = QUEUE.lock() {
        queue.push(event);
    }
}

/// What has arrived since the last call.
pub fn drain() -> Vec<Event> {
    QUEUE
        .lock()
        .map(|mut queue| std::mem::take(&mut *queue))
        .unwrap_or_default()
}

/// Posts a notification about the focus block.
pub fn notify(title: &str, subtitle: &str, body: &str, sound: bool) {
    notifications::post("focus", title, subtitle, body, sound, None);
}

/// Hands the process to the bundled `takt` CLI, which is the MCP server:
/// `exec`, not a child, so the client owns its stdio directly. Client
/// configurations naming this executable with `--mcp-server` keep working.
pub fn exec_mcp_server(forwarded: &[String]) -> ! {
    use std::os::unix::process::CommandExt;
    let helper = std::env::current_exe()
        .ok()
        .and_then(|exe| Some(exe.parent()?.parent()?.join("Helpers/takt")))
        .filter(|path| path.is_file())
        .map(|path| path.to_string_lossy().into_owned())
        .or_else(crate::state::agent::helper_path);
    let Some(helper) = helper else {
        eprintln!("takt: no MCP server here (Contents/Helpers/takt is missing)");
        std::process::exit(126);
    };
    let error = std::process::Command::new(&helper).args(forwarded).exec();
    eprintln!("takt: could not start {helper}: {error}");
    std::process::exit(126);
}

/// Makes an `NSString` (autoreleased) from Rust text.
pub(crate) fn ns_string(text: &str) -> *mut objc::runtime::Object {
    let owned = std::ffi::CString::new(text.replace('\0', "")).unwrap_or_default();
    unsafe { msg_send![class!(NSString), stringWithUTF8String: owned.as_ptr()] }
}

/// With a window up the app is an ordinary one, in the Dock and the app
/// switcher; with none it lives in the menu bar alone.
pub fn set_regular(regular: bool) {
    unsafe {
        let app: *mut objc::runtime::Object = msg_send![class!(NSApplication), sharedApplication];
        // NSApplicationActivationPolicyRegular = 0, Accessory = 1.
        let policy: i64 = if regular { 0 } else { 1 };
        let _: bool = msg_send![app, setActivationPolicy: policy];
    }
}

#[link(name = "ServiceManagement", kind = "framework")]
unsafe extern "C" {}

/// Registers or removes the app as a login item (`SMAppService.mainApp`).
/// Only a bundled app can be one; a bare binary gets an error back, said.
pub fn set_launch_at_login(enabled: bool) -> Result<(), String> {
    let Some(class) = objc::runtime::Class::get("SMAppService") else {
        return Err("Login items need macOS 13.".into());
    };
    if bundle_id().is_none() {
        return Err("Only the installed app can open at login.".into());
    }
    unsafe {
        let service: *mut objc::runtime::Object = msg_send![class, mainAppService];
        let mut error: *mut objc::runtime::Object = std::ptr::null_mut();
        let ok: bool = if enabled {
            msg_send![service, registerAndReturnError: &mut error]
        } else {
            msg_send![service, unregisterAndReturnError: &mut error]
        };
        if ok { Ok(()) } else { Err(describe(error)) }
    }
}

/// The running bundle's identifier, when it is a bundle.
pub fn bundle_id() -> Option<String> {
    unsafe {
        let bundle: *mut objc::runtime::Object = msg_send![class!(NSBundle), mainBundle];
        let id: *mut objc::runtime::Object = msg_send![bundle, bundleIdentifier];
        rust_string(id)
    }
}

unsafe fn rust_string(text: *mut objc::runtime::Object) -> Option<String> {
    if text.is_null() {
        return None;
    }
    let utf8: *const std::os::raw::c_char = unsafe { msg_send![text, UTF8String] };
    if utf8.is_null() {
        return None;
    }
    Some(
        unsafe { std::ffi::CStr::from_ptr(utf8) }
            .to_string_lossy()
            .into_owned(),
    )
}

unsafe fn describe(error: *mut objc::runtime::Object) -> String {
    if error.is_null() {
        return "It didn't take.".into();
    }
    let text: *mut objc::runtime::Object = unsafe { msg_send![error, localizedDescription] };
    unsafe { rust_string(text) }.unwrap_or_else(|| "It didn't take.".into())
}
