//! window_manager: the registry of managed windows (pinned and/or transparent) with
//! state capture, application, restoration and handle-change reattachment.
//!
//! The registry lives exclusively on the engine thread.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::error::{EngineError, EngineResult};
use crate::pinning;
use crate::transparency::{self, OriginalLayering};
use crate::types::{ManagedSource, ManagedWindowInfo};
use crate::win::{WindowHandle, process, window};

/// Everything AWWM knows about a managed window, including the state needed to restore it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedEntry {
    pub hwnd: u64,
    pub pid: u32,
    pub exe_path: String,
    pub class_name: String,
    pub title: String,
    pub pinned: bool,
    pub opacity: u8,
    /// The window was already topmost before AWWM pinned it.
    pub original_topmost: bool,
    /// Layering before AWWM changed it; `None` when it could not be captured (per-pixel
    /// layered windows), in which case transparency is never applied.
    pub original_layering: Option<OriginalLayering>,
    pub source: ManagedSource,
    pub rule_id: Option<String>,
    /// The window should be pinned but Windows refused topmost so far (a full-screen
    /// application had the foreground). Retried on foreground changes.
    #[serde(default)]
    pub pin_pending: bool,
    /// The user asked to release the window but restoring it failed (e.g. not responding).
    /// AWWM no longer re-applies its state, and the next start restores it instead of
    /// re-adopting it.
    #[serde(default)]
    pub release_pending: bool,
}

impl ManagedEntry {
    /// The window handle.
    #[must_use]
    pub fn handle(&self) -> WindowHandle {
        WindowHandle::from_u64(self.hwnd).unwrap_or(WindowHandle::from_raw(0))
    }

    /// Display name of the application (executable name, else title).
    #[must_use]
    pub fn app_name(&self) -> String {
        let exe = process::exe_name(&self.exe_path);
        if exe.is_empty() {
            self.title.clone()
        } else {
            exe
        }
    }

    /// UI representation (title refreshed live).
    #[must_use]
    pub fn info(&self) -> ManagedWindowInfo {
        let h = self.handle();
        let title = if window::exists(h) {
            window::title(h)
        } else {
            self.title.clone()
        };
        ManagedWindowInfo {
            hwnd: self.hwnd,
            title,
            exe_name: process::exe_name(&self.exe_path),
            exe_path: self.exe_path.clone(),
            pid: self.pid,
            pinned: self.pinned,
            pin_pending: self.pin_pending,
            opacity: self.opacity,
            source: self.source,
            rule_id: self.rule_id.clone(),
        }
    }
}

/// Identity of a destroyed managed window, kept briefly so a replacement window of the
/// same application can inherit its state.
#[derive(Clone, Debug)]
pub struct Departed {
    pub entry: ManagedEntry,
    pub at: Instant,
}

/// Desired state for [`Manager::manage`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Desired {
    pub pinned: bool,
    pub opacity: u8,
    pub source: ManagedSource,
    pub rule_id: Option<String>,
    /// When Windows refuses topmost (`TopmostRefused`), keep the window managed with the
    /// pin pending instead of failing. Used for automatic pins (rules, reattachment,
    /// recovery) that happen while the user may be in a full-screen application.
    pub defer_refused_pin: bool,
}

/// The managed-window registry.
#[derive(Debug, Default)]
pub struct Manager {
    entries: Vec<ManagedEntry>,
    departed: Vec<Departed>,
}

/// Facts about a window captured before managing it.
fn describe(h: WindowHandle) -> (u32, String, String, String) {
    let pid = window::process_id(h);
    let exe = process::exe_path(process::effective_pid(h)).unwrap_or_default();
    (pid, exe, window::class_name(h), window::title(h))
}

impl Manager {
    /// All entries in insertion order.
    #[must_use]
    pub fn entries(&self) -> &[ManagedEntry] {
        &self.entries
    }

    /// Looks up an entry.
    #[must_use]
    pub fn get(&self, h: WindowHandle) -> Option<&ManagedEntry> {
        self.entries.iter().find(|e| e.hwnd == h.as_u64())
    }

    fn index(&self, h: WindowHandle) -> Option<usize> {
        self.entries.iter().position(|e| e.hwnd == h.as_u64())
    }

