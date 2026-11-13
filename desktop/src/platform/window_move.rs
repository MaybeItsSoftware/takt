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

/// The window comes to whichever desktop (Space) is in front when it is
/// shown or brought forward, rather than switching to the one it was
/// opened on, and may sit over a full-screen window there.
pub fn follow_active_space(ns_window: usize) {
    // NSWindowCollectionBehaviorMoveToActiveSpace | FullScreenAuxiliary.
    let behavior: u64 = (1 << 1) | (1 << 8);
    unsafe {
        let _: () = msg_send![ns_window as *mut Object, setCollectionBehavior: behavior];
    }
}

/// The window's frame on screen: bottom-left corner, then size.
pub fn frame(ns_window: usize) -> (f64, f64, f64, f64) {
    let frame: NsRect = unsafe { msg_send![ns_window as *mut Object, frame] };
    (frame.origin.x, frame.origin.y, frame.size.x, frame.size.y)
}

/// Whether the user asked the system for less motion.
pub fn reduce_motion() -> bool {
    unsafe {
        let workspace: *mut Object = msg_send![class!(NSWorkspace), sharedWorkspace];
        let reduce: bool = msg_send![workspace, accessibilityDisplayShouldReduceMotion];
        reduce
    }
}

/// Eases the window's frame to `(x, y, width, height)` over `seconds`,
/// through AppKit's animator. Never from inside a GPUI update; see the
/// module notes.
pub fn animate_frame(ns_window: usize, to: (f64, f64, f64, f64), seconds: f64) {
    let rect = NsRect {
        origin: NsPoint { x: to.0, y: to.1 },
        size: NsPoint { x: to.2, y: to.3 },
    };
    unsafe {
        let _: () = msg_send![class!(NSAnimationContext), beginGrouping];
        let context: *mut Object = msg_send![class!(NSAnimationContext), currentContext];
        let _: () = msg_send![context, setDuration: seconds];
        let animator: *mut Object = msg_send![ns_window as *mut Object, animator];
        let _: () = msg_send![animator, setFrame: rect display: true];
        let _: () = msg_send![class!(NSAnimationContext), endGrouping];
    }
}

/// No AppKit shadow under the window: a rounded panel draws its own
/// hairline, and the shadow would ring its corners in grey.
pub fn no_shadow(ns_window: usize) {
    unsafe {
        let _: () = msg_send![ns_window as *mut Object, setHasShadow: false];
    }
}
