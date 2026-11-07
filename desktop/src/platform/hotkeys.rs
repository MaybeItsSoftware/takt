//! Global hotkeys through Carbon's `RegisterEventHotKey`, which needs no
//! accessibility permission: show the window, the focus panel, quick
//! capture. `GlobalShortcutManager.swift`.
//!
//! A shortcut is spelled the way the keymap spells keys,
//! `cmd+ctrl+option+shift+f`, and stored in the preferences as that text.

use super::{Event, push};
use std::cell::RefCell;
use std::ffi::c_void;

type OSStatus = i32;
type EventRef = *mut c_void;
type EventTargetRef = *mut c_void;
type EventHandlerRef = *mut c_void;
type EventHandlerCallRef = *mut c_void;
type EventHotKeyRef = *mut c_void;

#[repr(C)]
struct EventTypeSpec {
    event_class: u32,
    event_kind: u32,
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct EventHotKeyID {
    signature: u32,
    id: u32,
}

type Handler = extern "C" fn(EventHandlerCallRef, EventRef, *mut c_void) -> OSStatus;

#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    fn GetApplicationEventTarget() -> EventTargetRef;
    fn InstallEventHandler(
        target: EventTargetRef,
        handler: Handler,
        count: u32,
        types: *const EventTypeSpec,
        user_data: *mut c_void,
        out: *mut EventHandlerRef,
    ) -> OSStatus;
    fn RegisterEventHotKey(
        key_code: u32,
        modifiers: u32,
        id: EventHotKeyID,
        target: EventTargetRef,
        options: u32,
        out: *mut EventHotKeyRef,
    ) -> OSStatus;
    fn UnregisterEventHotKey(hot_key: EventHotKeyRef) -> OSStatus;
    fn GetEventParameter(
        event: EventRef,
        name: u32,
        desired_type: u32,
        actual_type: *mut u32,
        size: usize,
        actual_size: *mut usize,
        data: *mut c_void,
    ) -> OSStatus;
}

const fn four(code: &[u8; 4]) -> u32 {
    (code[0] as u32) << 24 | (code[1] as u32) << 16 | (code[2] as u32) << 8 | code[3] as u32
}

const SIGNATURE: u32 = four(b"CVFK");
const KEYBOARD_CLASS: u32 = four(b"keyb");
const HOT_KEY_PRESSED: u32 = 5;
const DIRECT_OBJECT: u32 = four(b"----");
const HOT_KEY_ID_TYPE: u32 = four(b"hkid");

const CMD: u32 = 0x100;
const SHIFT: u32 = 0x200;
const OPTION: u32 = 0x800;
const CONTROL: u32 = 0x1000;

/// The three things a hotkey can do, by the id Carbon hands back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hotkey {
    ShowWindow = 1,
    QuickCapture = 2,
    FocusPanel = 3,
}

impl Hotkey {
    pub const ALL: [Hotkey; 3] = [Hotkey::ShowWindow, Hotkey::QuickCapture, Hotkey::FocusPanel];

    fn event(self) -> Event {
        match self {
            Hotkey::ShowWindow => Event::ShowWindow,
            Hotkey::QuickCapture => Event::QuickCapture,
            Hotkey::FocusPanel => Event::FocusPanel,
        }
    }
}

/// Carbon's virtual key code for a key as the keymap names it (ANSI).
fn key_code(key: &str) -> Option<u32> {
    let letters = "asdfhgzxcv\0bqweryt";
    if key.len() == 1 {
        let c = key.chars().next()?;
        if let Some(ix) = letters.find(c)
            && c != '\0'
        {
            return Some(ix as u32);
        }
        let code = match c {
            '1' => 18,
            '2' => 19,
            '3' => 20,
            '4' => 21,
            '6' => 22,
            '5' => 23,
            '=' => 24,
            '9' => 25,
            '7' => 26,
            '-' => 27,
            '8' => 28,
            '0' => 29,
            ']' => 30,
            'o' => 31,
            'u' => 32,
            '[' => 33,
            'i' => 34,
            'p' => 35,
            'l' => 37,
            'j' => 38,
            '\'' => 39,
            'k' => 40,
            ';' => 41,
            '\\' => 42,
            ',' => 43,
            '/' => 44,
            'n' => 45,
            'm' => 46,
            '.' => 47,
            '`' => 50,
            _ => return None,
        };
        return Some(code);
    }
    Some(match key {
        "enter" => 36,
        "tab" => 48,
        "space" => 49,
        "delete" | "backspace" => 51,
        "escape" => 53,
        "comma" => 43,
        "f1" => 122,
        "f2" => 120,
        "f3" => 99,
        "f4" => 118,
        "f5" => 96,
        "f6" => 97,
        "f7" => 98,
        "f8" => 100,
        "f9" => 101,
        "f10" => 109,
        "f11" => 103,
        "f12" => 111,
        "left" => 123,
        "right" => 124,
        "down" => 125,
        "up" => 126,
        _ => return None,
    })
}

/// A spelled shortcut as Carbon's key code and modifier mask. A hotkey
/// needs a modifier: a bare letter would be taken from every app.
pub fn parse(spelled: &str) -> Option<(u32, u32)> {
    let mut parts: Vec<&str> = spelled.split('+').collect();
    let key = parts.pop()?;
    let mut modifiers = 0;
    for part in parts {
        modifiers |= match part {
            "cmd" => CMD,
            "ctrl" => CONTROL,
            "option" | "alt" => OPTION,
            "shift" => SHIFT,
            _ => return None,
        };
    }
    if modifiers == 0 || modifiers == SHIFT {
        return None;
    }
    Some((key_code(key)?, modifiers))
}

