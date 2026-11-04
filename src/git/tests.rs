use super::*;

#[test]
fn parses_status_codes() {
    assert_eq!(parse_status("??"), GitFileStatus::Untracked);
    assert_eq!(parse_status(" M"), GitFileStatus::Modified);
    assert_eq!(parse_status("A "), GitFileStatus::Added);
    assert_eq!(parse_status("UU"), GitFileStatus::Conflicted);
    assert_eq!(parse_status("R "), GitFileStatus::Renamed);
}

/// Initialise a throwaway repository with a deterministic identity, or
/// return `None` when git is not installed.
fn temp_repo(name: &str) -> Option<PathBuf> {
    if std::process::Command::new("git")
        .arg("--version")
        .output()
        .is_err()
    {
        return None;
    }
    let dir = std::env::temp_dir().join(format!("kemori-git-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    run_git(&dir, &["init", "-q"]);
    run_git(&dir, &["config", "user.email", "kemori@example.com"]);
    run_git(&dir, &["config", "user.name", "Kemori Test"]);
    // Resolve symlinks (macOS exposes the temp dir as `/var` → `/private/var`)
    // so the paths a test builds match the repository root git reports.
    Some(std::fs::canonicalize(&dir).unwrap())
}

fn run_git(dir: &Path, args: &[&str]) {
    std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap();
}

#[test]
fn parses_unstaged_status_without_losing_the_first_entry() {
    // Regression: the whole status output used to be trimmed, which ate the
    // leading space that marks an *unstaged* change on the first line and
    // shifted its path.
    let Some(dir) = temp_repo("unstaged") else {
        return;
    };
    let modified = dir.join("a b file.txt");
    std::fs::write(&modified, "one\n").unwrap();
    run_git(&dir, &["add", "--", "a b file.txt"]);
    run_git(&dir, &["commit", "-qm", "init"]);
    std::fs::write(&modified, "two\n").unwrap();

    let info = GitInfo::detect(&dir);
    assert_eq!(info.files.get(&modified), Some(&GitFileStatus::Modified));
    assert!(
        !info.is_staged(&modified),
        "an unstaged modification must not be marked staged"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn parses_renames_and_non_ascii_paths() {
    let Some(dir) = temp_repo("rename") else {
        return;
    };
    std::fs::write(dir.join("old.txt"), "x\n").unwrap();
    run_git(&dir, &["add", "--", "old.txt"]);
    run_git(&dir, &["commit", "-qm", "init"]);
    run_git(&dir, &["mv", "old.txt", "café renommé.txt"]);

    let info = GitInfo::detect(&dir);
    let renamed = dir.join("café renommé.txt");
    assert_eq!(
        info.files.get(&renamed),
        Some(&GitFileStatus::Renamed),
        "a renamed non-ASCII file should be reported at its new path"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn stages_and_commits_when_git_is_available() {
    if std::process::Command::new("git")
        .arg("--version")
        .output()
        .is_err()
    {
        return; // Skip when git is unavailable.
    }
    let dir = std::env::temp_dir().join(format!("kemori-git-commit-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .arg("-C")
            .arg(&dir)
            .args(args)
            .output()
    };
    // A fresh repository with a deterministic identity.
    assert!(git(&["init", "-q"]).unwrap().status.success());
    assert!(
        git(&["config", "user.email", "kemori@example.com"])
            .unwrap()
            .status
            .success()
    );
    assert!(
        git(&["config", "user.name", "Kemori Test"])
            .unwrap()
            .status
            .success()
    );
    std::fs::write(dir.join("a.txt"), "hello\n").unwrap();

    assert!(stage_all(&dir).is_ok());
    let result = commit_all(&dir, "initial commit");
    assert!(result.is_ok(), "commit failed: {result:?}");

    let log = git(&["log", "--oneline"]).unwrap();
    assert!(String::from_utf8_lossy(&log.stdout).contains("initial commit"));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn diff_shows_working_tree_staged_and_untracked_changes() {
    if std::process::Command::new("git")
        .arg("--version")
        .output()
        .is_err()
    {
        return; // Skip when git is unavailable.
    }
    let dir = std::env::temp_dir().join(format!("kemori-git-diff-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .arg("-C")
            .arg(&dir)
            .args(args)
            .output()
    };
    assert!(git(&["init", "-q"]).unwrap().status.success());
    assert!(
        git(&["config", "user.email", "kemori@example.com"])
            .unwrap()
            .status
            .success()
    );
    assert!(
        git(&["config", "user.name", "Kemori Test"])
            .unwrap()
            .status
            .success()
    );
    let path = dir.join("a.txt");
    std::fs::write(&path, "one\n").unwrap();
    assert!(stage_all(&dir).is_ok());
    assert!(commit_all(&dir, "initial").is_ok());

    // Working-tree change.
    std::fs::write(&path, "one\ntwo\n").unwrap();
    let unstaged = diff(&dir, &path, false).unwrap();
    assert!(unstaged.contains("+two"), "unstaged diff: {unstaged}");
    assert!(diff(&dir, &path, true).unwrap().is_empty());

    // Staged change.
    stage(&dir, &path).unwrap();
    let staged = diff(&dir, &path, true).unwrap();
    assert!(staged.contains("+two"), "staged diff: {staged}");

    // Untracked file: the whole file is an addition.
    let fresh = dir.join("new.txt");
    std::fs::write(&fresh, "hello\nworld\n").unwrap();
    let untracked = diff(&dir, &fresh, false).unwrap();
    assert!(untracked.contains("+hello") && untracked.contains("+world"));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn stages_and_unstages_a_file() {
    if std::process::Command::new("git")
        .arg("--version")
        .output()
        .is_err()
    {
        return; // Skip when git is unavailable.
    }
    let dir = std::env::temp_dir().join(format!("kemori-git-stage-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    // Resolve symlinks (macOS exposes the temp dir as `/var` → `/private/var`)
    // so `path` matches the repository root git reports.
    let dir = std::fs::canonicalize(&dir).unwrap();
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .arg("-C")
            .arg(&dir)
            .args(args)
            .output()
    };
    assert!(git(&["init", "-q"]).unwrap().status.success());
    std::fs::write(dir.join("a.txt"), "hello\n").unwrap();
    let path = dir.join("a.txt");

    let info = GitInfo::detect(&dir);
    assert!(info.available);
    assert!(!info.is_staged(&path), "an untracked file is not staged");

    stage(&dir, &path).unwrap();
    assert!(GitInfo::detect(&dir).is_staged(&path));

    unstage(&dir, &path).unwrap();
    assert!(!GitInfo::detect(&dir).is_staged(&path));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn line_changes_reports_added_and_modified_lines() {
    if std::process::Command::new("git")
        .arg("--version")
        .output()
        .is_err()
    {
        return;
    }
    let dir = std::env::temp_dir().join(format!("kemori-git-lines-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let dir = std::fs::canonicalize(&dir).unwrap();
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .arg("-C")
            .arg(&dir)
            .args(args)
            .output()
    };
    assert!(git(&["init", "-q"]).unwrap().status.success());
    assert!(
        git(&["config", "user.email", "kemori@example.com"])
            .unwrap()
            .status
            .success()
    );
    assert!(
        git(&["config", "user.name", "Kemori Test"])
            .unwrap()
            .status
            .success()
    );

    let path = dir.join("a.txt");
    std::fs::write(&path, "one\ntwo\nthree\n").unwrap();
    assert!(git(&["add", "--", "a.txt"]).unwrap().status.success());
    assert!(git(&["commit", "-qm", "init"]).unwrap().status.success());

    // Modify line 2 and add a line.
    std::fs::write(&path, "one\ntwo changed\nthree\nfour\n").unwrap();

    let changes = line_changes(&dir, &path);
    let kinds: Vec<(usize, GitLineChange)> =
        changes.into_iter().map(|m| (m.line, m.change)).collect();
    assert!(
        kinds.contains(&(1, GitLineChange::Modified)),
        "line 2 modified: {kinds:?}"
    );
    assert!(
        kinds.contains(&(3, GitLineChange::Added)),
        "line 4 added: {kinds:?}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn applies_and_reverts_a_hunk() {
    if std::process::Command::new("git")
        .arg("--version")
        .output()
        .is_err()
    {
        return;
    }
    let dir = std::env::temp_dir().join(format!("kemori-git-patch-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let dir = std::fs::canonicalize(&dir).unwrap();
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .arg("-C")
            .arg(&dir)
            .args(args)
            .output()
    };
    assert!(git(&["init", "-q"]).unwrap().status.success());
    assert!(
        git(&["config", "user.email", "kemori@example.com"])
            .unwrap()
            .status
            .success()
    );
    assert!(
        git(&["config", "user.name", "Kemori Test"])
            .unwrap()
            .status
            .success()
    );

    let path = dir.join("a.txt");
    std::fs::write(&path, "one\ntwo\n").unwrap();
    assert!(git(&["add", "--", "a.txt"]).unwrap().status.success());
    assert!(git(&["commit", "-qm", "init"]).unwrap().status.success());
    std::fs::write(&path, "one\nthree\n").unwrap();

    let patch = diff(&dir, &path, false).unwrap();
    assert!(patch.contains("+three"), "{patch}");
    // `run_checked` trims its output; git apply wants the final newline.
    let patch = format!("{patch}\n");

    apply_patch(&dir, &patch, true, false).expect("revert");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "one\ntwo\n");

    apply_patch(&dir, &patch, false, false).expect("apply");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "one\nthree\n");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn line_changes_untracked_file_is_all_added() {
    if std::process::Command::new("git")
        .arg("--version")
        .output()
        .is_err()
    {
        return;
    }
    let dir = std::env::temp_dir().join(format!("kemori-git-untracked-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let dir = std::fs::canonicalize(&dir).unwrap();
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .arg("-C")
            .arg(&dir)
            .args(args)
            .output()
    };
    assert!(git(&["init", "-q"]).unwrap().status.success());

    let path = dir.join("new.txt");
    std::fs::write(&path, "a\nb\n").unwrap();

    let changes = line_changes(&dir, &path);
    assert_eq!(
        changes,
        vec![
            GitLineMarker {
                line: 0,
                change: GitLineChange::Added
            },
            GitLineMarker {
                line: 1,
                change: GitLineChange::Added
            },
        ]
    );

    let _ = std::fs::remove_dir_all(&dir);
}
