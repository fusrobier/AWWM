//! Well-known file locations.
//!
//! The folders are named after the bundle identifier so the NSIS uninstaller's
//! "delete application data" option removes them, and so data never lives inside the
//! per-user installation folder (`%LOCALAPPDATA%\AWWM`).
//!
//! * settings: `%APPDATA%\dev.awwm.windowmanager\settings.json` (roams with the profile)
//! * session + logs: `%LOCALAPPDATA%\dev.awwm.windowmanager\` (machine-specific)

use std::path::PathBuf;

/// Folder name under `%APPDATA%` / `%LOCALAPPDATA%` (the Tauri bundle identifier).
pub const DATA_FOLDER: &str = "dev.awwm.windowmanager";

/// Resolved application directories.
#[derive(Debug, Clone)]
pub struct AppPaths {
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    pub log_dir: PathBuf,
}

impl AppPaths {
    /// Resolves the directories from the standard Windows environment variables.
    ///
    /// # Errors
    /// When `APPDATA` / `LOCALAPPDATA` are not set (non-interactive service contexts).
    pub fn resolve() -> Result<Self, String> {
        let appdata = std::env::var_os("APPDATA").ok_or("APPDATA is not set")?;
        let local = std::env::var_os("LOCALAPPDATA").ok_or("LOCALAPPDATA is not set")?;
        let config_dir = PathBuf::from(appdata).join(DATA_FOLDER);
        let data_dir = PathBuf::from(local).join(DATA_FOLDER);
        let log_dir = data_dir.join("logs");
        Ok(Self {
            config_dir,
            data_dir,
            log_dir,
        })
    }

    /// `settings.json`.
    #[must_use]
    pub fn settings_file(&self) -> PathBuf {
        self.config_dir.join("settings.json")
    }

    /// `session.json` (crash recovery of managed windows).
    #[must_use]
    pub fn session_file(&self) -> PathBuf {
        self.data_dir.join("session.json")
    }

    /// Creates the directories if needed.
    ///
    /// # Errors
    /// Any I/O error.
    pub fn ensure(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.config_dir)?;
        std::fs::create_dir_all(&self.log_dir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folders_use_the_bundle_identifier() {
        let p = AppPaths::resolve().expect("paths");
        assert!(
            p.settings_file()
                .ends_with(r"dev.awwm.windowmanager\settings.json")
        );
        assert!(
            p.session_file()
                .ends_with(r"dev.awwm.windowmanager\session.json")
        );
        assert!(p.log_dir.ends_with(r"dev.awwm.windowmanager\logs"));
        assert_ne!(p.config_dir, p.data_dir);
    }
}
