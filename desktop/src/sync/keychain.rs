//! What sync keeps outside the database: the sign-in and the Supabase
//! session, in the login keychain as the Swift app kept them, under a
//! service of this app's own (the Swift app's items are readable only by
//! its signature).
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
