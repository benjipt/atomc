use atomc_core::config::DiffMode;
use atomc_core::git::{apply_plan, compute_diff, ApplyRequest, GitError};
use atomc_core::hash::diff_hash;
use atomc_core::types::{ApplyStatus, CommitType, CommitUnit};
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
    std::env::temp_dir().join(format!("atomc-apply-{prefix}-{nanos}-{count}"))
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

    fs::write(dir.join("file.txt"), "one\n").unwrap();
    run_git(&dir, &["add", "file.txt"]);
    run_git(&dir, &["commit", "-qm", "init"]);

    fs::write(dir.join("file.txt"), "one\ntwo\n").unwrap();

    dir
}

fn sample_plan() -> Vec<CommitUnit> {
    vec![CommitUnit {
        id: "commit-1".to_string(),
        type_: CommitType::Docs,
        scope: Some("cli".to_string()),
        summary: "document apply execution flow and expected git outputs".to_string(),
        body: vec![
            "Update apply usage info".to_string(),
            "Note git execution ordering".to_string(),
        ],
        files: vec!["file.txt".to_string()],
        hunks: Vec::new(),
    }]
}

#[test]
fn apply_plan_creates_commit() {
    let repo = setup_repo();
    let diff = compute_diff(&repo, DiffMode::Worktree, false).unwrap();
    let plan = sample_plan();
    let request = ApplyRequest {
        repo: &repo,
        plan: &plan,
        diff: &diff,
        diff_mode: DiffMode::Worktree,
        include_untracked: false,
        expected_diff_hash: Some(diff_hash(&diff)),
        cleanup_on_error: false,
        assisted_by: None,
    };

    let results = apply_plan(request).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].status, ApplyStatus::Applied);
    assert!(results[0].commit_hash.as_ref().unwrap().len() > 6);

    fs::remove_dir_all(&repo).ok();
}

#[test]
fn apply_plan_appends_assisted_by_line() {
    let repo = setup_repo();
    let diff = compute_diff(&repo, DiffMode::Worktree, false).unwrap();
    let plan = sample_plan();
    let request = ApplyRequest {
        repo: &repo,
        plan: &plan,
        diff: &diff,
        diff_mode: DiffMode::Worktree,
        include_untracked: false,
        expected_diff_hash: Some(diff_hash(&diff)),
        cleanup_on_error: false,
        assisted_by: Some("qwen2.5-coder:14b"),
    };

    let results = apply_plan(request).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].status, ApplyStatus::Applied);

    let output = Command::new("git")
        .current_dir(&repo)
        .args(["log", "-1", "--pretty=%B"])
        .output()
        .expect("git log");
    assert!(output.status.success());
    let message = String::from_utf8_lossy(&output.stdout);
    assert!(message.contains("Assisted by: qwen2.5-coder:14b"));

    fs::remove_dir_all(&repo).ok();
}

#[test]
fn apply_plan_rejects_changed_diff() {
    let repo = setup_repo();
    let diff = compute_diff(&repo, DiffMode::Worktree, false).unwrap();
    let plan = sample_plan();
    fs::write(repo.join("file.txt"), "one\ntwo\nthree\n").unwrap();

    let request = ApplyRequest {
        repo: &repo,
        plan: &plan,
        diff: &diff,
        diff_mode: DiffMode::Worktree,
        include_untracked: false,
        expected_diff_hash: Some(diff_hash(&diff)),
        cleanup_on_error: false,
        assisted_by: None,
    };

    let error = apply_plan(request).unwrap_err();
    assert!(matches!(error, GitError::DiffHashMismatch { .. }));

    fs::remove_dir_all(&repo).ok();
}

#[test]
fn apply_plan_rejects_diff_input_mismatch() {
    let repo = setup_repo();
    let diff = "diff --git a/file.txt b/file.txt\n";
    let plan = sample_plan();

    let request = ApplyRequest {
        repo: &repo,
        plan: &plan,
        diff,
        diff_mode: DiffMode::Worktree,
        include_untracked: false,
        expected_diff_hash: Some(diff_hash(diff)),
        cleanup_on_error: false,
        assisted_by: None,
    };

    let error = apply_plan(request).unwrap_err();
    assert!(matches!(error, GitError::DiffHashMismatch { .. }));

    fs::remove_dir_all(&repo).ok();
}

#[test]
fn apply_plan_cleans_up_on_error() {
    let repo = setup_repo();
    fs::write(repo.join("extra.txt"), "extra\n").unwrap();
    run_git(&repo, &["add", "extra.txt"]);

    let diff = compute_diff(&repo, DiffMode::Worktree, false).unwrap();
    let plan = sample_plan();

    let request = ApplyRequest {
        repo: &repo,
        plan: &plan,
        diff: &diff,
        diff_mode: DiffMode::Worktree,
        include_untracked: false,
        expected_diff_hash: Some(diff_hash(&diff)),
        cleanup_on_error: true,
        assisted_by: None,
    };

    let error = apply_plan(request).unwrap_err();
    assert!(matches!(error, GitError::StagedFilesMismatch { .. }));

    let staged = list_staged_files(&repo);
    assert!(staged.iter().any(|file| file == "extra.txt"));
    assert!(!staged.iter().any(|file| file == "file.txt"));

    fs::remove_dir_all(&repo).ok();
}

fn list_staged_files(repo: &PathBuf) -> Vec<String> {
    let output = Command::new("git")
        .current_dir(repo)
        .args(["diff", "--staged", "--name-only", "-z"])
        .output()
        .expect("git diff failed");
    let status_ok = output.status.success() || matches!(output.status.code(), Some(1));
    assert!(status_ok, "git diff failed");
    let stdout = String::from_utf8(output.stdout).expect("utf-8");
    stdout
        .split('\0')
        .filter(|entry| !entry.is_empty())
        .map(|entry| entry.to_string())
        .collect()
}