    /// Whether the window is managed.
    #[must_use]
    pub fn contains(&self, h: WindowHandle) -> bool {
        self.index(h).is_some()
    }

    /// UI list of all managed windows.
    #[must_use]
    pub fn infos(&self) -> Vec<ManagedWindowInfo> {
        self.entries.iter().map(ManagedEntry::info).collect()
    }

    /// Handles of pinned windows (including pins still pending).
    pub fn pinned_handles(&self) -> impl Iterator<Item = WindowHandle> + '_ {
        self.entries
            .iter()
            .filter(|e| e.pinned)
            .map(ManagedEntry::handle)
    }

    /// Handles of windows that are pinned and actually topmost (they get a frame).
    pub fn effectively_pinned_handles(&self) -> impl Iterator<Item = WindowHandle> + '_ {
        self.entries
            .iter()
            .filter(|e| e.pinned && !e.pin_pending)
            .map(ManagedEntry::handle)
    }

    /// Whether any pin is waiting for Windows to allow it.
    #[must_use]
    pub fn has_pending_pins(&self) -> bool {
        self.entries.iter().any(|e| e.pin_pending)
    }

    /// Retries every pending pin. Returns `true` when at least one succeeded.
    pub fn retry_pending_pins(&mut self) -> bool {
        let mut changed = false;
        for e in self.entries.iter_mut().filter(|e| e.pin_pending) {
            let h = e.handle();
            if !window::is_same_window(h, e.pid) {
                continue;
            }
            if pinning::set_topmost(h, true, &e.app_name()).is_ok() {
                e.pin_pending = false;
                changed = true;
                tracing::info!(hwnd = %h, app = %e.app_name(), "pending pin applied");
            }
        }
        changed
    }

    /// Starts managing `h` (or updates an existing entry) so it has the desired state.
    ///
    /// For a new window a failure leaves nothing applied. For an already managed window,
    /// steps that succeeded before the failing one stay applied and the entry reflects
    /// them, so the registry always matches the window.
    ///
    /// # Errors
    /// The window is gone, elevated, hung, or rejects the change.
    pub fn manage(&mut self, h: WindowHandle, desired: Desired) -> EngineResult<&ManagedEntry> {
        if let Some(i) = self.index(h) {
            {
                let e = &mut self.entries[i];
                if !window::is_same_window(h, e.pid) {
                    self.entries.remove(i);
                    return Err(EngineError::WindowNotFound);
                }
            }
            self.set_pinned_at(i, desired.pinned, desired.defer_refused_pin)?;
            self.set_opacity_at(i, desired.opacity)?;
            let e = &mut self.entries[i];
            if desired.source == ManagedSource::Rule {
                e.source = ManagedSource::Rule;
                e.rule_id = desired.rule_id;
            }
            return Ok(&self.entries[i]);
        }
        if !window::exists(h) {
            return Err(EngineError::WindowNotFound);
        }
        let (pid, exe_path, class_name, title) = describe(h);
        let app = if exe_path.is_empty() {
            title.clone()
        } else {
            process::exe_name(&exe_path)
        };
        pinning::ensure_modifiable(h, &app)?;
        let original_layering = match transparency::capture(h, &app) {
            Ok(l) => Some(l),
            Err(e) if desired.opacity < 100 => return Err(e),
            Err(_) => None,
        };
        let mut entry = ManagedEntry {
            hwnd: h.as_u64(),
            pid,
            exe_path,
            class_name,
            title,
            pinned: false,
            opacity: 100,
            original_topmost: window::is_topmost(h),
            original_layering,
            source: desired.source,
            rule_id: desired.rule_id,
            pin_pending: false,
            release_pending: false,
        };
        if desired.pinned {
            match pinning::set_topmost(h, true, &app) {
                Ok(()) => {}
                Err(EngineError::TopmostRefused(reason)) if desired.defer_refused_pin => {
                    tracing::info!(hwnd = %h, app = %app, %reason, "pin deferred until Windows allows it");
                    entry.pin_pending = true;
                }
                Err(e) => return Err(e),
            }
            entry.pinned = true;
        }
        if desired.opacity < 100 {
            let layering = entry
                .original_layering
                .unwrap_or(OriginalLayering::NotLayered);
            if let Err(e) = transparency::apply(h, layering, desired.opacity, &app) {
                // Roll back the pin so a failed request leaves no trace.
                if entry.pinned && !entry.pin_pending && !entry.original_topmost {
                    let _ = pinning::set_topmost(h, false, &app);
                }
                return Err(e);
            }
            entry.opacity = desired.opacity;
        }
        tracing::info!(hwnd = %h, app = %app, pinned = entry.pinned, opacity = entry.opacity, source = ?entry.source, "window managed");
        self.entries.push(entry);
        Ok(&self.entries[self.entries.len() - 1])
    }

    /// Adopts a previously persisted entry (crash recovery) and re-applies its state.
    ///
    /// # Errors
    /// The window is gone or rejects the change.
    pub fn adopt(&mut self, mut entry: ManagedEntry) -> EngineResult<()> {
        let h = entry.handle();
        if !window::is_same_window(h, entry.pid) {
            return Err(EngineError::WindowNotFound);
        }
        let app = entry.app_name();
        entry.pin_pending = false;
        if entry.pinned && !window::is_topmost(h) {
            match pinning::set_topmost(h, true, &app) {
                Ok(()) => {}
                Err(EngineError::TopmostRefused(_)) => entry.pin_pending = true,
                Err(e) => return Err(e),
            }
        }
        if entry.opacity < 100 {
            match entry.original_layering {
                Some(layering) => transparency::apply(h, layering, entry.opacity, &app)?,
                None => entry.opacity = 100,
            }
        }
        if !self.contains(h) {
            self.entries.push(entry);
        }
        Ok(())
    }

    fn set_pinned_at(&mut self, i: usize, pinned: bool, defer_refused: bool) -> EngineResult<()> {
        let e = &mut self.entries[i];
        let h = e.handle();
        let app = e.app_name();
        if pinned && e.pinned && e.pin_pending && !defer_refused {
            // An explicit pin of a pending window: try now and report a refusal.
            pinning::set_topmost(h, true, &app)?;
            e.pin_pending = false;
            return Ok(());
        }
        if e.pinned == pinned {
            return Ok(());
        }
        if pinned {
            match pinning::set_topmost(h, true, &app) {
                Ok(()) => e.pin_pending = false,
                Err(EngineError::TopmostRefused(_)) if defer_refused => e.pin_pending = true,
                Err(err) => return Err(err),
            }
        } else {
            // Unpinning a window that was topmost on its own (or never became topmost
            // through us) leaves its Z-order alone.
            if !e.original_topmost && !e.pin_pending {
                pinning::set_topmost(h, false, &app)?;
            }
            e.pin_pending = false;
        }
        e.pinned = pinned;
        Ok(())
    }

    fn set_opacity_at(&mut self, i: usize, opacity: u8) -> EngineResult<()> {
        let opacity = opacity.min(100);
        let e = &mut self.entries[i];
        if e.opacity == opacity {
            return Ok(());
        }
        let h = e.handle();
        let app = e.app_name();
        let Some(layering) = e.original_layering else {
            return Err(EngineError::Unsupported(format!(
                "\u{201c}{app}\u{201d} draws its own per-pixel transparency, so its opacity cannot be changed."
            )));
        };
        transparency::apply(h, layering, opacity, &app)?;
        e.opacity = opacity;
        Ok(())
    }

    /// Pins or unpins a managed window (keeping it managed).
    ///
    /// # Errors
    /// Unknown window or a Win32 failure.
    pub fn set_pinned(&mut self, h: WindowHandle, pinned: bool) -> EngineResult<&ManagedEntry> {
        let i = self.index(h).ok_or(EngineError::WindowNotFound)?;
        self.set_pinned_at(i, pinned, false)?;
        Ok(&self.entries[i])
    }

    /// Changes the opacity of a managed window.
    ///
    /// # Errors
    /// Unknown window, unsupported layering or a Win32 failure.
    pub fn set_opacity(&mut self, h: WindowHandle, opacity: u8) -> EngineResult<&ManagedEntry> {
        let i = self.index(h).ok_or(EngineError::WindowNotFound)?;
        if !window::is_same_window(h, self.entries[i].pid) {
            self.entries.remove(i);
            return Err(EngineError::WindowNotFound);
        }
        self.set_opacity_at(i, opacity)?;
        Ok(&self.entries[i])
    }

    /// Restores the window's original state and stops managing it. A window that no longer
    /// exists is simply forgotten.
    ///
    /// # Errors
    /// Windows rejected the restoration (the entry is removed anyway).
    pub fn release(&mut self, h: WindowHandle) -> EngineResult<ManagedEntry> {
        let i = self.index(h).ok_or(EngineError::WindowNotFound)?;
        match restore_entry(&self.entries[i]) {
            Ok(()) | Err(EngineError::WindowNotFound) => {
                let entry = self.entries.remove(i);
                tracing::info!(hwnd = %h, app = %entry.app_name(), "window released");
                Ok(entry)
            }
            // Keep the entry (and its original state) so the user can retry, and so the
            // session file still lists the window for recovery.
            Err(e) => {
                self.entries[i].release_pending = true;
                Err(e)
            }
        }
    }

    /// Releases every managed window. Windows that could not be restored (for example
    /// because they are not responding) stay registered; returns their number.
    pub fn release_all(&mut self) -> usize {
        let entries = std::mem::take(&mut self.entries);
        for mut e in entries {
            match restore_entry(&e) {
                Ok(()) | Err(EngineError::WindowNotFound) => {}
                Err(err) => {
                    tracing::warn!(hwnd = e.hwnd, error = %err, "window could not be restored; keeping it");
                    e.release_pending = true;
                    self.entries.push(e);
                }
            }
        }
        self.entries.len()
    }

    /// Forgets a destroyed window. If `grace` is non-zero its identity is kept so a
    /// replacement window can inherit its state.
    pub fn on_destroyed(&mut self, h: WindowHandle, grace: Duration) -> Option<ManagedEntry> {
        let i = self.index(h)?;
        let entry = self.entries.remove(i);
        tracing::info!(hwnd = %h, app = %entry.app_name(), "managed window closed");
        if !grace.is_zero() {
            self.departed.push(Departed {
                entry: entry.clone(),
                at: Instant::now(),
            });
        }
        Some(entry)
    }

    /// Finds (and removes) a recently destroyed entry matching a new window of the same
    /// executable and class. An identical title is preferred.
    pub fn take_departed(
        &mut self,
        exe_path: &str,
        class_name: &str,
        title: &str,
        grace: Duration,
    ) -> Option<ManagedEntry> {
        let now = Instant::now();
        self.departed.retain(|d| now.duration_since(d.at) <= grace);
        if exe_path.is_empty() {
            return None;
        }
        let matches = |d: &Departed| {
            d.entry.exe_path.eq_ignore_ascii_case(exe_path) && d.entry.class_name == class_name
        };
        let index = self
            .departed
            .iter()
            .position(|d| matches(d) && d.entry.title == title)
            .or_else(|| self.departed.iter().position(matches))?;
        Some(self.departed.remove(index).entry)
    }

    /// Whether any departed entries are waiting for a replacement window.
    #[must_use]
    pub fn has_departed(&self) -> bool {
        !self.departed.is_empty()
    }

    /// Re-applies topmost/opacity when the application reset them. Returns `true` when
    /// something had to be re-applied.
    pub fn reassert(&mut self, h: WindowHandle) -> bool {
        let Some(i) = self.index(h) else { return false };
        let e = &mut self.entries[i];
        if e.release_pending || !window::is_same_window(h, e.pid) {
            return false;
        }
        let app = e.app_name();
        let mut fixed = false;
        if e.pinned && !window::is_topmost(h) && pinning::set_topmost(h, true, &app).is_ok() {
            e.pin_pending = false;
            fixed = true;
        }
        if e.opacity < 100
            && let Some(layering) = e.original_layering
            && !transparency::is_applied(h, layering, e.opacity)
            && transparency::apply(h, layering, e.opacity, &app).is_ok()
        {
            fixed = true;
        }
        if fixed {
            tracing::info!(hwnd = %h, app = %app, "re-applied window state reset by the application");
        }
        fixed
    }
}

