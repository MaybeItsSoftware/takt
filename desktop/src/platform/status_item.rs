//! The menu bar item: what is next, or the running block and its clock,
//! and a menu of the day and the ways in. `MenuBarController.swift`.
//!
//! The window hands it a [`Snapshot`] whenever what it says may have
//! changed; a click builds the menu from the latest one there and then, so
//! the menu is never staler than the last snapshot. Picking an entry queues
//! its action as an [`Event::Menu`].

use super::{Event, ns_string, push};
use objc::declare::ClassDecl;
use objc::runtime::{Class, NO, Object, Sel, YES};
use std::cell::RefCell;
use std::sync::Once;

/// One row of the menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    /// A greyed heading or reading.
    Label {
        title: String,
        indent: i64,
    },
    /// Something to do, by its action id.
    Item {
        title: String,
        action: String,
        indent: i64,
        checked: bool,
        tooltip: Option<String>,
    },
    Separator,
}

/// What the item shows and what its menu holds.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Snapshot {
    /// Empty for the icon alone.
    pub title: String,
    pub tooltip: String,
    /// Drawn in orange: the clock overrun, or the next thing overdue.
    pub warn: bool,
    /// A clock in the title: digits of one width so it doesn't jiggle.
    pub monospaced: bool,
    pub visible: bool,
    pub menu: Vec<Entry>,
}

thread_local! {
    static ITEM: RefCell<Option<*mut Object>> = const { RefCell::new(None) };
    static TARGET: RefCell<Option<*mut Object>> = const { RefCell::new(None) };
    static SHOWN: RefCell<Option<Snapshot>> = const { RefCell::new(None) };
    /// The actions of the menu last built, by item tag.
    static ACTIONS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

fn target_class() -> &'static Class {
    static REGISTER: Once = Once::new();
    REGISTER.call_once(|| {
        let mut decl = ClassDecl::new("TaktStatusTarget", class!(NSObject))
            .expect("TaktStatusTarget is declared once");
        unsafe {
            decl.add_method(
                sel!(clicked:),
                clicked as extern "C" fn(&Object, Sel, *mut Object),
            );
            decl.add_method(
                sel!(pick:),
                pick as extern "C" fn(&Object, Sel, *mut Object),
            );
        }
        decl.register();
    });
    Class::get("TaktStatusTarget").expect("registered above")
}

/// Shows `snapshot`, creating the item the first time.
pub fn update(snapshot: Snapshot) {
    let unchanged = SHOWN.with(|shown| shown.borrow().as_ref() == Some(&snapshot));
    if unchanged {
        return;
    }
    let item = ITEM.with(|item| *item.borrow());
    let item = match item {
        Some(item) => item,
        None => unsafe { create() },
    };
    unsafe {
        let _: () = msg_send![item, setVisible: if snapshot.visible { YES } else { NO }];
        let button: *mut Object = msg_send![item, button];
        if snapshot.title.is_empty() {
            let image: *mut Object = msg_send![class!(NSImage),
                imageWithSystemSymbolName: ns_string("checkmark.square")
                accessibilityDescription: ns_string("Open Takt")];
            if !image.is_null() {
                let _: () = msg_send![image, setTemplate: YES];
            }
            let _: () = msg_send![button, setImage: image];
            let _: () = msg_send![button, setTitle: ns_string("")];
            // NSImageOnly
            let _: () = msg_send![button, setImagePosition: 1u64];
        } else {
            let _: () = msg_send![button, setImage: std::ptr::null_mut::<Object>()];
            // NSNoImage
            let _: () = msg_send![button, setImagePosition: 0u64];
            let _: () = msg_send![button, setAttributedTitle: attributed(&snapshot)];
        }
        let _: () = msg_send![button, setToolTip: ns_string(&snapshot.tooltip)];
    }
    SHOWN.with(|shown| *shown.borrow_mut() = Some(snapshot));
}

unsafe fn create() -> *mut Object {
    unsafe {
        let bar: *mut Object = msg_send![class!(NSStatusBar), systemStatusBar];
        // NSVariableStatusItemLength
        let item: *mut Object = msg_send![bar, statusItemWithLength: -1.0f64];
        let _: *mut Object = msg_send![item, retain];
        let target: *mut Object = msg_send![target_class(), new];
        let button: *mut Object = msg_send![item, button];
        let _: () = msg_send![button, setTarget: target];
        let _: () = msg_send![button, setAction: sel!(clicked:)];
        // NSEventMaskLeftMouseUp | NSEventMaskRightMouseUp
        let _: i64 = msg_send![button, sendActionOn: (1u64 << 2) | (1u64 << 4)];
        ITEM.with(|cell| *cell.borrow_mut() = Some(item));
        TARGET.with(|cell| *cell.borrow_mut() = Some(target));
        item
    }
}

