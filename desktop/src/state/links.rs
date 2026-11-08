//! What the link picker (`pl`) searches beyond a pasted address: the notes
//! and folders in this Mac's Obsidian vaults, and Google Drive by name when
//! that is switched on (`docs/links.md`, `NativeLinksPlugin.swift`,
//! `ObsidianVaultIndex.swift`, `GoogleDriveSearch.swift`).
//!
//! The vaults are the ones Obsidian lists in its own `obsidian.json`, plus
//! any added in Settings. The index is a walk of their Markdown and canvas
//! files and folders, dot-folders skipped, made each time the picker opens.
//! Drive is searched on a thread a moment after typing stops; its answer is
//! kept against the text it was for, and the picker redrawn.

use super::App;
use crate::google;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Reading Drive file names and types, never contents. A restricted scope,
/// so asked for only once Drive search is switched on.
pub const DRIVE_SCOPE: &str = "https://www.googleapis.com/auth/drive.metadata.readonly";
/// The most entries one vault contributes.
const ENTRY_LIMIT: usize = 20_000;
/// The most vault rows the picker shows for one query.
const SHOWN: usize = 40;
const DRIVE_PAUSE: Duration = Duration::from_millis(350);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub url: String,
    pub title: String,
    pub subtitle: String,
    pub mime_type: Option<String>,
    /// What the query is matched against: a note's path without its
    /// extension, a folder's path, a Drive file's name.
    pub search: String,
}

#[derive(Default)]
struct Drive {
    /// The text last asked about, and the text the answer is for.
    asked: String,
    answered: String,
    found: Vec<Candidate>,
    error: Option<String>,
}

#[derive(Default)]
pub struct Links {
    pub vaults: Vec<PathBuf>,
    pub entries: Vec<Candidate>,
    drive: Arc<Mutex<Drive>>,
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|p| p.into_inner())
}

/// The vault paths in Obsidian's `obsidian.json`, sorted.
pub fn configured_vaults(config: &Path) -> Vec<PathBuf> {
    let Ok(text) = std::fs::read_to_string(config) else {
        return Vec::new();
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = value["vaults"]
        .as_object()
        .into_iter()
        .flat_map(|vaults| vaults.values())
        .filter_map(|vault| vault["path"].as_str().map(PathBuf::from))
        .collect();
    paths.sort();
    paths
}

fn obsidian_config() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default()
        .join("Library/Application Support/obsidian/obsidian.json")
}

