// SPDX-License-Identifier: MPL-2.0

use std::ffi::{OsStr, OsString};
use std::sync::{Mutex, MutexGuard, OnceLock};

pub fn env_lock() -> MutexGuard<'static, ()> {
    lock_env()
}

pub struct TestEnv {
    _guard: MutexGuard<'static, ()>,
    saved: Vec<(OsString, Option<OsString>)>,
    _root: Option<tempfile::TempDir>,
}

pub fn test_env() -> TestEnv {
    let mut env = TestEnv {
        _guard: lock_env(),
        saved: Vec::new(),
        _root: None,
    };
    let root = tempfile::tempdir().expect("create isolated test root");
    env.set("XDG_CONFIG_HOME", root.path().join("config"));
    env.set("XDG_STATE_HOME", root.path().join("state"));
    env.set("XDG_CACHE_HOME", root.path().join("cache"));
    env.remove("FLATPAK_ID");
    env._root = Some(root);
    env
}

impl TestEnv {
    pub fn set<K, V>(&mut self, key: K, value: V)
    where
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        self.save_once(key.as_ref());
        unsafe {
            std::env::set_var(key, value);
        }
    }

    pub fn remove<K>(&mut self, key: K)
    where
        K: AsRef<OsStr>,
    {
        self.save_once(key.as_ref());
        unsafe {
            std::env::remove_var(key);
        }
    }

    fn save_once(&mut self, key: &OsStr) {
        if self.saved.iter().any(|(saved, _)| saved == key) {
            return;
        }
        self.saved.push((key.to_os_string(), std::env::var_os(key)));
    }
}

impl Drop for TestEnv {
    fn drop(&mut self) {
        for (key, value) in self.saved.iter().rev() {
            unsafe {
                if let Some(value) = value {
                    std::env::set_var(key, value);
                } else {
                    std::env::remove_var(key);
                }
            }
        }
    }
}

fn lock_env() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
