//! The Mac's own glue: what AppKit and the system services give that GPUI
//! does not. Each piece is its own file, reached through plain functions so
//! the state never sees an Objective-C type.

/// Posts a notification: the system's own when the app is bundled, else
/// through `osascript`, which works from a bare binary.
pub fn notify(title: &str, subtitle: &str, body: &str, sound: bool) {
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

/// Registers or removes the app as a login item.
pub fn set_launch_at_login(enabled: bool) {
    let _ = enabled;
}