/// A shortcut as the Mac draws it: `⌃⌥⇧⌘F`.
pub fn display(spelled: &str) -> String {
    let mut parts: Vec<&str> = spelled.split('+').collect();
    let Some(key) = parts.pop() else {
        return String::new();
    };
    let mut out = String::new();
    for (name, glyph) in [("ctrl", "⌃"), ("option", "⌥"), ("shift", "⇧"), ("cmd", "⌘")] {
        if parts.contains(&name) || (name == "option" && parts.contains(&"alt")) {
            out.push_str(glyph);
        }
    }
    out.push_str(&match key {
        "space" => "Space".to_string(),
        "enter" => "↩".to_string(),
        "tab" => "⇥".to_string(),
        "escape" => "⎋".to_string(),
        "delete" => "⌫".to_string(),
        "left" => "←".to_string(),
        "right" => "→".to_string(),
        "up" => "↑".to_string(),
        "down" => "↓".to_string(),
        other => other.to_uppercase(),
    });
    out
}

/// A key pressed in the recorder, spelled for [`parse`]: the modifiers in
/// a fixed order, then the key.
pub fn spell(press: &crate::keys::Press) -> String {
    let mut parts = Vec::new();
    if press.cmd {
        parts.push("cmd");
    }
    if press.ctrl {
        parts.push("ctrl");
    }
    if press.option {
        parts.push("option");
    }
    if press.shift {
        parts.push("shift");
    }
    let key = match press.key.as_str() {
        " " => "space".to_string(),
        "backspace" => "delete".to_string(),
        other => other.to_lowercase(),
    };
    parts.push(&key);
    parts.join("+")
}

extern "C" fn pressed(_: EventHandlerCallRef, event: EventRef, _: *mut c_void) -> OSStatus {
    let mut id = EventHotKeyID::default();
    let status = unsafe {
        GetEventParameter(
            event,
            DIRECT_OBJECT,
            HOT_KEY_ID_TYPE,
            std::ptr::null_mut(),
            std::mem::size_of::<EventHotKeyID>(),
            std::ptr::null_mut(),
            &mut id as *mut EventHotKeyID as *mut c_void,
        )
    };
    if status == 0
        && id.signature == SIGNATURE
        && let Some(hotkey) = Hotkey::ALL.iter().find(|h| **h as u32 == id.id)
    {
        push(hotkey.event());
    }
    0
}

thread_local! {
    static INSTALLED: RefCell<bool> = const { RefCell::new(false) };
    static REGISTERED: RefCell<Vec<EventHotKeyRef>> = const { RefCell::new(Vec::new()) };
}

/// Registers the hotkeys afresh, dropping the ones before. Each is
/// `(which, spelled)`; none spelled is off. Returns the ones the system
/// refused (another app holds them), for diagnostics.
pub fn register(wanted: &[(Hotkey, Option<String>)]) -> Vec<String> {
    INSTALLED.with(|installed| {
        if !*installed.borrow() {
            let spec = EventTypeSpec {
                event_class: KEYBOARD_CLASS,
                event_kind: HOT_KEY_PRESSED,
            };
            unsafe {
                InstallEventHandler(
                    GetApplicationEventTarget(),
                    pressed,
                    1,
                    &spec,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                );
            }
            *installed.borrow_mut() = true;
        }
    });
    REGISTERED.with(|registered| {
        for hot_key in registered.borrow_mut().drain(..) {
            unsafe {
                UnregisterEventHotKey(hot_key);
            }
        }
    });
    let mut refused = Vec::new();
    for (hotkey, spelled) in wanted {
        let Some(spelled) = spelled else { continue };
        let Some((code, modifiers)) = parse(spelled) else {
            refused.push(format!("{spelled} isn't a shortcut"));
            continue;
        };
        let mut out: EventHotKeyRef = std::ptr::null_mut();
        let id = EventHotKeyID {
            signature: SIGNATURE,
            id: *hotkey as u32,
        };
        let status = unsafe {
            RegisterEventHotKey(
                code,
                modifiers,
                id,
                GetApplicationEventTarget(),
                0,
                &mut out,
            )
        };
        if status == 0 {
            REGISTERED.with(|registered| registered.borrow_mut().push(out));
        } else {
            refused.push(format!("{} is taken", display(spelled)));
        }
    }
    refused
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spelled_shortcuts_read_as_carbon_codes() {
        assert_eq!(
            parse("cmd+ctrl+option+shift+f"),
            Some((3, CMD | CONTROL | OPTION | SHIFT))
        );
        assert_eq!(parse("option+space"), Some((49, OPTION)));
        assert_eq!(parse("cmd+d"), Some((2, CMD)));
        assert_eq!(parse("cmd+k"), Some((40, CMD)));
        assert_eq!(parse("f"), None, "a bare key is every app's");
        assert_eq!(parse("shift+f"), None);
        assert_eq!(parse("cmd+nope"), None);
    }

    #[test]
    fn shortcuts_draw_in_the_macs_order() {
        assert_eq!(display("cmd+ctrl+option+shift+f"), "⌃⌥⇧⌘F");
        assert_eq!(display("option+space"), "⌥Space");
    }
}
