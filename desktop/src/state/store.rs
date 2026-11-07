//! The workspace database, through the core.

use std::path::PathBuf;
use std::sync::Arc;
use takt_core::workspace::CoreWorkspace;

pub struct Store {
    pub core: Arc<CoreWorkspace>,
    pub workspace_id: String,
    pub zone: String,
    /// The database's path, for diagnostics.
    pub path: String,
}

impl Store {
    /// `TAKT_DESKTOP_DB`, or the app's own database:
    /// `~/Library/Application Support/Takt/priority.sqlite`. The file kept
    /// the old name; it is stored data, like the keychain's service names.
    pub fn open() -> Result<Self, String> {
        let named = std::env::var_os("TAKT_DESKTOP_DB").map(PathBuf::from);
        if named.is_none() {
            carry_forward(&app_support());
        }
        let path = named.clone().unwrap_or_else(default_database_path);
        if !path.exists() {
            // A database named on purpose must be there; the app's own is
            // made on first launch, as the Swift app made it.
            if named.is_some() {
                return Err(format!("no workspace database at {}", path.display()));
            }
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            }
        }
        let path = path.to_string_lossy().into_owned();
        takt_core::schema::migrate_workspace(path.clone()).map_err(|e| e.to_string())?;
        let core = CoreWorkspace::open(path.clone()).map_err(|e| e.to_string())?;
        let workspace_id = core.bootstrap(now_ms()).map_err(|e| e.to_string())?;
        let zone = iana_time_zone::get_timezone().unwrap_or_else(|_| "UTC".into());
        Ok(Store {
            core,
            workspace_id,
            zone,
            path,
        })
    }
}

pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

pub fn app_support() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    home.join("Library/Application Support/Takt")
}

fn default_database_path() -> PathBuf {
    app_support().join("priority.sqlite")
}

/// What the app was called before, whose folders hold data the Takt folder
/// may lack (`LegacyNameMigration.swift`).
const LEGACY_FOLDERS: [&str; 2] = ["Priority", "Bar Tasker"];
const SQLITE_SIDECARS: [&str; 3] = ["-wal", "-shm", "-journal"];

/// Copies, never moves, whatever an old folder beside `destination` holds
/// that it lacks. A database's sidecars go with it, so it is copied whole
/// rather than torn. Nothing already in the Takt folder is touched.
pub fn carry_forward(destination: &std::path::Path) {
    let Some(base) = destination.parent() else {
        return;
    };
    for name in LEGACY_FOLDERS {
        let Ok(entries) = std::fs::read_dir(base.join(name)) else {
            continue;
        };
        let names: Vec<String> = entries
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        for entry in &names {
            let sidecar = SQLITE_SIDECARS.iter().any(|suffix| {
                entry
                    .strip_suffix(suffix)
                    .is_some_and(|main| names.iter().any(|n| n == main))
            });
            let target = destination.join(entry);
            if sidecar || target.exists() {
                continue;
            }
            let source = base.join(name).join(entry);
            let _ = std::fs::create_dir_all(destination);
            if copy_tree(&source, &target).is_ok() {
                for suffix in SQLITE_SIDECARS {
                    let side = base.join(name).join(format!("{entry}{suffix}"));
                    if side.exists() {
                        let _ = std::fs::copy(&side, destination.join(format!("{entry}{suffix}")));
                    }
                }
            }
        }
    }
}

fn copy_tree(source: &std::path::Path, target: &std::path::Path) -> std::io::Result<()> {
    if source.is_dir() {
        std::fs::create_dir_all(target)?;
        for entry in std::fs::read_dir(source)? {
            let entry = entry?;
            copy_tree(&entry.path(), &target.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        std::fs::copy(source, target).map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_folders_fill_gaps_and_touch_nothing_else() {
        let root = std::env::temp_dir().join(format!("takt-carry-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let old = root.join("Priority");
        let new = root.join("Takt");
        std::fs::create_dir_all(old.join("themes")).unwrap();
        std::fs::create_dir_all(&new).unwrap();
        std::fs::write(old.join("priority.sqlite"), "old db").unwrap();
        std::fs::write(old.join("priority.sqlite-wal"), "wal").unwrap();
        std::fs::write(old.join("themes/a.json"), "{}").unwrap();
        std::fs::write(old.join("kept.json"), "old").unwrap();
        std::fs::write(new.join("kept.json"), "new").unwrap();
        carry_forward(&new);
        assert_eq!(
            std::fs::read_to_string(new.join("priority.sqlite")).unwrap(),
            "old db"
        );
        assert_eq!(
            std::fs::read_to_string(new.join("priority.sqlite-wal")).unwrap(),
            "wal"
        );
        assert!(new.join("themes/a.json").exists());
        assert_eq!(
            std::fs::read_to_string(new.join("kept.json")).unwrap(),
            "new"
        );
        assert!(old.join("priority.sqlite").exists(), "copied, not moved");
        let _ = std::fs::remove_dir_all(&root);
    }
}
