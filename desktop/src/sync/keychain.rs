//! What sync keeps outside the database: the sign-in and the Supabase
//! session, in the login keychain as the Swift app kept them, under a
//! service of this app's own. The Swift app's items are readable now that
//! this app carries its signature; `swift_refresh_token` reads the one
//! sign-in carries over.
//!
//! A scratch run (`TAKT_DESKTOP_PREFS` set) keeps them in a file beside its
//! preferences instead, so a smoke test never touches the keychain.

use serde::Serialize;
use serde::de::DeserializeOwned;
use std::path::PathBuf;

const SERVICE: &str = "uk.co.maybeitssoftware.takt.desktop.sync";

fn scratch(account: &str) -> Option<PathBuf> {
    std::env::var_os("TAKT_DESKTOP_PREFS")
        .map(PathBuf::from)
        .map(|prefs| prefs.with_file_name(format!("sync-{account}.json")))
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
        Some(path) => std::fs::write(path, bytes).map_err(|e| e.to_string()),
        None => security_framework::passwords::set_generic_password(SERVICE, account, &bytes)
            .map_err(|e| format!("The keychain refused it: {e}")),
    }
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
    if std::env::var_os("TAKT_DESKTOP_PREFS").is_some() {
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
