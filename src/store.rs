//! Saved connections, settings and query history. The details are JSON files
//! in the configuration folder of the user. Passwords are in the system
//! keyring, with the connection id as the key. They are never in a file.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use crate::db::Config;

const SERVICE: &str = "sqlsea";

fn dir() -> Option<PathBuf> {
    Some(dirs_next::config_dir()?.join("sqlsea"))
}

fn path() -> Option<PathBuf> {
    Some(dir()?.join("connections.json"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Appearance {
    /// Use the light or dark theme of the desktop.
    #[default]
    Auto,
    Light,
    Dark,
}

impl Appearance {
    pub const ALL: [Appearance; 3] = [Appearance::Auto, Appearance::Light, Appearance::Dark];

    pub fn label(self) -> &'static str {
        match self {
            Appearance::Auto => "Auto",
            Appearance::Light => "Light",
            Appearance::Dark => "Dark",
        }
    }
}

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub appearance: Appearance,
}

pub fn load_settings() -> Settings {
    dir()
        .and_then(|d| std::fs::read(d.join("settings.json")).ok())
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

pub fn save_settings(settings: &Settings) -> Result<(), String> {
    let dir = dir().ok_or("No config directory on this system.")?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let json = serde_json::to_vec_pretty(settings).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("settings.json"), json).map_err(|e| e.to_string())
}

/// Loads the saved connections, with their passwords from the keyring. If an
/// older file has passwords, they move to the keyring and out of the file. The
/// second value is a keyring problem, if there is one.
pub fn load() -> (Vec<Config>, Option<String>) {
    let Some(mut configs) = path()
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|bytes| serde_json::from_slice::<Vec<Config>>(&bytes).ok())
    else {
        return (Vec::new(), None);
    };

    let mut rewrite = false;
    let mut problem = None;
    for config in &mut configs {
        if config.id.is_empty() {
            config.id = new_id();
            rewrite = true;
        }
        if config.password.is_empty() {
            match get_password(&config.id) {
                Ok(password) => config.password = password,
                Err(e) => problem = Some(e),
            }
        } else {
            match set_password(&config.id, &config.password) {
                Ok(()) => rewrite = true,
                Err(e) => problem = Some(e),
            }
        }
        if config.ssh {
            match get_password(&ssh_key(&config.id)) {
                Ok(secret) => config.ssh_secret = secret,
                Err(e) => problem = Some(e),
            }
        }
    }
    // The rewrite removes plain-text passwords from the file. Do it only after
    // all of them are in the keyring.
    if rewrite
        && problem.is_none()
        && let Err(e) = save(&configs)
    {
        problem = Some(e);
    }
    (
        configs,
        problem.map(|e| format!("The system keyring is not available: {e}")),
    )
}

/// Writes the connection details, without passwords. Only the owner can read
/// the file, because it names hosts and users.
pub fn save(configs: &[Config]) -> Result<(), String> {
    let path = path().ok_or("No config directory on this system.")?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_vec_pretty(configs).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| e.to_string())?;
    }
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())
}

/// A new id for a new connection. The id does not change later.
pub fn new_id() -> String {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    format!("{nanos:x}-{}", COUNTER.fetch_add(1, Ordering::Relaxed))
}

fn entry(id: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, id).map_err(|e| e.to_string())
}

fn get_password(id: &str) -> Result<String, String> {
    match entry(id)?.get_password() {
        Ok(password) => Ok(password),
        Err(keyring::Error::NoEntry) => Ok(String::new()),
        Err(e) => Err(e.to_string()),
    }
}

/// Stores the password. If the password is empty, removes it. Some servers
/// accept users without a password.
pub fn set_password(id: &str, password: &str) -> Result<(), String> {
    if password.is_empty() {
        return delete_password(id);
    }
    entry(id)?.set_password(password).map_err(|e| e.to_string())
}

