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
        let path = std::env::var_os("TAKT_DESKTOP_DB")
            .map(PathBuf::from)
            .unwrap_or_else(default_database_path);
        if !path.exists() {
            return Err(format!("no workspace database at {}", path.display()));
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