unsafe fn attributed(snapshot: &Snapshot) -> *mut Object {
    unsafe {
        let size: f64 = msg_send![class!(NSFont), systemFontSize];
        let font: *mut Object = if snapshot.monospaced {
            msg_send![class!(NSFont), monospacedDigitSystemFontOfSize: size weight: 0.0f64]
        } else {
            msg_send![class!(NSFont), menuBarFontOfSize: 0.0f64]
        };
        let attributes: *mut Object = msg_send![class!(NSMutableDictionary), dictionary];
        let _: () = msg_send![attributes, setObject: font forKey: ns_string("NSFont")];
        if snapshot.warn {
            let orange: *mut Object = msg_send![class!(NSColor), systemOrangeColor];
            let _: () = msg_send![attributes, setObject: orange forKey: ns_string("NSColor")];
        }
        let text: *mut Object = msg_send![class!(NSAttributedString), alloc];
        let text: *mut Object =
            msg_send![text, initWithString: ns_string(&snapshot.title) attributes: attributes];
        let _: *mut Object = msg_send![text, autorelease];
        text
    }
}

/// Either button opens the menu, built from the latest snapshot.
extern "C" fn clicked(this: &Object, _: Sel, _: *mut Object) {
    let Some(entries) = SHOWN.with(|shown| shown.borrow().as_ref().map(|s| s.menu.clone())) else {
        return;
    };
    let Some(item) = ITEM.with(|item| *item.borrow()) else {
        return;
    };
    let mut actions = Vec::new();
    unsafe {
        let menu: *mut Object = msg_send![class!(NSMenu), new];
        let _: () = msg_send![menu, setAutoenablesItems: NO];
        for entry in &entries {
            let row: *mut Object = match entry {
                Entry::Separator => msg_send![class!(NSMenuItem), separatorItem],
                Entry::Label { title, indent } => {
                    let row = menu_item(title, None);
                    let _: () = msg_send![row, setEnabled: NO];
                    let _: () = msg_send![row, setIndentationLevel: *indent];
                    row
                }
                Entry::Item {
                    title,
                    action,
                    indent,
                    checked,
                    tooltip,
                } => {
                    let row = menu_item(title, Some(this));
                    let _: () = msg_send![row, setTag: actions.len() as i64];
                    actions.push(action.clone());
                    let _: () = msg_send![row, setIndentationLevel: *indent];
                    if *checked {
                        let _: () = msg_send![row, setState: 1i64];
                    }
                    if let Some(tip) = tooltip {
                        let _: () = msg_send![row, setToolTip: ns_string(tip)];
                    }
                    row
                }
            };
            let _: () = msg_send![menu, addItem: row];
        }
        ACTIONS.with(|cell| *cell.borrow_mut() = actions);
        let button: *mut Object = msg_send![item, button];
        let _: () = msg_send![item, setMenu: menu];
        let _: () = msg_send![button, performClick: std::ptr::null_mut::<Object>()];
        let _: () = msg_send![item, setMenu: std::ptr::null_mut::<Object>()];
        let _: () = msg_send![menu, release];
    }
}

unsafe fn menu_item(title: &str, target: Option<&Object>) -> *mut Object {
    unsafe {
        let row: *mut Object = msg_send![class!(NSMenuItem), alloc];
        let action = if target.is_some() {
            sel!(pick:)
        } else {
            Sel::from_ptr(std::ptr::null())
        };
        let row: *mut Object = msg_send![row,
            initWithTitle: ns_string(title)
            action: action
            keyEquivalent: ns_string("")];
        if let Some(target) = target {
            let _: () = msg_send![row, setTarget: target];
        }
        let _: *mut Object = msg_send![row, autorelease];
        row
    }
}

extern "C" fn pick(_: &Object, _: Sel, sender: *mut Object) {
    let tag: i64 = unsafe { msg_send![sender, tag] };
    let action = ACTIONS.with(|cell| cell.borrow().get(tag as usize).cloned());
    if let Some(action) = action {
        push(Event::Menu(action));
    }
}

/// Clips a title so the clock beside it keeps its place.
pub fn clip(title: &str, limit: usize) -> String {
    if title.chars().count() <= limit {
        return title.to_string();
    }
    let kept: String = title.chars().take(limit.saturating_sub(1)).collect();
    format!("{}…", kept.trim_end())
}

#[cfg(test)]
mod tests {
    use super::clip;

    #[test]
    fn long_titles_are_clipped_with_an_ellipsis() {
        assert_eq!(clip("Short", 10), "Short");
        assert_eq!(clip("Write the release notes", 10), "Write the…");
    }
}
