//! Recently opened projects and files.
//!
//! A small global store — not per project — so the welcome screen can offer
//! where you were last. It lives alongside the per-project sessions in the
//! user's state directory and is written when Kemori quits.
//!
//! The format is a tiny JSON object parsed without derive macros, matching the
//! session store's approach.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

/// How many recent projects to remember.
pub const MAX_PROJECTS: usize = 8;
/// How many recent files to remember.
pub const MAX_FILES: usize = 12;

/// Recently opened projects and files, most recent first.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Recent {
    pub projects: Vec<PathBuf>,
    pub files: Vec<PathBuf>,
}

impl Recent {
    /// Load the store from the user's state directory, if available.
    pub fn load() -> Recent {
        state_path("recent.json")
            .and_then(|path| Recent::load_from(&path))
            .unwrap_or_default()
    }

    /// Persist the store. Errors are non-fatal.
    pub fn save(&self) {
        if let Some(path) = state_path("recent.json") {
            let _ = self.save_to(&path);
        }
    }

    pub fn load_from(path: &Path) -> Option<Recent> {
        let text = std::fs::read_to_string(path).ok()?;
        let value: Value = serde_json::from_str(&text).ok()?;
        Some(Recent {
            projects: string_list(value.get("projects")),
            files: string_list(value.get("files")),
        })
    }

    pub fn save_to(&self, path: &Path) -> std::io::Result<()> {
        let value = json!({
            "projects": self.projects.iter().map(|p| p.to_string_lossy()).collect::<Vec<_>>(),
            "files": self.files.iter().map(|p| p.to_string_lossy()).collect::<Vec<_>>(),
        });
        let text = serde_json::to_string_pretty(&value).map_err(std::io::Error::other)?;
        crate::filesystem::write_atomic(path, &text)
    }

    /// Record a project root, moving it to the front.
    pub fn add_project(&mut self, path: &Path) {
        push_unique(&mut self.projects, path, MAX_PROJECTS);
    }

    /// Record a file, moving it to the front.
    pub fn add_file(&mut self, path: &Path) {
        push_unique(&mut self.files, path, MAX_FILES);
    }

    pub fn is_empty(&self) -> bool {
        self.projects.is_empty() && self.files.is_empty()
    }
}

/// Move `path` to the front, deduplicating and capping the list.
fn push_unique(list: &mut Vec<PathBuf>, path: &Path, max: usize) {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    list.retain(|existing| {
        existing.canonicalize().unwrap_or_else(|_| existing.clone()) != canonical
    });
    list.insert(0, canonical);
    list.truncate(max);
}

fn string_list(value: Option<&Value>) -> Vec<PathBuf> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str())
                .map(PathBuf::from)
                .collect()
        })
        .unwrap_or_default()
}

/// The Kemori state directory, if a home directory is available.
///
/// Shared with [`crate::settings`] and [`crate::session`] so all Kemori state
/// follows the same rule under `sudo` (see [`crate::privileges`]).
pub fn state_dir() -> Option<PathBuf> {
    crate::privileges::current_state_base().map(|base| base.join("kemori"))
}

/// The state directory used before the Kemori rename.
fn legacy_state_dir() -> Option<PathBuf> {
    crate::privileges::current_state_base().map(|base| base.join("koda"))
}

/// Import settings, sessions and recents from the pre-rename state directory.
///
/// This is a best-effort, one-time copy: existing Kemori files always win, the
/// old directory is never modified or removed, and a marker keeps a later launch
/// from repeating the work.
pub fn migrate_legacy_state() {
    let (Some(old), Some(new)) = (legacy_state_dir(), state_dir()) else {
        return;
    };
    let _ = migrate_state_dirs(&old, &new);
}

fn migrate_state_dirs(old: &Path, new: &Path) -> std::io::Result<bool> {
    let marker = new.join(".migrated-from-koda");
    if marker.exists() || !old.is_dir() {
        return Ok(false);
    }
    std::fs::create_dir_all(new)?;
    for entry in std::fs::read_dir(old)? {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if !kind.is_file() {
            continue;
        }
        let destination = new.join(entry.file_name());
        if destination.exists() {
            continue;
        }
        let _ = std::fs::copy(entry.path(), destination);
    }
    std::fs::write(marker, "Imported from the old Koda state directory.\n")?;
    Ok(true)
}

fn state_path(name: &str) -> Option<PathBuf> {
    state_dir().map(|dir| dir.join(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kemori-recent-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn legacy_state_is_imported_once_without_overwriting() {
        let old = scratch("legacy-old");
        let new = scratch("legacy-new");
        std::fs::write(old.join("settings.json"), "old").unwrap();
        std::fs::write(old.join("session-1.json"), "session").unwrap();
        std::fs::write(new.join("settings.json"), "new").unwrap();

        assert!(migrate_state_dirs(&old, &new).unwrap());
        assert_eq!(
            std::fs::read_to_string(new.join("settings.json")).unwrap(),
            "new",
            "a current file wins over the legacy copy"
        );
        assert_eq!(
            std::fs::read_to_string(new.join("session-1.json")).unwrap(),
            "session"
        );
        assert!(new.join(".migrated-from-koda").is_file());

        // A second launch does not import again.
        assert!(!migrate_state_dirs(&old, &new).unwrap());

        let _ = std::fs::remove_dir_all(&old);
        let _ = std::fs::remove_dir_all(&new);
    }

    #[test]
    fn round_trips_through_disk() {
        let dir = scratch("roundtrip");
        let path = dir.join("recent.json");
        let mut recent = Recent::default();
        recent.add_project(&dir.join("alpha"));
        recent.add_file(&dir.join("main.rs"));
        recent.save_to(&path).unwrap();
        let loaded = Recent::load_from(&path).unwrap();
        assert_eq!(loaded.projects.len(), 1);
        assert_eq!(loaded.files.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn moves_repeats_to_the_front_and_caps() {
        let dir = scratch("order");
        let mut recent = Recent::default();
        recent.add_project(&dir.join("a"));
        recent.add_project(&dir.join("b"));
        recent.add_project(&dir.join("a"));
        assert_eq!(recent.projects.len(), 2);
        assert!(recent.projects[0].ends_with("a"));
        for index in 0..(MAX_FILES + 5) {
            recent.add_file(&dir.join(format!("file-{index}")));
        }
        assert_eq!(recent.files.len(), MAX_FILES);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
