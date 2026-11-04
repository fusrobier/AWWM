//! settings_manager: loads and saves `settings.json`.
//!
//! Saving is atomic (temporary file + rename) and keeps the previous version as
//! `settings.json.bak`. A file that cannot be parsed is moved aside to
//! `settings.corrupt-<unix time>.json` and defaults are used, with a warning surfaced in
//! the UI, so a damaged file never prevents AWWM from starting.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use awwm_core::settings::Settings;
use awwm_engine::session::write_atomic;

/// Result of loading settings.
#[derive(Debug)]
pub struct Loaded {
    pub settings: Settings,
    /// `true` when no settings file existed (first run).
    pub first_run: bool,
    /// User-facing warning when the file was unreadable.
    pub warning: Option<String>,
}

/// Settings persistence at a fixed path.
#[derive(Debug, Clone)]
pub struct SettingsStore {
    path: PathBuf,
}

impl SettingsStore {
    #[must_use]
    pub const fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// The settings file path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Loads, migrates and sanitizes settings.
    #[must_use]
    pub fn load(&self) -> Loaded {
        let text = match fs::read_to_string(&self.path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Loaded {
                    settings: Settings::default(),
                    first_run: true,
                    warning: None,
                };
            }
            Err(e) => {
                tracing::error!(error = %e, path = %self.path.display(), "cannot read settings");
                return Loaded {
                    settings: Settings::default(),
                    first_run: false,
                    warning: Some(format!(
                        "Settings could not be read ({e}); defaults are in use."
                    )),
                };
            }
        };
        match serde_json::from_str::<Settings>(&text) {
            Ok(s) => Loaded {
                settings: s.sanitized(),
                first_run: false,
                warning: None,
            },
            Err(e) => {
                let stamp = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_or(0, |d| d.as_secs());
                let aside = self
                    .path
                    .with_file_name(format!("settings.corrupt-{stamp}.json"));
                let moved = fs::rename(&self.path, &aside).is_ok();
                let location = if moved {
                    format!(" The damaged file was saved as {}.", aside.display())
                } else {
                    String::new()
                };
                // Prefer the last good backup over defaults; write it back so the next save's
                // backup step does not overwrite it with defaults.
                if let Some(backup) = self.read_backup() {
                    tracing::error!(error = %e, moved_to = %aside.display(), "settings file is invalid; restored the backup");
                    // Write directly, without the usual backup step: that step would copy
                    // the damaged file over the good backup if moving it aside failed.
                    let written = serde_json::to_vec_pretty(&backup)
                        .map_err(std::io::Error::other)
                        .and_then(|json| write_atomic(&self.path, &json));
                    if let Err(err) = written {
                        tracing::warn!(error = %err, "could not write the restored settings");
                    }
                    return Loaded {
                        settings: backup,
                        first_run: false,
                        warning: Some(format!(
                            "The settings file was damaged; the last backup was restored.{location}"
                        )),
                    };
                }
                tracing::error!(error = %e, moved_to = %aside.display(), "settings file is invalid; using defaults");
                Loaded {
                    settings: Settings::default(),
                    first_run: false,
                    warning: Some(format!(
                        "The settings file was damaged and has been reset to defaults.{location}"
                    )),
                }
            }
        }
    }

    fn backup_path(&self) -> PathBuf {
        self.path.with_extension("json.bak")
    }

    fn read_backup(&self) -> Option<Settings> {
        let text = fs::read_to_string(self.backup_path()).ok()?;
        serde_json::from_str::<Settings>(&text)
            .ok()
            .map(Settings::sanitized)
    }

    /// Saves settings atomically, keeping a backup of the previous file.
    ///
    /// # Errors
    /// Serialization or I/O failure.
    pub fn save(&self, settings: &Settings) -> Result<(), String> {
        let json = serde_json::to_vec_pretty(settings)
            .map_err(|e| format!("Could not serialize settings: {e}"))?;
        if self.path.exists() {
            let backup = self.backup_path();
            if let Err(e) = fs::copy(&self.path, &backup) {
                tracing::warn!(error = %e, "could not back up settings");
            }
        }
        write_atomic(&self.path, &json)
            .map_err(|e| format!("Could not save settings to {}: {e}", self.path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store(name: &str) -> (SettingsStore, PathBuf) {
        let dir =
            std::env::temp_dir().join(format!("awwm-settings-test-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        (SettingsStore::new(dir.join("settings.json")), dir)
    }

    #[test]
    fn missing_file_is_first_run_with_defaults() {
        let (store, dir) = temp_store("missing");
        let loaded = store.load();
        assert!(loaded.first_run);
        assert!(loaded.warning.is_none());
        assert_eq!(loaded.settings, Settings::default());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn save_then_load_round_trips_and_keeps_backup() {
        let (store, dir) = temp_store("roundtrip");
        let mut s = Settings::default();
        s.snapping.distance = 24;
        store.save(&s).expect("save");
        s.snapping.distance = 30;
        store.save(&s).expect("save again");
        let loaded = store.load();
        assert_eq!(loaded.settings.snapping.distance, 30);
        assert!(!loaded.first_run);
        let backup: Settings = serde_json::from_str(
            &fs::read_to_string(store.path().with_extension("json.bak")).expect("backup"),
        )
        .expect("parse");
        assert_eq!(backup.snapping.distance, 24);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn corrupt_file_is_moved_aside() {
        let (store, dir) = temp_store("corrupt");
        fs::create_dir_all(&dir).expect("dir");
        fs::write(store.path(), "{ this is not json").expect("write");
        let loaded = store.load();
        assert!(loaded.warning.is_some());
        assert_eq!(loaded.settings, Settings::default());
        assert!(!store.path().exists());
        let aside = fs::read_dir(&dir)
            .expect("read dir")
            .filter_map(Result::ok)
            .any(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("settings.corrupt-")
            });
        assert!(aside);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn corrupt_file_is_replaced_by_a_valid_backup() {
        let (store, dir) = temp_store("backup");
        let mut good = Settings::default();
        good.snapping.distance = 42;
        store.save(&good).expect("save");
        store.save(&good).expect("save again (creates backup)");
        fs::write(store.path(), "garbage").expect("corrupt");
        let loaded = store.load();
        assert_eq!(loaded.settings.snapping.distance, 42);
        assert!(
            loaded
                .warning
                .as_deref()
                .is_some_and(|w| w.contains("backup"))
        );
        // The restored settings were written back, and a further save keeps them as backup.
        store.save(&loaded.settings).expect("save after restore");
        let backup = store.read_backup().expect("backup readable");
        assert_eq!(backup.snapping.distance, 42);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn out_of_range_values_are_sanitized_on_load() {
        let (store, dir) = temp_store("sanitize");
        fs::create_dir_all(&dir).expect("dir");
        fs::write(
            store.path(),
            r#"{"snapping":{"distance":5000},"pinning":{"borderColor":"nope"}}"#,
        )
        .expect("write");
        let loaded = store.load();
        assert_eq!(loaded.settings.snapping.distance, 100);
        assert_eq!(loaded.settings.pinning.border_color, "#F59E0B");
        let _ = fs::remove_dir_all(dir);
    }
}
