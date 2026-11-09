//! What sync keeps outside the database: the sign-in and the Supabase
//! session, in the login keychain as the Swift app kept them, under a
//! service of this app's own. The Swift app's items are readable now that
//! this app carries its signature; `swift_refresh_token` reads the one
//! sign-in carries over.
//!
//! A scratch run (`TAKT_DESKTOP_PREFS` set) keeps them in a file beside its
//! preferences instead, so a smoke test never touches the keychain. So does
//! any binary that isn't the installed bundle (`cargo run`, `cargo test`, a
//! smoke run): an unbundled build is signed afresh on every link, so macOS
//! sees a new app each time and asks for the keychain again.

use serde::Serialize;
use serde::de::DeserializeOwned;
use std::path::PathBuf;

const SERVICE: &str = "uk.co.maybeitssoftware.takt.desktop.sync";

fn scratch(account: &str) -> Option<PathBuf> {
    if let Some(prefs) = std::env::var_os("TAKT_DESKTOP_PREFS") {
        return Some(PathBuf::from(prefs).with_file_name(format!("sync-{account}.json")));
    }
    if bundled() {
        return None;
    }
    let dir = std::env::temp_dir().join("takt-desktop-unbundled");
    let _ = std::fs::create_dir_all(&dir);
    Some(dir.join(format!("sync-{account}.json")))
}

/// Whether this is the bundled app (`….app/Contents/MacOS/…`), the only
/// build whose signature stays the same from one install to the next.
fn bundled() -> bool {
    !cfg!(test)
        && std::env::var_os("TAKT_DESKTOP_SMOKE").is_none()
        && std::env::current_exe()
            .map(|exe| exe.to_string_lossy().contains(".app/Contents/MacOS/"))
            .unwrap_or(false)
}

/// Whether reads and writes go to a scratch file rather than the keychain.
pub fn is_scratch() -> bool {
    std::env::var_os("TAKT_DESKTOP_PREFS").is_some() || !bundled()
}

pub fn load<T: DeserializeOwned>(account: &str) -> Option<T> {
    let bytes = match scratch(account) {
        Some(path) => std::fs::read(path).ok()?,
        None => security_framework::passwords::get_generic_password(SERVICE, account).ok()?,
    };
    serde_json::from_slice(&bytes).ok()
}

pub fn save<T: Serialize>(account: &str, value: &T) -> Result<(), String> {
    let bytes = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    match scratch(account) {
        Some(path) => write_private(&path, &bytes).map_err(|e| e.to_string()),
        None => security_framework::passwords::set_generic_password(SERVICE, account, &bytes)
            .map_err(|e| format!("The keychain refused it: {e}")),
    }
}

/// A scratch file only its owner can read: it holds tokens.
fn write_private(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?
        .write_all(bytes)
}

pub fn clear(account: &str) {
    match scratch(account) {
        Some(path) => {
            let _ = std::fs::remove_file(path);
        }
        None => {
            let _ = security_framework::passwords::delete_generic_password(SERVICE, account);
        }
    }
}

/// The refresh token in the Swift app's Supabase session (supabase-swift's
/// keychain storage), wherever in the stored JSON it sits.
pub fn swift_refresh_token() -> Option<String> {
    if is_scratch() {
        return None;
    }
    let bytes = security_framework::passwords::get_generic_password(
        "uk.co.maybeitsadam.priority.supabase",
        "supabase.auth.token",
    )
    .ok()?;
    find_refresh_token(&serde_json::from_slice(&bytes).ok()?)
}

fn find_refresh_token(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::Object(map) => map
            .get("refresh_token")
            .or_else(|| map.get("refreshToken"))
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .or_else(|| map.values().find_map(find_refresh_token)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_refresh_token_is_found_however_it_is_wrapped() {
        let flat = serde_json::json!({"access_token": "a", "refresh_token": "r1"});
        let nested = serde_json::json!({"session": {"refreshToken": "r2"}, "expirationDate": 1.5});
        assert_eq!(super::find_refresh_token(&flat).as_deref(), Some("r1"));
        assert_eq!(super::find_refresh_token(&nested).as_deref(), Some("r2"));
        assert_eq!(super::find_refresh_token(&serde_json::json!({})), None);
    }
}
