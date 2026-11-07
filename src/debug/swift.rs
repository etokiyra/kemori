//! Swift debug plans.

use std::path::{Path, PathBuf};

use super::*;

/// Build a Swift launch plan from a SwiftPM or standalone build.
pub(super) fn swift_plan(path: &Path, root: &Path) -> Result<LaunchPlan, String> {
    let program = swift_debug_binary(root, path).ok_or_else(|| {
        "No Swift debug build found; run `swift build` first (expected .build/debug/<executable>)"
            .to_string()
    })?;
    let adapter = crate::language::tools::lldb_dap_path().ok_or_else(|| {
        "No Swift debug adapter found — install `lldb-dap` (or `lldb-vscode`) or set KEMORI_DEBUG_ADAPTER".to_string()
    })?;
    Ok(swift_plan_with(program, adapter))
}

/// Build a Swift plan from already-resolved paths; used by tests.
pub fn swift_plan_with(program: PathBuf, adapter: PathBuf) -> LaunchPlan {
    lldb_plan(program, adapter)
}

/// Find the executable a Swift build produced.
fn swift_debug_binary(root: &Path, source: &Path) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    collect_executables(&root.join(".build/debug"), &mut candidates);
    if let Ok(entries) = std::fs::read_dir(root.join(".build")) {
        for entry in entries.flatten() {
            let dir = entry.path();
            if dir.is_dir() {
                collect_executables(&dir.join("debug"), &mut candidates);
            }
        }
    }
    // Prefer the newest executable; a SwiftPM build can leave several tools in
    // `.build/debug`, and the most recently linked one is the usual target.
    candidates.sort_by_key(|path| {
        std::fs::metadata(path)
            .and_then(|metadata| metadata.modified())
            .ok()
            .map(std::cmp::Reverse)
    });
    candidates
        .into_iter()
        .next()
        .or_else(|| conventional_binary(source))
}

fn collect_executables(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if std::fs::metadata(&path)
                .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
                .unwrap_or(false)
            {
                out.push(path);
            }
        }
        #[cfg(not(unix))]
        {
            out.push(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swift_plan_uses_lldb_dap() {
        let plan = swift_plan_with(
            PathBuf::from("/tmp/demo"),
            PathBuf::from("/opt/llvm/bin/lldb-dap"),
        );
        assert_eq!(plan.adapter_id, "lldb");
        assert_eq!(plan.label, "lldb-dap");
        assert_eq!(plan.program, PathBuf::from("/tmp/demo"));
    }

    #[cfg(unix)]
    #[test]
    fn finds_the_newest_swiftpm_executable() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("kemori-swift-build-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".build/debug")).unwrap();
        std::fs::write(dir.join("Package.swift"), "// swift-tools-version: 6.0\n").unwrap();
        for name in ["tool", "app"] {
            let path = dir.join(".build/debug").join(name);
            std::fs::write(&path, "").unwrap();
            let mut permissions = std::fs::metadata(&path).unwrap().permissions();
            permissions.set_mode(0o755);
            std::fs::set_permissions(&path, permissions).unwrap();
        }
        let found = swift_debug_binary(&dir, &dir.join("Sources/App/main.swift"));
        assert!(
            found.is_some(),
            "a SwiftPM debug executable should be discovered"
        );
        let _ = std::fs::remove_dir_all(dir);
    }
}