/// Restores the original topmost state and layering of an entry.
///
/// # Errors
/// Windows rejected the change.
pub fn restore_entry(entry: &ManagedEntry) -> EngineResult<()> {
    let h = entry.handle();
    if !window::is_same_window(h, entry.pid) {
        return Ok(());
    }
    let app = entry.app_name();
    let mut result = Ok(());
    if entry.opacity < 100
        && let Some(layering) = entry.original_layering
        && let Err(e) = transparency::restore(h, layering, &app)
    {
        tracing::warn!(hwnd = %h, error = %e, "failed to restore opacity");
        result = Err(e);
    }
    if entry.pinned
        && !entry.original_topmost
        && !entry.pin_pending
        && let Err(e) = pinning::set_topmost(h, false, &app)
    {
        tracing::warn!(hwnd = %h, error = %e, "failed to restore z-order");
        result = Err(e);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(hwnd: u64, exe: &str, class: &str, title: &str) -> ManagedEntry {
        ManagedEntry {
            hwnd,
            pid: 1,
            exe_path: exe.into(),
            class_name: class.into(),
            title: title.into(),
            pinned: true,
            opacity: 80,
            original_topmost: false,
            original_layering: Some(OriginalLayering::NotLayered),
            source: ManagedSource::Manual,
            rule_id: None,
            pin_pending: false,
            release_pending: false,
        }
    }

    #[test]
    fn pending_pin_flag_defaults_to_false_in_old_session_files() {
        let json = r#"{"hwnd":5,"pid":1,"exePath":"C:\\a.exe","className":"C","title":"T","pinned":true,
            "opacity":100,"originalTopmost":false,"originalLayering":null,"source":"manual","ruleId":null}"#;
        let e: ManagedEntry = serde_json::from_str(json).expect("parse");
        assert!(!e.pin_pending);
    }

    #[test]
    fn effectively_pinned_excludes_pending_pins() {
        let mut m = Manager::default();
        m.entries.push(entry(1, r"C:\a.exe", "C", "A"));
        m.entries.push(ManagedEntry {
            pin_pending: true,
            ..entry(2, r"C:\a.exe", "C", "B")
        });
        assert_eq!(m.pinned_handles().count(), 2);
        assert_eq!(
            m.effectively_pinned_handles()
                .map(WindowHandle::as_u64)
                .collect::<Vec<_>>(),
            vec![1]
        );
        assert!(m.has_pending_pins());
        assert!(m.infos()[1].pin_pending);
    }

    #[test]
    fn departed_entries_match_by_exe_and_class_preferring_title() {
        let mut m = Manager::default();
        m.departed.push(Departed {
            entry: entry(1, r"C:\a.exe", "Main", "One"),
            at: Instant::now(),
        });
        m.departed.push(Departed {
            entry: entry(2, r"C:\a.exe", "Main", "Two"),
            at: Instant::now(),
        });
        let grace = Duration::from_secs(30);
        assert!(m.take_departed(r"C:\b.exe", "Main", "One", grace).is_none());
        assert!(
            m.take_departed(r"C:\a.exe", "Other", "One", grace)
                .is_none()
        );
        assert_eq!(
            m.take_departed(r"C:\A.EXE", "Main", "Two", grace)
                .map(|e| e.hwnd),
            Some(2)
        );
        assert_eq!(
            m.take_departed(r"C:\a.exe", "Main", "Unrelated", grace)
                .map(|e| e.hwnd),
            Some(1)
        );
        assert!(!m.has_departed());
    }

    #[test]
    fn departed_entries_expire() {
        let mut m = Manager::default();
        let old = Instant::now()
            .checked_sub(Duration::from_secs(60))
            .unwrap_or_else(Instant::now);
        m.departed.push(Departed {
            entry: entry(1, r"C:\a.exe", "Main", "One"),
            at: old,
        });
        assert!(
            m.take_departed(r"C:\a.exe", "Main", "One", Duration::from_secs(30))
                .is_none()
        );
        assert!(!m.has_departed());
    }

    #[test]
    fn entry_serializes_camel_case() {
        let json = serde_json::to_string(&entry(5, r"C:\a.exe", "Main", "T")).expect("json");
        assert!(json.contains("\"originalLayering\":{\"kind\":\"notLayered\"}"));
        assert!(json.contains("\"exePath\""));
        let back: ManagedEntry = serde_json::from_str(&json).expect("parse");
        assert_eq!(back.hwnd, 5);
    }
}
