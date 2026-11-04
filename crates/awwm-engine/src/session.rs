//! Persistence of the managed-window set (`session.json`) for crash recovery.
//!
//! The file is rewritten atomically (temporary file + rename) whenever the set changes,
//! so after a crash or forced termination the next start can either re-adopt the windows
//! or restore their original state.

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::manager::ManagedEntry;
use crate::transparency::OriginalLayering;

const SESSION_VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionFile {
    version: u32,
    entries: Vec<ManagedEntry>,
}

/// Writes `contents` to `path` atomically.
///
/// # Errors
/// Any I/O error from creating, writing, syncing or renaming the temporary file.
pub fn write_atomic(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(contents)?;
        f.sync_all()?;
    }
    // On Windows `rename` replaces the destination atomically (MoveFileExW with
    // MOVEFILE_REPLACE_EXISTING).
    fs::rename(&tmp, path)
}

/// Larger session files are ignored (a real one holds at most a few dozen entries).
const MAX_SESSION_BYTES: u64 = 1024 * 1024;
/// At most this many entries are read from a session file.
const MAX_SESSION_ENTRIES: usize = 256;

/// The session file is user-writable, so its values are clamped before they drive any
/// Win32 call: opacity to 0–100 and layered flags to the two documented bits.
fn sanitize_entry(mut e: ManagedEntry) -> ManagedEntry {
    use windows::Win32::UI::WindowsAndMessaging::{LWA_ALPHA, LWA_COLORKEY};
    e.opacity = e.opacity.min(100);
    if let Some(OriginalLayering::Attributes { flags, .. }) = &mut e.original_layering {
        *flags &= LWA_ALPHA.0 | LWA_COLORKEY.0;
    }
    e
}

/// Session store at a fixed path.
#[derive(Debug, Clone)]
pub struct SessionStore {
    path: Option<PathBuf>,
}

impl SessionStore {
    /// A store writing to `path`; `None` disables persistence (tests).
    #[must_use]
    pub const fn new(path: Option<PathBuf>) -> Self {
        Self { path }
    }

    /// Loads the persisted entries (empty when missing or unreadable).
    #[must_use]
    pub fn load(&self) -> Vec<ManagedEntry> {
        let Some(path) = &self.path else {
            return Vec::new();
        };
        if fs::metadata(path).is_ok_and(|m| m.len() > MAX_SESSION_BYTES) {
            tracing::warn!(path = %path.display(), "ignoring oversized session file");
            return Vec::new();
        }
        let Ok(text) = fs::read_to_string(path) else {
            return Vec::new();
        };
        match serde_json::from_str::<SessionFile>(&text) {
            Ok(file) if file.version == SESSION_VERSION => file
                .entries
                .into_iter()
                .take(MAX_SESSION_ENTRIES)
                .map(sanitize_entry)
                .collect(),
            Ok(file) => {
                tracing::warn!(
                    version = file.version,
                    "ignoring session file with unknown version"
                );
                Vec::new()
            }
            Err(e) => {
                tracing::warn!(error = %e, "ignoring unreadable session file");
                Vec::new()
            }
        }
    }

    /// Persists the entries (removes the file when empty).
    pub fn save(&self, entries: &[ManagedEntry]) {
        let Some(path) = &self.path else { return };
        let result = if entries.is_empty() {
            match fs::remove_file(path) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
                _ => Ok(()),
            }
        } else {
            // Titles are not needed for recovery (identity is HWND + PID + executable) and
            // can contain private information such as document names: keep them off disk.
            let file = SessionFile {
                version: SESSION_VERSION,
                entries: entries
                    .iter()
                    .map(|e| ManagedEntry {
                        title: String::new(),
                        ..e.clone()
                    })
                    .collect(),
            };
            serde_json::to_vec_pretty(&file)
                .map_err(std::io::Error::other)
                .and_then(|bytes| write_atomic(path, &bytes))
        };
        if let Err(e) = result {
            tracing::warn!(error = %e, path = %path.display(), "could not persist session");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transparency::OriginalLayering;
    use crate::types::ManagedSource;

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir()
            .join(format!("awwm-test-{}-{name}", std::process::id()))
            .join("session.json")
    }

    #[test]
    fn save_load_and_clear() {
        let path = temp_path("session");
        let store = SessionStore::new(Some(path.clone()));
        assert!(store.load().is_empty());
        let entry = ManagedEntry {
            hwnd: 42,
            pid: 7,
            exe_path: r"C:\x.exe".into(),
            class_name: "C".into(),
            title: "T".into(),
            pinned: true,
            opacity: 70,
            original_topmost: false,
            original_layering: Some(OriginalLayering::Attributes {
                key: 0,
                alpha: 255,
                flags: 2,
            }),
            source: ManagedSource::Rule,
            rule_id: Some("r1".into()),
            pin_pending: false,
            release_pending: false,
        };
        store.save(std::slice::from_ref(&entry));
        // Everything round-trips except the title, which is never written to disk.
        assert!(!fs::read_to_string(&path).expect("read").contains("\"T\""));
        assert_eq!(
            store.load(),
            vec![ManagedEntry {
                title: String::new(),
                ..entry
            }]
        );
        store.save(&[]);
        assert!(!path.exists());
        let _ = fs::remove_dir_all(path.parent().expect("parent"));
    }

    #[test]
    fn tampered_values_are_clamped_on_load() {
        let path = temp_path("tampered");
        let json = r#"{"version":1,"entries":[{"hwnd":1,"pid":2,"exePath":"C:\\a.exe","className":"C",
            "title":"","pinned":true,"opacity":250,"originalTopmost":false,
            "originalLayering":{"kind":"attributes","key":0,"alpha":10,"flags":4294967295},
            "source":"manual","ruleId":null}]}"#;
        write_atomic(&path, json.as_bytes()).expect("write");
        let loaded = SessionStore::new(Some(path.clone())).load();
        assert_eq!(loaded[0].opacity, 100);
        assert_eq!(
            loaded[0].original_layering,
            Some(OriginalLayering::Attributes {
                key: 0,
                alpha: 10,
                flags: 3
            })
        );
        let _ = fs::remove_dir_all(path.parent().expect("parent"));
    }

    #[test]
    fn corrupt_file_is_ignored() {
        let path = temp_path("corrupt");
        write_atomic(&path, b"{not json").expect("write");
        assert!(SessionStore::new(Some(path.clone())).load().is_empty());
        let _ = fs::remove_dir_all(path.parent().expect("parent"));
    }

    #[test]
    fn disabled_store_is_inert() {
        let store = SessionStore::new(None);
        store.save(&[]);
        assert!(store.load().is_empty());
    }
}
