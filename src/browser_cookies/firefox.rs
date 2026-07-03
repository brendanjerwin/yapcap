// SPDX-License-Identifier: MPL-2.0

use super::BrowserCookie;
use std::path::PathBuf;

/// Find the Firefox profile directory containing cookies.sqlite.
///
/// Firefox stores profiles in ~/.mozilla/firefox/ with names like
/// `xxxxx.default-release`. We look for the default-release profile,
/// falling back to any profile directory that has a cookies.sqlite file.
fn find_firefox_profiles() -> Vec<PathBuf> {
    let mut profiles = Vec::new();

    // Standard Firefox location
    let firefox_dir = dirs::home_dir().map(|h| h.join(".mozilla/firefox"));
    if let Some(dir) = firefox_dir
        && let Ok(entries) = std::fs::read_dir(&dir)
    {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() && path.join("cookies.sqlite").exists() {
                profiles.push(path);
            }
        }
    }

    // Also check for Flatpak Firefox
    let flatpak_firefox_dir =
        dirs::home_dir().map(|h| h.join(".var/app/org.mozilla.firefox/.mozilla/firefox"));
    if let Some(dir) = flatpak_firefox_dir
        && let Ok(entries) = std::fs::read_dir(&dir)
    {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() && path.join("cookies.sqlite").exists() {
                profiles.push(path);
            }
        }
    }

    // Sort: prefer default-release profiles first
    profiles.sort_by_key(|p| {
        let name = p
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        // default-release first, then default, then anything else
        if name.contains("default-release") {
            0
        } else if name.contains("default") {
            1
        } else {
            2
        }
    });

    profiles
}

/// Copy cookies.sqlite to a temp file (Firefox locks the DB while running via WAL).
/// Returns the temp file path — caller should clean up.
fn copy_cookies_to_temp(profile_dir: &PathBuf) -> Option<PathBuf> {
    let src = profile_dir.join("cookies.sqlite");
    let wal = profile_dir.join("cookies.sqlite-wal");
    let shm = profile_dir.join("cookies.sqlite-shm");

    let tmp = std::env::temp_dir().join(format!(
        "yapcap-ff-cookies-{}-{}.sqlite",
        profile_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown"),
        std::process::id()
    ));

    // Copy main DB
    if std::fs::copy(&src, &tmp).is_err() {
        return None;
    }

    // Copy WAL and SHM if they exist (needed for consistent reads)
    if wal.exists() {
        let _ = std::fs::copy(&wal, format!("{}-wal", tmp.display()));
    }
    if shm.exists() {
        let _ = std::fs::copy(&shm, format!("{}-shm", tmp.display()));
    }

    Some(tmp)
}

/// Find a cookie by name and domain from Firefox's on-disk SQLite database.
///
/// Firefox stores cookies unencrypted in cookies.sqlite. The DB is WAL-locked
/// while Firefox is running, so we copy it to a temp file first.
pub fn find_cookie(cookie_name: &str, domain: &str) -> Option<BrowserCookie> {
    let profiles = find_firefox_profiles();

    for profile_dir in profiles {
        let tmp_path = match copy_cookies_to_temp(&profile_dir) {
            Some(p) => p,
            None => continue,
        };

        let result = query_cookie(&tmp_path, cookie_name, domain);

        // Clean up temp files
        let _ = std::fs::remove_file(&tmp_path);
        let _ = std::fs::remove_file(format!("{}-wal", tmp_path.display()));
        let _ = std::fs::remove_file(format!("{}-shm", tmp_path.display()));

        if let Some(cookie) = result {
            return Some(cookie);
        }
    }

    None
}

fn query_cookie(
    db_path: &PathBuf,
    cookie_name: &str,
    domain: &str,
) -> Option<BrowserCookie> {
    let conn = rusqlite::Connection::open(db_path).ok()?;

    let mut stmt = conn
        .prepare(
            "SELECT name, value, host FROM moz_cookies
             WHERE name = ?1 AND host = ?2
             ORDER BY lastAccessed DESC LIMIT 1",
        )
        .ok()?;

    let row = stmt
        .query_row(rusqlite::params![cookie_name, domain], |row| {
            Ok(BrowserCookie {
                value: row.get(1)?,
            })
        })
        .ok()?;

    Some(row)
}

/// Discover OpenCode workspaces from Firefox's browsing history (places.sqlite).
///
/// Looks for URLs matching /workspace/<wrk_...>/go and extracts the workspace IDs.
/// Returns deduplicated workspaces sorted by most recently visited.
pub fn discover_workspaces() -> Vec<super::WorkspaceInfo> {
    let profiles = find_firefox_profiles();
    let mut all_workspaces = Vec::new();
    let mut seen_ids = std::collections::HashSet::new();

    for profile_dir in profiles {
        let places_path = profile_dir.join("places.sqlite");
        if !places_path.exists() {
            continue;
        }

        // Copy to temp (places.sqlite is also WAL-locked while Firefox runs)
        let tmp = std::env::temp_dir().join(format!(
            "yapcap-ff-places-{}-{}.sqlite",
            profile_dir.file_name().and_then(|n| n.to_str()).unwrap_or("unknown"),
            std::process::id()
        ));

        if std::fs::copy(&places_path, &tmp).is_err() {
            continue;
        }
        // Copy WAL/SHM if present
        let wal = profile_dir.join("places.sqlite-wal");
        let shm = profile_dir.join("places.sqlite-shm");
        if wal.exists() {
            let _ = std::fs::copy(&wal, format!("{}-wal", tmp.display()));
        }
        if shm.exists() {
            let _ = std::fs::copy(&shm, format!("{}-shm", tmp.display()));
        }

        let conn = match rusqlite::Connection::open(&tmp) {
            Ok(c) => c,
            Err(_) => {
                let _ = std::fs::remove_file(&tmp);
                continue;
            }
        };

        // Query history for workspace URLs, extract wrk_ IDs
        if let Ok(mut stmt) = conn.prepare(
            "SELECT DISTINCT url FROM moz_places
             WHERE url LIKE '%opencode.ai/workspace/wrk_%'
             ORDER BY last_visit_date DESC NULLS LAST"
        ) {
            if let Ok(rows) = stmt.query_map([], |row| {
                let url: String = row.get(0)?;
                Ok(url)
            }) {
                for row_url in rows.flatten() {
                    if let Some(id) = extract_workspace_id(&row_url) {
                        if seen_ids.insert(id.clone()) {
                            all_workspaces.push(super::WorkspaceInfo {
                                id,
                                name: None,
                            });
                        }
                    }
                }
            }
        }

        // Clean up
        let _ = std::fs::remove_file(&tmp);
        let _ = std::fs::remove_file(format!("{}-wal", tmp.display()));
        let _ = std::fs::remove_file(format!("{}-shm", tmp.display()));
    }

    all_workspaces
}

/// Extract a workspace ID from a URL like https://opencode.ai/workspace/wrk_XXXXX/go
fn extract_workspace_id(url: &str) -> Option<String> {
    let marker = "/workspace/";
    let start = url.find(marker)? + marker.len();
    let rest = &url[start..];
    // Take everything up to the next / or end of string
    let end = rest.find('/').unwrap_or(rest.len());
    let id = &rest[..end];
    if id.starts_with("wrk_") && id.len() > 4 {
        Some(id.to_string())
    } else {
        None
    }
}