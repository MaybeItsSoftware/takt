//! The cutover, once: the first launch of this app as `Takt.app` carries
//! over what the Swift app it replaced kept outside the database. It shares
//! that app's bundle id and signature, so its preferences domain and its
//! keychain items are this app's to read. The database needed nothing: both
//! apps open the same one.

use super::App;

/// The Swift app's preferences domain, now this app's too.
const DOMAIN: &str = "uk.co.maybeitssoftware.takt";

/// One of the Swift app's preferences, as `defaults` prints it.
pub fn swift_default(key: &str) -> Option<String> {
    if std::env::var_os("TAKT_DESKTOP_PREFS").is_some() {
        return None;
    }
    let out = std::process::Command::new("/usr/bin/defaults")
        .args(["read", DOMAIN, key])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8(out.stdout).ok()?.trim().to_string();
    (!text.is_empty()).then_some(text)
}

fn swift_bool(key: &str) -> Option<bool> {
    swift_default(key).map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
}

fn swift_width(key: &str) -> Option<f32> {
    swift_default(key)?
        .parse::<f32>()
        .ok()
        .filter(|w| (120.0..=900.0).contains(w))
}

impl App {
    /// Not in a scratch run, and only the once.
    pub fn adopt_swift_app(&mut self) {
        if self.prefs.adopted_swift || std::env::var_os("TAKT_DESKTOP_PREFS").is_some() {
            return;
        }
        self.prefs.adopted_swift = true;
        let p = &mut self.prefs;
        if let Some(on) = swift_bool("localWorkspaceSidebarVisibleV1") {
            p.sidebar_visible = on;
        }
        if let Some(on) = swift_bool("localWorkspaceRightDockVisibleV1") {
            p.right_dock_visible = on;
        }
        if let Some(on) = swift_bool("localWorkspaceBottomDockVisibleV1") {
            p.bottom_dock_visible = on;
        }
        if let Some(width) = swift_width("localWorkspaceSidebarWidthV1") {
            p.sidebar_width = width;
        }
        if let Some(width) = swift_width("localWorkspaceRightDockWidthV1") {
            p.right_dock_width = width;
        }
        if let Some(client) =
            swift_default("agentClientV1").filter(|c| c == "claude" || c == "codex")
        {
            p.agent_client = client;
        }
        if let Some(on) = swift_bool("googleCalendarIntegrationEnabled") {
            p.google_calendar = on;
        }
        if swift_bool("launchAtLogin") == Some(true) {
            p.launch_at_login = crate::platform::set_launch_at_login(true).is_ok();
        }
        self.prefs.save();
        self.sync.adopt_swift_session();
    }
}
