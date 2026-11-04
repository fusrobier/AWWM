//! Export and import of the settings (including automatic rules) as a JSON file, chosen
//! with the native Windows Save/Open dialogs.
//!
//! The exported file has the same format as `settings.json`. An import is validated
//! before anything changes: the file must be at most 1 MiB, be a JSON object written by
//! AWWM (it carries `schemaVersion`) and parse as settings; it is then sanitized like any
//! other settings change.

use std::path::{Path, PathBuf};

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Controls::Dialogs::{
    CommDlgExtendedError, GetOpenFileNameW, GetSaveFileNameW, OFN_EXPLORER, OFN_FILEMUSTEXIST,
    OFN_NOCHANGEDIR, OFN_OVERWRITEPROMPT, OFN_PATHMUSTEXIST, OPENFILENAMEW,
};
use windows::core::{PCWSTR, PWSTR, w};

use awwm_core::settings::Settings;
use awwm_engine::session::write_atomic;

/// Largest file accepted for import.
pub const MAX_IMPORT_BYTES: u64 = 1024 * 1024;
/// File name suggested in the Save dialog.
pub const DEFAULT_FILE_NAME: &str = "AWWM-settings.json";

/// Writes `settings` to `path` (pretty JSON, atomically).
///
/// # Errors
/// A user-facing message when the file cannot be written.
pub fn write_export(path: &Path, settings: &Settings) -> Result<(), String> {
    let json = serde_json::to_vec_pretty(settings)
        .map_err(|e| format!("Could not serialize the settings: {e}"))?;
    write_atomic(path, &json).map_err(|e| format!("Could not write {}: {e}", path.display()))
}

/// Reads and validates an exported settings file.
///
/// # Errors
/// A user-facing message when the file is too large, unreadable, not an AWWM settings
/// file, or invalid.
pub fn read_import(path: &Path) -> Result<Settings, String> {
    let size = std::fs::metadata(path)
        .map_err(|e| format!("Could not read {}: {e}", path.display()))?
        .len();
    if size > MAX_IMPORT_BYTES {
        return Err("This file is too large to be an AWWM settings file.".to_owned());
    }
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("Could not read {}: {e}", path.display()))?;
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("This file is not valid JSON: {e}"))?;
    if !value
        .as_object()
        .is_some_and(|o| o.contains_key("schemaVersion"))
    {
        return Err("This file is not an AWWM settings export.".to_owned());
    }
    let settings: Settings =
        serde_json::from_value(value).map_err(|e| format!("This settings file is invalid: {e}"))?;
    Ok(settings.sanitized())
}

/// Which dialog to show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogKind {
    Export,
    Import,
}

/// Shows the native Save (export) or Open (import) dialog. Returns `Ok(None)` when the user
/// cancels. Must run on a thread that pumps messages (the UI thread).
///
/// # Errors
/// A user-facing message when the dialog itself fails.
pub fn pick_file(kind: DialogKind, owner: Option<HWND>) -> Result<Option<PathBuf>, String> {
    let mut buffer = [0u16; 1024];
    if kind == DialogKind::Export {
        for (slot, unit) in buffer.iter_mut().zip(DEFAULT_FILE_NAME.encode_utf16()) {
            *slot = unit;
        }
    }
    let mut ofn = OPENFILENAMEW {
        lStructSize: u32::try_from(std::mem::size_of::<OPENFILENAMEW>()).unwrap_or(0),
        hwndOwner: owner.unwrap_or_default(),
        lpstrFilter: w!("AWWM settings (*.json)\0*.json\0All files (*.*)\0*.*\0\0"),
        lpstrFile: PWSTR(buffer.as_mut_ptr()),
        nMaxFile: u32::try_from(buffer.len()).unwrap_or(0),
        lpstrDefExt: w!("json"),
        lpstrTitle: match kind {
            DialogKind::Export => w!("Export AWWM settings"),
            DialogKind::Import => w!("Import AWWM settings"),
        },
        Flags: OFN_EXPLORER
            | OFN_NOCHANGEDIR
            | OFN_PATHMUSTEXIST
            | match kind {
                DialogKind::Export => OFN_OVERWRITEPROMPT,
                DialogKind::Import => OFN_FILEMUSTEXIST,
            },
        lpstrInitialDir: PCWSTR::null(),
        ..OPENFILENAMEW::default()
    };
    // SAFETY: `ofn` is fully initialised; `buffer` outlives the modal call and its length
    // is passed in `nMaxFile`; all strings are static NUL-terminated wide literals.
    let chosen = unsafe {
        match kind {
            DialogKind::Export => GetSaveFileNameW(&raw mut ofn),
            DialogKind::Import => GetOpenFileNameW(&raw mut ofn),
        }
    }
    .as_bool();
    if !chosen {
        // SAFETY: plain call reporting the error of the dialog that just closed.
        let error = unsafe { CommDlgExtendedError() };
        return if error.0 == 0 {
            Ok(None)
        } else {
            Err(format!("The file dialog failed (error {:#x}).", error.0))
        };
    }
    let len = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    Ok(Some(PathBuf::from(String::from_utf16_lossy(
        &buffer[..len],
    ))))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("awwm-backup-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dir");
        dir
    }

    #[test]
    fn export_then_import_round_trips_rules_and_settings() {
        let dir = temp_dir("roundtrip");
        let mut settings = Settings::default();
        settings.rules[0].enabled = true;
        settings.rules[0].opacity = 42;
        settings.snapping.distance = 33;
        let path = dir.join(DEFAULT_FILE_NAME);
        write_export(&path, &settings).expect("export");
        let imported = read_import(&path).expect("import");
        assert_eq!(imported, settings.sanitized());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn imports_are_sanitized() {
        let dir = temp_dir("sanitize");
        let path = dir.join("s.json");
        std::fs::write(&path, r#"{"schemaVersion":1,"snapping":{"distance":9999},"rules":[{"id":"x","exe":"a.exe","opacity":250}]}"#)
            .expect("write");
        let imported = read_import(&path).expect("import");
        assert_eq!(imported.snapping.distance, 100);
        assert_eq!(imported.rules.len(), 1);
        assert_eq!(imported.rules[0].opacity, 100);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn foreign_and_broken_files_are_rejected() {
        let dir = temp_dir("reject");
        let cases = [
            ("empty-object.json", "{}", "not an AWWM settings export"),
            ("array.json", "[1,2,3]", "not an AWWM settings export"),
            ("broken.json", "{ nope", "not valid JSON"),
            (
                "wrong-types.json",
                r#"{"schemaVersion":1,"rules":"nope"}"#,
                "invalid",
            ),
        ];
        for (name, content, expected) in cases {
            let path = dir.join(name);
            std::fs::write(&path, content).expect("write");
            let err = read_import(&path).expect_err(name);
            assert!(err.contains(expected), "{name}: {err}");
        }
        assert!(read_import(&dir.join("missing.json")).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn oversized_files_are_rejected() {
        let dir = temp_dir("size");
        let path = dir.join("big.json");
        let big = format!(
            "{{\"schemaVersion\":1,\"x\":\"{}\"}}",
            "a".repeat(usize::try_from(MAX_IMPORT_BYTES).unwrap_or(0))
        );
        std::fs::write(&path, big).expect("write");
        assert!(
            read_import(&path)
                .expect_err("too big")
                .contains("too large")
        );
        let _ = std::fs::remove_dir_all(dir);
    }
}
