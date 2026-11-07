//! The Checkvist import: the account's lists the workspace lacks, copied in
//! with their tasks (`CheckvistAccount.swift`, `WorkspaceViewModel+Import`).
//! Requests and answers are the core's (`takt_core::checkvist`); this sends
//! them. The credentials are the keychain's, or the CLI's config file's
//! when there are none, so a Mac set up for the `takt` command needs nothing
//! more.
//!
//! Which remote list became which local one is kept in `checkvist.json`
//! beside the preferences: a list imported once is not imported again
//! unless it was deleted here.

use super::App;
use crate::sync::{http, keychain};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use takt_core::checkvist::{self, CheckvistCall};
use takt_core::workspace::CoreWorkspace;

const SOURCE_SYSTEM: &str = "checkvist";
const USER_AGENT: &str = "TaktDesktop/0.1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Credentials {
    pub username: String,
    pub remote_key: String,
}

fn mapping_path() -> PathBuf {
    std::env::var_os("TAKT_DESKTOP_PREFS")
        .map(PathBuf::from)
        .map(|p| p.with_file_name("checkvist.json"))
        .unwrap_or_else(|| super::store::app_support().join("checkvist.json"))
}

fn load_mapping() -> HashMap<String, String> {
    std::fs::read(mapping_path())
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// The keychain's, else the `takt` command's own (`~/.config/takt`).
pub fn credentials() -> Option<Credentials> {
    keychain::load::<Credentials>("checkvist").or_else(|| {
        if std::env::var_os("TAKT_DESKTOP_PREFS").is_some() {
            return None;
        }
        let home = std::env::var_os("HOME").map(PathBuf::from)?;
        ["takt", "priority"].iter().find_map(|dir| {
            let bytes = std::fs::read(home.join(".config").join(dir).join("config.json")).ok()?;
            let value: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
            let text = |key: &str| {
                value
                    .get(key)
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
            };
            Some(Credentials {
                username: text("username")?,
                remote_key: text("remote_key")?,
            })
        })
    })
}

fn send(call: CheckvistCall, token: Option<String>) -> Result<String, String> {
    let request = checkvist::checkvist_request(
        checkvist::DEFAULT_BASE_URL.into(),
        call,
        token,
        USER_AGENT.into(),
    )
    .map_err(|e| e.to_string())?;
    let headers: Vec<(&str, &str)> = request
        .headers
        .iter()
        .map(|h| (h.name.as_str(), h.value.as_str()))
        .collect();
    let reply = http::request(
        &request.method,
        &request.url,
        &headers,
        request.body.as_deref(),
        Duration::from_secs(60),
    )?;
    if reply.status == 401 {
        return Err("Checkvist refused the email or API key.".into());
    }
    if !reply.ok() {
        return Err(format!("Checkvist answered {}.", reply.status));
    }
    Ok(reply.body)
}

/// Signs in, then copies each list not yet here. Returns what it says.
fn import(
    core: &CoreWorkspace,
    workspace_id: &str,
    existing: &[String],
    credentials: &Credentials,
) -> Result<String, String> {
    let body = send(
        CheckvistCall::Login {
            username: credentials.username.clone(),
            remote_key: credentials.remote_key.clone(),
        },
        None,
    )?;
    let token = checkvist::checkvist_login_token(body)
        .ok_or_else(|| "Checkvist refused the email or API key.".to_string())?;
    let lists = checkvist::checkvist_lists(send(CheckvistCall::Lists, Some(token.clone()))?)
        .map_err(|e| e.to_string())?;
    let mut mapping = load_mapping();
    let mut imported = Vec::new();
    for list in lists.iter().filter(|l| l.archived != Some(true)) {
        let remote = list.id.to_string();
        if mapping
            .get(&remote)
            .is_some_and(|local| existing.contains(local))
        {
            continue;
        }
        let tasks = send(
            CheckvistCall::Tasks {
                list_id: remote.clone(),
                with_notes: true,
            },
            Some(token.clone()),
        )?;
        let tasks = checkvist::checkvist_tasks(tasks, true).map_err(|e| e.to_string())?;
        let seeds = checkvist::checkvist_import_seeds(format!("checkvist:{remote}:"), tasks);
        let now = super::store::now_ms();
        let local = if seeds.is_empty() {
            core.create_list(workspace_id.into(), list.name.clone(), None, now)
                .map_err(|e| e.to_string())?
                .id
        } else {
            match core
                .import_tasks(
                    workspace_id.into(),
                    list.name.clone(),
                    SOURCE_SYSTEM.into(),
                    seeds,
                    now,
                )
                .map_err(|e| e.to_string())?
            {
                Some(outcome) => outcome.list_id,
                None => continue,
            }
        };
        mapping.insert(remote, local);
        imported.push(list.name.clone());
    }
    if let Ok(bytes) = serde_json::to_vec_pretty(&mapping) {
        let _ = std::fs::write(mapping_path(), bytes);
    }
    Ok(match imported.len() {
        0 => "Every Checkvist list is already here.".into(),
        1 => format!("Imported “{}” from Checkvist.", imported[0]),
        n => format!("Imported {n} lists from Checkvist."),
    })
}

impl App {
    pub fn checkvist_connected(&self) -> Option<String> {
        credentials().map(|c| c.username)
    }

    /// Saves the credentials and imports straight away.
    pub fn connect_checkvist(&mut self, email: &str, key: &str) {
        let credentials = Credentials {
            username: email.trim().into(),
            remote_key: key.trim().into(),
        };
        if credentials.username.is_empty() || credentials.remote_key.is_empty() {
            self.message = Some("Checkvist needs both the email and the API key.".into());
            return;
        }
        if let Err(error) = keychain::save("checkvist", &credentials) {
            self.message = Some(error);
            return;
        }
        self.import_checkvist();
    }

    pub fn disconnect_checkvist(&mut self) {
        keychain::clear("checkvist");
        self.message = Some("Checkvist disconnected; its lists stay here.".into());
    }

    /// Copies in the lists the workspace lacks, on a thread; the window
    /// hears of it through a sync event.
    pub fn import_checkvist(&mut self) {
        let Some(credentials) = credentials() else {
            self.message = Some("Connect Checkvist first.".into());
            return;
        };
        let core: Arc<CoreWorkspace> = self.store.core.clone();
        let workspace_id = self.store.workspace_id.clone();
        let existing: Vec<String> = self.lists.iter().map(|l| l.id.clone()).collect();
        self.message = Some("Importing from Checkvist…".into());
        std::thread::spawn(move || {
            let said =
                import(&core, &workspace_id, &existing, &credentials).unwrap_or_else(|error| error);
            crate::sync::announce(&said, true);
        });
    }
}
