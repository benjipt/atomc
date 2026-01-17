use atomc_core::config::DiffMode;
use atomc_core::git::compute_diff;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_dir(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let count = COUNTER.fetch_add(1, Ordering::SeqCst);
    std::env::temp_dir().join(format!("atomc-git-{prefix}-{nanos}-{count}"))
}

fn run_git(repo: &PathBuf, args: &[&str]) {
    let status = Command::new("git")
        .current_dir(repo)
        .args(args)
        .status()
        .expect("git command failed to start");
    assert!(status.success(), "git command failed: git {}", args.join(" "));
}

fn setup_repo() -> PathBuf {
    let dir = temp_dir("repo");
    fs::create_dir_all(&dir).unwrap();
    run_git(&dir, &["init", "-q"]);
    run_git(&dir, &["config", "user.email", "atomc@example.com"]);
    run_git(&dir, &["config", "user.name", "atomc"]);

    fs::write(dir.join("tracked.txt"), "one\n").unwrap();
    run_git(&dir, &["add", "tracked.txt"]);
    run_git(&dir, &["commit", "-qm", "init"]);

    fs::write(dir.join("tracked.txt"), "one\ntwo\n").unwrap();

    fs::write(dir.join("staged.txt"), "staged\n").unwrap();
    run_git(&dir, &["add", "staged.txt"]);

    fs::write(dir.join("untracked.txt"), "untracked\n").unwrap();

    dir
}

#[test]
fn compute_diff_worktree_includes_unstaged_only() {
    let repo = setup_repo();
    let diff = compute_diff(&repo, DiffMode::Worktree, false).unwrap();

    assert!(diff.contains("tracked.txt"));
    assert!(!diff.contains("staged.txt"));
    assert!(!diff.contains("untracked.txt"));

    fs::remove_dir_all(&repo).ok();
}

#[test]
fn compute_diff_staged_includes_staged_only() {
    let repo = setup_repo();
    let diff = compute_diff(&repo, DiffMode::Staged, false).unwrap();

    assert!(!diff.contains("tracked.txt"));
    assert!(diff.contains("staged.txt"));
    assert!(!diff.contains("untracked.txt"));

    fs::remove_dir_all(&repo).ok();
}

#[test]
fn compute_diff_all_includes_all_changes() {
    let repo = setup_repo();
    let diff = compute_diff(&repo, DiffMode::All, true).unwrap();

    assert!(diff.contains("tracked.txt"));
    assert!(diff.contains("staged.txt"));
    assert!(diff.contains("untracked.txt"));

    fs::remove_dir_all(&repo).ok();
}