/// The keyring key for the SSH password or passphrase of a connection.
pub fn ssh_key(id: &str) -> String {
    format!("{id}.ssh")
}

pub fn delete_password(id: &str) -> Result<(), String> {
    match entry(id)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

// ---------- query history ----------

/// The most runs kept for each connection. The oldest ones are removed.
const HISTORY_LIMIT: usize = 500;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct HistoryEntry {
    pub sql: String,
    /// Seconds since the Unix epoch.
    pub at: u64,
    pub ok: bool,
    /// The rows returned, or the rows that a statement without results changed.
    pub rows: Option<u64>,
    pub millis: u64,
}

/// The file of a connection in `folder`, such as its history.
fn connection_file(folder: &str, id: &str) -> Option<PathBuf> {
    // Ids are generated, but remove all characters that can name another folder.
    let safe: String = id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect();
    Some(dir()?.join(folder).join(format!("{safe}.json")))
}

fn history_path(id: &str) -> Option<PathBuf> {
    connection_file("history", id)
}

/// A connection's past queries, newest first.
pub fn load_history(id: &str) -> Vec<HistoryEntry> {
    history_path(id)
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// Adds a run at the start of the history. If the same SQL is already in the
/// history, it moves to the start.
pub fn record(history: &mut Vec<HistoryEntry>, entry: HistoryEntry) {
    history.retain(|e| e.sql.trim() != entry.sql.trim());
    history.insert(0, entry);
    history.truncate(HISTORY_LIMIT);
}

/// SQL can be sensitive, so only the owner can read the history file.
pub fn save_history(id: &str, history: &[HistoryEntry]) -> Result<(), String> {
    let path = history_path(id).ok_or("No config directory on this system.")?;
    write_private(
        &path,
        &serde_json::to_vec(history).map_err(|e| e.to_string())?,
    )
}

/// Writes a file that only the owner can read. A crash in the middle of the
/// write leaves the old file.
fn write_private(path: &std::path::Path, json: &[u8]) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| e.to_string())?;
    }
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

/// Removes the history and the diagram places of a connection that is deleted.
pub fn delete_connection_files(id: &str) {
    if let Some(path) = history_path(id) {
        let _ = std::fs::remove_file(path);
    }
    if let Some(path) = connection_file("diagrams", id) {
        let _ = std::fs::remove_file(path);
    }
}

// ---------- diagram places ----------

/// The places of the tables that were moved by hand in the diagram: by schema,
/// then by table. SQLite uses the schema "".
pub type Places =
    std::collections::BTreeMap<String, std::collections::BTreeMap<String, (f32, f32)>>;

pub fn load_places(id: &str) -> Places {
    connection_file("diagrams", id)
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

pub fn save_places(id: &str, places: &Places) -> Result<(), String> {
    let path = connection_file("diagrams", id).ok_or("No config directory on this system.")?;
    write_private(
        &path,
        &serde_json::to_vec(places).map_err(|e| e.to_string())?,
    )
}

pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(sql: &str) -> HistoryEntry {
        HistoryEntry {
            sql: sql.into(),
            at: 1,
            ok: true,
            rows: Some(1),
            millis: 2,
        }
    }

    #[test]
    fn history_moves_repeats_to_the_front_and_stays_bounded() {
        let mut h = Vec::new();
        record(&mut h, entry("select 1"));
        record(&mut h, entry("select 2"));
        record(&mut h, entry(" select 1 "));
        assert_eq!(h.len(), 2);
        assert_eq!(h[0].sql, " select 1 ");
        for i in 0..HISTORY_LIMIT + 10 {
            record(&mut h, entry(&format!("select {i}")));
        }
        assert_eq!(h.len(), HISTORY_LIMIT);
    }

    #[test]
    fn history_ids_cannot_escape_the_folder() {
        let path = history_path("../../etc/passwd").unwrap();
        assert!(path.file_name().unwrap().to_str().unwrap() == "etcpasswd.json");
    }
}
