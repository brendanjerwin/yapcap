// SPDX-License-Identifier: MPL-2.0

use crate::config::AppPaths;
use std::fs::{File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct ProcessInfo {
    pub id: String,
    pub pid: u32,
    pub panel_output: Option<String>,
    pub flatpak_id: Option<String>,
    pub lock_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct RefreshOwner {
    inner: Arc<RefreshOwnerInner>,
}

#[derive(Debug, Clone)]
pub struct RefreshOwnerWaiter {
    lock_path: PathBuf,
}

#[derive(Debug)]
pub enum RefreshOwnerAttempt {
    Owner(RefreshOwner),
    NonOwner(RefreshOwnerWaiter),
}

#[derive(Debug)]
struct RefreshOwnerInner {
    file: File,
    lock_path: PathBuf,
}

impl ProcessInfo {
    #[must_use]
    pub fn current(lock_path: PathBuf) -> Self {
        Self {
            id: process_id(),
            pid: std::process::id(),
            panel_output: std::env::var("COSMIC_PANEL_OUTPUT").ok(),
            flatpak_id: std::env::var("FLATPAK_ID").ok(),
            lock_path,
        }
    }

    #[must_use]
    pub fn flatpak_status(&self) -> &'static str {
        if self.flatpak_id.is_some() {
            "flatpak"
        } else {
            "native"
        }
    }
}

impl RefreshOwner {
    #[must_use]
    pub fn lock_path(&self) -> &Path {
        &self.inner.lock_path
    }
}

impl RefreshOwnerWaiter {
    pub fn wait(self) -> io::Result<RefreshOwner> {
        let file = open_lock_file(&self.lock_path)?;
        lock_blocking(&file)?;
        Ok(RefreshOwner {
            inner: Arc::new(RefreshOwnerInner {
                file,
                lock_path: self.lock_path,
            }),
        })
    }
}

impl Drop for RefreshOwnerInner {
    fn drop(&mut self) {
        let _ = unlock(&self.file);
    }
}

#[must_use]
pub fn lock_path(paths: &AppPaths) -> PathBuf {
    paths.state_dir.join("refresh-owner.lock")
}

pub fn try_acquire(lock_path: PathBuf) -> io::Result<RefreshOwnerAttempt> {
    let file = open_lock_file(&lock_path)?;
    match lock_nonblocking(&file) {
        Ok(()) => Ok(RefreshOwnerAttempt::Owner(RefreshOwner {
            inner: Arc::new(RefreshOwnerInner { file, lock_path }),
        })),
        Err(error) if lock_is_contended(&error) => {
            Ok(RefreshOwnerAttempt::NonOwner(RefreshOwnerWaiter {
                lock_path,
            }))
        }
        Err(error) => Err(error),
    }
}

fn process_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    format_process_id(std::process::id(), nanos)
}

fn format_process_id(pid: u32, nanos: u128) -> String {
    format!("{pid:x}-{:06x}", nanos & 0x00ff_ffff)
}

fn open_lock_file(path: &Path) -> io::Result<File> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
}

#[cfg(unix)]
fn lock_nonblocking(file: &File) -> io::Result<()> {
    use std::os::fd::AsRawFd;
    flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB)
}

#[cfg(unix)]
fn lock_blocking(file: &File) -> io::Result<()> {
    use std::os::fd::AsRawFd;
    flock(file.as_raw_fd(), libc::LOCK_EX)
}

#[cfg(unix)]
fn unlock(file: &File) -> io::Result<()> {
    use std::os::fd::AsRawFd;
    flock(file.as_raw_fd(), libc::LOCK_UN)
}

#[cfg(unix)]
fn flock(fd: std::os::fd::RawFd, operation: libc::c_int) -> io::Result<()> {
    let result = unsafe { libc::flock(fd, operation) };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(not(unix))]
fn lock_nonblocking(_file: &File) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "refresh ownership requires Unix file locks",
    ))
}

#[cfg(not(unix))]
fn lock_blocking(_file: &File) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "refresh ownership requires Unix file locks",
    ))
}

#[cfg(not(unix))]
fn unlock(_file: &File) -> io::Result<()> {
    Ok(())
}

fn lock_is_contended(error: &io::Error) -> bool {
    matches!(
        error.raw_os_error(),
        Some(code) if code == libc::EWOULDBLOCK || code == libc::EAGAIN
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    fn temp_lock_path() -> PathBuf {
        tempfile::tempdir()
            .unwrap()
            .keep()
            .join("refresh-owner.lock")
    }

    #[test]
    fn first_process_acquires_owner_lock() {
        let path = temp_lock_path();

        let attempt = try_acquire(path.clone()).unwrap();

        match attempt {
            RefreshOwnerAttempt::Owner(owner) => assert_eq!(owner.lock_path(), path),
            RefreshOwnerAttempt::NonOwner(_) => panic!("first process should be owner"),
        }
    }

    #[test]
    fn second_process_is_non_owner_when_lock_is_held() {
        let path = temp_lock_path();
        let _owner = match try_acquire(path.clone()).unwrap() {
            RefreshOwnerAttempt::Owner(owner) => owner,
            RefreshOwnerAttempt::NonOwner(_) => panic!("first process should be owner"),
        };

        let attempt = try_acquire(path).unwrap();

        assert!(matches!(attempt, RefreshOwnerAttempt::NonOwner(_)));
    }

    #[test]
    fn dropped_owner_releases_lock() {
        let path = temp_lock_path();
        let owner = match try_acquire(path.clone()).unwrap() {
            RefreshOwnerAttempt::Owner(owner) => owner,
            RefreshOwnerAttempt::NonOwner(_) => panic!("first process should be owner"),
        };
        drop(owner);

        let attempt = try_acquire(path).unwrap();

        assert!(matches!(attempt, RefreshOwnerAttempt::Owner(_)));
    }

    #[test]
    fn waiting_process_takes_over_after_owner_is_dropped() {
        let path = temp_lock_path();
        let owner = match try_acquire(path.clone()).unwrap() {
            RefreshOwnerAttempt::Owner(owner) => owner,
            RefreshOwnerAttempt::NonOwner(_) => panic!("first process should be owner"),
        };
        let waiter = match try_acquire(path).unwrap() {
            RefreshOwnerAttempt::Owner(_) => panic!("second process should wait"),
            RefreshOwnerAttempt::NonOwner(waiter) => waiter,
        };
        let (tx, rx) = mpsc::channel();

        let handle = std::thread::spawn(move || {
            let owner = waiter.wait().unwrap();
            tx.send(owner.lock_path().to_path_buf()).unwrap();
        });

        assert!(rx.recv_timeout(Duration::from_millis(100)).is_err());
        drop(owner);
        let acquired_path = rx.recv_timeout(Duration::from_secs(2)).unwrap();
        handle.join().unwrap();

        assert!(acquired_path.ends_with("refresh-owner.lock"));
    }

    #[test]
    fn process_id_is_short_and_pid_scoped() {
        let id = format_process_id(0x12ab, 0x9876_5432_10fe_dcba);

        assert_eq!(id, "12ab-fedcba");
        assert!(id.len() <= 12);
    }
}
