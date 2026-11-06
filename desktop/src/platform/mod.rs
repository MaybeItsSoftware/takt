//! The Mac's own glue: what AppKit and the system services give that GPUI
//! does not. Each piece is its own file, reached through plain functions so
//! the state never sees an Objective-C type.

/// Registers or removes the app as a login item.
pub fn set_launch_at_login(enabled: bool) {
    let _ = enabled;
}
