// SPDX-License-Identifier: MPL-2.0

use crate::config::paths;
use crate::error::{LoggingError, Result};
use chrono::{Days, NaiveDate, Utc};
use std::fs;
use std::path::Path;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::prelude::*;
use tracing_subscriber::{EnvFilter, fmt};

pub const LOG_RETENTION_DAYS: u64 = 7;

pub fn init(default_level: &str) -> Result<WorkerGuard, LoggingError> {
    let paths = paths();
    fs::create_dir_all(&paths.log_dir).map_err(|source| LoggingError::CreateLogDir {
        path: paths.log_dir.clone(),
        source,
    })?;
    prune_old_logs(&paths.log_dir);
    let file_appender = tracing_appender::rolling::daily(&paths.log_dir, "yapcap.log");
    let (file_writer, guard) = tracing_appender::non_blocking(file_appender);
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_level));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt::layer().with_writer(std::io::stderr))
        .with(fmt::layer().with_writer(file_writer).with_ansi(false))
        .try_init()
        .map_err(LoggingError::InitTracing)?;

    Ok(guard)
}

fn prune_old_logs(log_dir: &Path) {
    let cutoff = Utc::now()
        .date_naive()
        .checked_sub_days(Days::new(LOG_RETENTION_DAYS.saturating_sub(1)))
        .unwrap_or(NaiveDate::MIN);
    let Ok(entries) = fs::read_dir(log_dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if should_prune_log_file(&path, cutoff) {
            let _ = fs::remove_file(path);
        }
    }
}

fn should_prune_log_file(path: &Path, cutoff: NaiveDate) -> bool {
    log_file_date(path).is_some_and(|date| date < cutoff)
}

fn log_file_date(path: &Path) -> Option<NaiveDate> {
    let file_name = path.file_name()?.to_str()?;
    let date = file_name.strip_prefix("yapcap.log.")?;
    NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_log_dir() -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("yapcap-log-test-{nanos}"))
    }

    #[test]
    fn log_dir_is_created_on_init() {
        let dir = test_log_dir();
        assert!(!dir.exists());
        fs::create_dir_all(&dir).unwrap();
        assert!(dir.exists());
    }

    #[test]
    fn dated_logs_older_than_retention_are_pruned() {
        let cutoff = NaiveDate::from_ymd_opt(2026, 6, 9).unwrap();
        assert!(should_prune_log_file(
            &PathBuf::from("yapcap.log.2026-06-08"),
            cutoff
        ));
        assert!(!should_prune_log_file(
            &PathBuf::from("yapcap.log.2026-06-09"),
            cutoff
        ));
        assert!(!should_prune_log_file(
            &PathBuf::from("other.log.2026-06-08"),
            cutoff
        ));
    }

    #[test]
    fn prune_old_logs_deletes_only_old_dated_yapcap_logs() {
        let dir = test_log_dir();
        fs::create_dir_all(&dir).unwrap();

        let old = dir.join("yapcap.log.2000-01-01");
        let invalid = dir.join("yapcap.log.old");
        let other = dir.join("other.log.2000-01-01");
        fs::write(&old, "").unwrap();
        fs::write(&invalid, "").unwrap();
        fs::write(&other, "").unwrap();

        prune_old_logs(&dir);

        assert!(!old.exists());
        assert!(invalid.exists());
        assert!(other.exists());
        fs::remove_dir_all(dir).unwrap();
    }
}
