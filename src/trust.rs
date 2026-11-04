//! Workspace trust.
//!
//! Running a project's tasks, starting its debug adapter or opening a shell in
//! its root executes code the project ships. Kemori asks once per project root
//! and remembers the answer in its own state directory. The decision is
//! per-root, not global, and it never lives inside the project, so a cloned
//! repository cannot trust itself by editing a file.
//!
//! Untrusted projects remain fully editable: trust only gates execution.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

/// How many trusted roots are remembered. Old entries fall off the end.
const MAX_ROOTS: usize = 128;

/// The set of project roots the user has trusted.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TrustStore {
    roots: Vec<PathBuf>,
}

impl TrustStore {
    /// Load the decisions from the user's state directory.
    pub fn load() -> TrustStore {
        path()
            .and_then(|path| TrustStore::load_from(&path))
            .unwrap_or_default()
    }

    /// Persist the decisions. Errors are non-fatal, matching the other stores.
    pub fn save(&self) {
        if let Some(path) = path() {
            let _ = self.save_to(&path);
        }
    }

    pub fn load_from(path: &Path) -> Option<TrustStore> {
        let text = std::fs::read_to_string(path).ok()?;
        let value: Value = serde_json::from_str(&text).ok()?;
        let roots = value
            .get("trusted")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(PathBuf::from)
                    .collect()
            })
            .unwrap_or_default();
        Some(TrustStore { roots })
    }

    pub fn save_to(&self, path: &Path) -> std::io::Result<()> {
        let value = json!({
            "trusted": self.roots.iter().map(|p| p.to_string_lossy()).collect::<Vec<_>>(),
        });
        let text = serde_json::to_string_pretty(&value).map_err(std::io::Error::other)?;
        crate::filesystem::write_atomic(path, &text)
    }

    /// Whether `root` has been trusted.
    pub fn is_trusted(&self, root: &Path) -> bool {
        let root = canonical(root);
        self.roots.iter().any(|trusted| canonical(trusted) == root)
    }

    /// Trust `root`, persisting the decision.
    pub fn trust(&mut self, root: &Path) {
        let root = canonical(root);
        if !self.roots.iter().any(|trusted| canonical(trusted) == root) {
            self.roots.insert(0, root);
            self.roots.truncate(MAX_ROOTS);
        }
        self.save();
    }

    /// Forget a root.
    pub fn revoke(&mut self, root: &Path) {
        let root = canonical(root);
        self.roots.retain(|trusted| canonical(trusted) != root);
        self.save();
    }

    /// The remembered roots, most recent first.
    pub fn roots(&self) -> &[PathBuf] {
        &self.roots
    }
}

fn canonical(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn path() -> Option<PathBuf> {
    crate::recent::state_dir().map(|dir| dir.join("trust.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kemori-trust-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn trusts_and_revokes_a_root() {
        let dir = temp("roundtrip");
        let root = dir.join("project");
        std::fs::create_dir_all(&root).unwrap();
        let mut store = TrustStore::default();
        assert!(!store.is_trusted(&root));
        store.trust(&root);
        assert!(store.is_trusted(&root));
        store.revoke(&root);
        assert!(!store.is_trusted(&root));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn persists_to_disk() {
        let dir = temp("persist");
        let root = dir.join("project");
        std::fs::create_dir_all(&root).unwrap();
        let file = dir.join("trust.json");
        let mut store = TrustStore::default();
        store.trust(&root);
        store.save_to(&file).unwrap();
        let loaded = TrustStore::load_from(&file).unwrap();
        assert!(loaded.is_trusted(&root));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_malformed_store_loads_as_empty() {
        let dir = temp("malformed");
        let file = dir.join("trust.json");
        std::fs::write(&file, "not json").unwrap();
        assert!(TrustStore::load_from(&file).is_none());
        let _ = std::fs::remove_dir_all(dir);
    }
}
