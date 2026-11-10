//! Dragging a window that has no title bar: GPUI on the Mac leaves
//! `start_window_move` empty, so the pill moves itself by its frame origin.
//!
//! Moving an `NSWindow` tells GPUI (`windowDidMove:`), which then updates
//! the app; doing that from inside one of GPUI's own handlers would borrow
//! the app twice and abort. So these are only ever called from a task
//! spawned onto the main thread, never from inside an update, and
//! everything is in AppKit's coordinates (origin bottom left) so the
//! pointer and the frame agree.

use objc::runtime::Object;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct NsPoint {
    x: f64,
    y: f64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct NsRect {
    origin: NsPoint,
    size: NsPoint,
}

/// The `NSWindow` behind a GPUI window, as an address to hold on to.
pub fn ns_window(window: &gpui::Window) -> Option<usize> {
    let handle = HasWindowHandle::window_handle(window).ok()?;
    let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
        return None;
    };
    let view = appkit.ns_view.as_ptr() as *mut Object;
    let ns_window: *mut Object = unsafe { msg_send![view, window] };
    (!ns_window.is_null()).then_some(ns_window as usize)
}

/// The pointer on screen.
pub fn mouse_location() -> (f64, f64) {
    let point: NsPoint = unsafe { msg_send![class!(NSEvent), mouseLocation] };
    (point.x, point.y)
}

/// Where the window's bottom-left corner is on screen.
pub fn origin(ns_window: usize) -> (f64, f64) {
    let frame: NsRect = unsafe { msg_send![ns_window as *mut Object, frame] };
    (frame.origin.x, frame.origin.y)
}

/// Moves the window's bottom-left corner to `(x, y)`. Never from inside a
/// GPUI update; see the module notes.
pub fn set_origin(ns_window: usize, x: f64, y: f64) {
    let point = NsPoint { x, y };
    unsafe {
        let _: () = msg_send![ns_window as *mut Object, setFrameOrigin: point];
    }
}

/// Shows and hides the window at once, with none of AppKit's fade: a panel
/// that is summoned and dismissed by a key should be there, then gone.
pub fn no_animation(ns_window: usize) {
    // NSWindowAnimationBehaviorNone.
    let none: i64 = 2;
    unsafe {
        let _: () = msg_send![ns_window as *mut Object, setAnimationBehavior: none];
    }
}