/// Every note (`.md`, `.canvas`) and folder in `vault`, dot-entries skipped,
/// as picker rows. The vault's name is its folder's name, as Obsidian's.
pub fn entries(vault: &Path) -> Vec<Candidate> {
    let name = vault
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut out = Vec::new();
    let mut stack = vec![vault.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(read) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut children: Vec<_> = read.flatten().collect();
        children.sort_by_key(|entry| entry.file_name());
        for entry in children {
            if out.len() >= ENTRY_LIMIT {
                return out;
            }
            let file_name = entry.file_name().to_string_lossy().into_owned();
            if file_name.starts_with('.') {
                continue;
            }
            let path = entry.path();
            let Ok(relative) = path.strip_prefix(vault) else {
                continue;
            };
            let relative = relative.to_string_lossy().into_owned();
            let is_folder = entry.file_type().is_ok_and(|t| t.is_dir());
            if is_folder {
                out.push(candidate(&name, &relative, true));
                stack.push(path);
                continue;
            }
            let extension = path
                .extension()
                .map(|e| e.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            if extension == "md" || extension == "canvas" {
                out.push(candidate(&name, &relative, false));
            }
        }
    }
    out
}

fn candidate(vault: &str, path: &str, is_folder: bool) -> Candidate {
    let url = takt_core::links::obsidian_link_url(vault.into(), path.into(), is_folder);
    let described = takt_core::links::link_describe(url.clone(), None, None);
    let search = if is_folder {
        path.to_string()
    } else {
        Path::new(path)
            .with_extension("")
            .to_string_lossy()
            .into_owned()
    };
    Candidate {
        url,
        title: described.title,
        subtitle: described.subtitle,
        mime_type: None,
        search,
    }
}

/// Drive API v3 `files.list` by name, newest first, the bin left out.
fn drive_url(text: &str) -> String {
    use crate::sync::http::escape;
    let quoted = text.replace('\\', "\\\\").replace('\'', "\\'");
    let q = format!("name contains '{quoted}' and trashed = false");
    format!(
        "https://www.googleapis.com/drive/v3/files?q={}&fields={}&pageSize=25&orderBy={}&includeItemsFromAllDrives=true&supportsAllDrives=true",
        escape(&q),
        escape("files(id,name,mimeType,webViewLink)"),
        escape("modifiedTime desc"),
    )
}

/// The files in a `files.list` answer, linked by Drive's viewing address.
fn drive_candidates(body: &str) -> Result<Vec<Candidate>, String> {
    let value: serde_json::Value =
        serde_json::from_str(body).map_err(|_| "Google Drive sent something unreadable.")?;
    Ok(value["files"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|file| {
            let url = file["webViewLink"]
                .as_str()
                .map(str::to_string)
                .or_else(|| {
                    file["id"]
                        .as_str()
                        .map(|id| format!("https://drive.google.com/open?id={id}"))
                })?;
            let name = file["name"].as_str().unwrap_or("Untitled").to_string();
            let mime_type = file["mimeType"].as_str().map(str::to_string);
            let described =
                takt_core::links::link_describe(url.clone(), Some(name.clone()), mime_type.clone());
            Some(Candidate {
                url,
                title: name.clone(),
                subtitle: described.subtitle,
                mime_type,
                search: name,
            })
        })
        .collect())
}

fn search_drive(client_id: &str, text: &str) -> Result<Vec<Candidate>, String> {
    let token =
        google::access_token(client_id, &[DRIVE_SCOPE], "Google Drive").map_err(|e| e.message())?;
    let reply = google::send("GET", &drive_url(text), &token, None).map_err(|e| e.message())?;
    if !reply.ok() {
        return Err(google::failure("Google Drive", &reply).message());
    }
    drive_candidates(&reply.body)
}

impl App {
    /// The vaults and their entries, read afresh: on opening the picker.
    pub fn index_obsidian(&mut self) {
        let mut vaults = configured_vaults(&obsidian_config());
        for extra in &self.prefs.links_obsidian_vaults {
            let path = PathBuf::from(extra);
            if !vaults.contains(&path) {
                vaults.push(path);
            }
        }
        vaults.retain(|vault| vault.is_dir());
        self.links.entries = vaults.iter().flat_map(|vault| entries(vault)).collect();
        self.links.vaults = vaults;
    }

    pub fn drive_search_ready(&self) -> bool {
        self.prefs.links_drive_search
            && self.plugin_on(super::plugins::Plugin::GoogleDrive)
            && google::token()
                .is_some_and(|token| token.scopes.split(' ').any(|s| s == DRIVE_SCOPE))
    }

    /// The picker's rows for `query` from the vaults and Drive: Drive's
    /// answer for this text if it has come, else a search started for it.
    pub fn link_candidates(&self, query: &str) -> (Vec<Candidate>, Vec<Candidate>, Option<String>) {
        let query = query.trim();
        if query.is_empty() {
            return (Vec::new(), Vec::new(), None);
        }
        let notes = rank(&self.links.entries, query);
        if !self.drive_search_ready() {
            return (notes, Vec::new(), None);
        }
        let mut drive = lock(&self.links.drive);
        if drive.answered == query {
            return (notes, drive.found.clone(), drive.error.clone());
        }
        if drive.asked != query {
            drive.asked = query.to_string();
            let state = self.links.drive.clone();
            let client = self.google_client_id();
            let text = query.to_string();
            std::thread::spawn(move || {
                std::thread::sleep(DRIVE_PAUSE);
                if lock(&state).asked != text {
                    return;
                }
                let result = search_drive(&client, &text);
                let mut drive = lock(&state);
                if drive.asked != text {
                    return;
                }
                drive.answered = text;
                match result {
                    Ok(found) => {
                        drive.found = found;
                        drive.error = None;
                    }
                    Err(error) => {
                        drive.found.clear();
                        drive.error = Some(error);
                    }
                }
                drop(drive);
                crate::platform::push(crate::platform::Event::Sync { workspace: false });
            });
        }
        (notes, Vec::new(), None)
    }

    pub fn links_rows(&self) -> Vec<(&'static str, &'static str, &'static str, String)> {
        let vaults = if self.links.vaults.is_empty() {
            let found = configured_vaults(&obsidian_config()).len()
                + self.prefs.links_obsidian_vaults.len();
            format!("{found} vaults")
        } else {
            format!(
                "{} vaults, {} entries",
                self.links.vaults.len(),
                self.links.entries.len()
            )
        };
        let drive = if !self.prefs.links_drive_search {
            "Off".to_string()
        } else if self.google_client_id().is_empty() {
            "On · set a Google client ID".to_string()
        } else if self.drive_search_ready() {
            "On".to_string()
        } else if google::connected() {
            "On · connect Google again to allow it".to_string()
        } else {
            "On · connect Google".to_string()
        };
        vec![
            (
                "links-vault-add",
                "Links",
                "Add an Obsidian vault folder",
                vaults,
            ),
            (
                "links-vault-forget",
                "Links",
                "Forget the vault folders added here",
                self.prefs.links_obsidian_vaults.len().to_string(),
            ),
            (
                "links-drive",
                "Links",
                "Search Google Drive in the link picker",
                drive,
            ),
        ]
    }

    /// Return on a Links row; false when it isn't one.
    pub fn change_links_setting(&mut self, id: &str) -> bool {
        match id {
            "links-vault-add" => {
                self.ask(super::ask::Ask::ObsidianVault);
                return true;
            }
            "links-vault-forget" => {
                self.prefs.links_obsidian_vaults.clear();
                self.prefs.save();
            }
            "links-drive" => {
                self.prefs.links_drive_search = !self.prefs.links_drive_search;
                self.prefs.save();
                if self.prefs.links_drive_search
                    && google::connected()
                    && !self.drive_search_ready()
                {
                    self.message = Some("Choose Connect Google to let Takt search Drive.".into());
                }
            }
            _ => return false,
        }
        self.open_overlay(super::Overlay::Settings);
        true
    }

    pub fn add_obsidian_vault(&mut self, text: &str) {
        let path = text.trim();
        let path = match path.strip_prefix("~/") {
            Some(rest) => std::env::var("HOME")
                .map(|h| format!("{h}/{rest}"))
                .unwrap_or_default(),
            None => path.to_string(),
        };
        if path.is_empty() {
            return;
        }
        if !Path::new(&path).is_dir() {
            self.message = Some(format!("No folder at {path}."));
            return;
        }
        if !self.prefs.links_obsidian_vaults.contains(&path) {
            self.prefs.links_obsidian_vaults.push(path);
            self.prefs.save();
        }
        self.index_obsidian();
    }
}

/// The entries matching `query` best, at most `SHOWN`: every word of the
/// query in the entry's path, in any order, ranked by the palette's fuzzy
/// order among those.
fn rank(entries: &[Candidate], query: &str) -> Vec<Candidate> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    let matching: Vec<&Candidate> = entries
        .iter()
        .filter(|entry| {
            let search = entry.search.to_lowercase();
            words.iter().all(|word| search.contains(word.as_str()))
        })
        .collect();
    let candidates = matching
        .iter()
        .map(|entry| takt_core::palette::PaletteCandidate {
            title: entry.title.clone(),
            detail: entry.search.clone(),
            keys: Vec::new(),
            boost: 0,
        })
        .collect();
    let order = takt_core::palette::palette_rank(query.into(), candidates);
    let mut ranked: Vec<Candidate> = order
        .into_iter()
        .filter_map(|ix| matching.get(ix as usize).map(|c| (*c).clone()))
        .collect();
    // Anything the fuzzy order dropped still matched every word.
    for entry in matching {
        if ranked.len() >= SHOWN {
            break;
        }
        if !ranked.iter().any(|c| c.url == entry.url) {
            ranked.push(entry.clone());
        }
    }
    ranked.truncate(SHOWN);
    ranked
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("takt-links-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_vault_lists_its_notes_and_folders_but_not_dot_folders() {
        let vault = scratch("vault").join("Life");
        std::fs::create_dir_all(vault.join(".obsidian")).unwrap();
        std::fs::create_dir_all(vault.join("Projects")).unwrap();
        std::fs::write(vault.join("Projects/Budget plan.md"), "").unwrap();
        std::fs::write(vault.join("Board.canvas"), "").unwrap();
        std::fs::write(vault.join("photo.png"), "").unwrap();
        std::fs::write(vault.join(".obsidian/app.json"), "").unwrap();
        let found = entries(&vault);
        let searches: Vec<&str> = found.iter().map(|c| c.search.as_str()).collect();
        assert_eq!(searches, vec!["Board", "Projects", "Projects/Budget plan"]);
        assert!(found[2].url.starts_with("obsidian://open?vault=Life"));
        assert!(found[1].url.starts_with("obsidian://search?vault=Life"));
    }

    #[test]
    fn obsidian_lists_its_vaults_in_its_own_file() {
        let dir = scratch("config");
        let config = dir.join("obsidian.json");
        std::fs::write(
            &config,
            r#"{"vaults":{"b":{"path":"/Users/x/Studies","ts":1},"a":{"path":"/Users/x/Life","ts":2}}}"#,
        )
        .unwrap();
        assert_eq!(
            configured_vaults(&config),
            vec![
                PathBuf::from("/Users/x/Life"),
                PathBuf::from("/Users/x/Studies")
            ]
        );
        assert!(configured_vaults(&dir.join("missing.json")).is_empty());
    }

    #[test]
    fn every_word_of_the_query_must_match() {
        let entries = vec![
            candidate("Life", "Projects/Budget plan.md", false),
            candidate("Life", "Journal/2026-11-07.md", false),
            candidate("Life", "Budget", true),
        ];
        let found = rank(&entries, "plan budget");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].search, "Projects/Budget plan");
        assert_eq!(rank(&entries, "budget").len(), 2);
    }

    #[test]
    fn a_drive_answer_becomes_named_rows() {
        let body = r#"{"files":[{"id":"1","name":"Budget","mimeType":"application/vnd.google-apps.spreadsheet","webViewLink":"https://docs.google.com/spreadsheets/d/1/edit"},{"id":"2","name":"Scan"}]}"#;
        let found = drive_candidates(body).unwrap();
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].title, "Budget");
        assert_eq!(found[1].url, "https://drive.google.com/open?id=2");
        assert!(drive_candidates("nope").is_err());
        assert!(drive_url("it's").contains("it%5C%27s"));
    }
}
