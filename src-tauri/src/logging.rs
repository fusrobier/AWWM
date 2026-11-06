//! Structured logging with `tracing`: daily-rotated files in
//! `%LOCALAPPDATA%\dev.awwm.windowmanager\logs`
//! (7 files kept) and, in debug builds, stderr. The level can be changed at runtime.

use std::path::Path;

use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{Builder, Rotation};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Registry, fmt, reload};

use awwm_core::settings::LogLevel;

/// Keeps the log writer alive and allows changing the level.
pub struct Logging {
    _guard: Option<WorkerGuard>,
    filter: reload::Handle<EnvFilter, Registry>,
}

impl std::fmt::Debug for Logging {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Logging").finish_non_exhaustive()
    }
}

fn filter_for(level: LogLevel) -> EnvFilter {
    // Our crates at the configured level; noisy dependencies limited to warnings.
    let l = level.as_filter();
    EnvFilter::new(format!(
        "warn,awwm={l},awwm_lib={l},awwm_engine={l},awwm_core={l}"
    ))
}

impl Logging {
    /// Installs the global subscriber. File logging is skipped (with a message on stderr)
    /// if the log directory cannot be used.
    #[must_use]
    pub fn init(log_dir: &Path, level: LogLevel) -> Self {
        let (filter, handle) = reload::Layer::new(filter_for(level));
        let file = Builder::new()
            .rotation(Rotation::DAILY)
            .filename_prefix("awwm")
            .filename_suffix("log")
            .max_log_files(7)
            .build(log_dir);
        let (file_layer, guard) = match file {
            Ok(appender) => {
                let (writer, guard) = tracing_appender::non_blocking(appender);
                let layer = fmt::layer()
                    .with_writer(writer)
                    .with_ansi(false)
                    .with_target(true)
                    .with_thread_names(true);
                (Some(layer), Some(guard))
            }
            Err(e) => {
                eprintln!("AWWM: file logging disabled: {e}");
                (None, None)
            }
        };
        let stderr_layer = cfg!(debug_assertions)
            .then(|| fmt::layer().with_writer(std::io::stderr).with_target(true));
        let _ = tracing_subscriber::registry()
            .with(filter)
            .with(file_layer)
            .with(stderr_layer)
            .try_init();
        Self {
            _guard: guard,
            filter: handle,
        }
    }

    /// Changes the log level at runtime.
    pub fn set_level(&self, level: LogLevel) {
        if let Err(e) = self.filter.reload(filter_for(level)) {
            tracing::warn!(error = %e, "could not change the log level");
        }
    }
}
