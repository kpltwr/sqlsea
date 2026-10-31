//! Saved connections. The details live as JSON in the user's config
//! directory; passwords live in the system keyring (the Secret Service, such as
//! GNOME Keyring), keyed by each connection's id, and never touch the file.

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
    /// Follow the desktop's light or dark preference.
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

/// Loads saved connections with their passwords filled in from the keyring.
/// Passwords found in an older file are moved into the keyring and dropped
/// from the file. The second value describes a keyring problem, if any.
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
    }
    // Rewriting is what drops plain-text passwords from the file, so only do
    // it once every one of them is safely in the keyring.
    if rewrite
        && problem.is_none()
        && let Err(e) = save(&configs)
    {
        problem = Some(e);
    }
    (
        configs,
        problem.map(|e| format!("The system keyring could not be used: {e}")),
    )
}

/// Writes the connection details. Passwords are skipped by `Config`'s
/// serializer; the file is still owner-only, since it names hosts and users.
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

/// A fresh, stable id for a new connection.
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

/// Stores the password, or removes it when empty (for servers that trust the
/// user without one).
pub fn set_password(id: &str, password: &str) -> Result<(), String> {
    if password.is_empty() {
        return delete_password(id);
    }
    entry(id)?.set_password(password).map_err(|e| e.to_string())
}

pub fn delete_password(id: &str) -> Result<(), String> {
    match entry(id)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}
