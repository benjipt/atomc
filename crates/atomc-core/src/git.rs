use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::config::DiffMode;
use crate::hash;
use crate::types::{ApplyResult, ApplyStatus, CommitType, CommitUnit, InputSource};

#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("git command failed: {cmd}")]
    CommandFailed { cmd: String, stderr: String },
    #[error("git command io error: {cmd}: {source}")]
    CommandIo { cmd: String, source: std::io::Error },
    #[error("git output was not utf-8")]
    OutputNotUtf8,
    #[error("diff hash mismatch: expected {expected}, actual {actual}")]
    DiffHashMismatch { expected: String, actual: String },
    #[error("plan includes unsupported hunks for commit {id}")]
    HunksNotSupported { id: String },
    #[error("plan file not found in diff for commit {id}: {file}")]
    PlanFileMissing { id: String, file: String },
    #[error("staged files do not match plan for commit {id}")]
    StagedFilesMismatch {
        id: String,
        expected: Vec<String>,
        actual: Vec<String>,
    },
    #[error("staged diff is empty for commit {id}")]
    StagedDiffEmpty { id: String },
}

pub struct ApplyRequest<'a> {
    pub repo: &'a Path,
    pub plan: &'a [CommitUnit],
    pub diff: &'a str,
    pub source: InputSource,
    pub diff_mode: DiffMode,
    pub include_untracked: bool,
    pub expected_diff_hash: Option<String>,
    pub cleanup_on_error: bool,
}

pub fn compute_diff(repo: &Path, mode: DiffMode, include_untracked: bool) -> Result<String, GitError> {
    let mut parts = Vec::new();

    match mode {
        DiffMode::Worktree => {
            let diff = run_git_diff(repo, &["diff"], &[])?;
            push_if_non_empty(&mut parts, diff);
        }
        DiffMode::Staged => {
            let diff = run_git_diff(repo, &["diff", "--staged"], &[])?;
            push_if_non_empty(&mut parts, diff);
        }
        DiffMode::All => {
            let diff = run_git_diff(repo, &["diff"], &[])?;
            let staged = run_git_diff(repo, &["diff", "--staged"], &[])?;
            push_if_non_empty(&mut parts, diff);
            push_if_non_empty(&mut parts, staged);
        }
    }

    if include_untracked {
        let untracked = list_untracked_files(repo)?;
        for path in untracked {
            let diff = run_git_diff(repo, &["diff", "--no-index", "--", "/dev/null"], &[path])?;
            push_if_non_empty(&mut parts, diff);
        }
    }

    Ok(parts.join("\n"))
}

pub fn apply_plan(request: ApplyRequest<'_>) -> Result<Vec<ApplyResult>, GitError> {
    let expected_hash = request
        .expected_diff_hash
        .unwrap_or_else(|| hash::diff_hash(request.diff));
    let diff_files = diff_files(request.diff);

    verify_diff_hash(
        request.repo,
        &request.source,
        request.diff_mode,
        request.include_untracked,
        &expected_hash,
    )?;

    let mut results = Vec::new();
    for unit in request.plan {
        verify_diff_hash(
            request.repo,
            &request.source,
            request.diff_mode,
            request.include_untracked,
            &expected_hash,
        )?;
        if !unit.hunks.is_empty() {
            return Err(GitError::HunksNotSupported { id: unit.id.clone() });
        }

        for file in &unit.files {
            if !diff_files.contains(file) {
                return Err(GitError::PlanFileMissing {
                    id: unit.id.clone(),
                    file: file.clone(),
                });
            }
        }

        let file_paths: Vec<PathBuf> = unit.files.iter().map(|file| request.repo.join(file)).collect();
        if let Err(error) = stage_files(request.repo, &file_paths)
            .and_then(|_| verify_staged_files(request.repo, unit))
            .and_then(|_| commit_unit(request.repo, unit))
            .and_then(|hash| {
                results.push(ApplyResult {
                    id: unit.id.clone(),
                    status: ApplyStatus::Applied,
                    commit_hash: Some(hash),
                    error: None,
                });
                Ok(())
            })
        {
            if request.cleanup_on_error {
                let _ = reset_files(request.repo, &file_paths);
            }
            return Err(error);
        }
    }

    Ok(results)
}

fn list_untracked_files(repo: &Path) -> Result<Vec<PathBuf>, GitError> {
    let output = run_git(repo, &["status", "--porcelain=v1", "-z"])?;
    let mut paths = Vec::new();
    for entry in output.split('\0') {
        if entry.is_empty() {
            continue;
        }
        if let Some(rest) = entry.strip_prefix("?? ") {
            paths.push(repo.join(rest));
        }
    }
    Ok(paths)
}

fn verify_diff_hash(
    repo: &Path,
    source: &InputSource,
    diff_mode: DiffMode,
    include_untracked: bool,
    expected: &str,
) -> Result<(), GitError> {
    if matches!(source, InputSource::Diff) {
        return Ok(());
    }

    let current = compute_diff(repo, diff_mode, include_untracked)?;
    let actual = hash::diff_hash(&current);
    if actual != expected {
        return Err(GitError::DiffHashMismatch {
            expected: expected.to_string(),
            actual,
        });
    }
    Ok(())
}

fn stage_files(repo: &Path, files: &[PathBuf]) -> Result<(), GitError> {
    if files.is_empty() {
        return Ok(());
    }
    run_git_with_extra_paths(repo, &["reset", "-q", "--"], files, false)?;
    run_git_with_extra_paths(repo, &["add", "--"], files, false)?;
    Ok(())
}

fn reset_files(repo: &Path, files: &[PathBuf]) -> Result<(), GitError> {
    if files.is_empty() {
        return Ok(());
    }
    run_git_with_extra_paths(repo, &["reset", "-q", "--"], files, false)?;
    Ok(())
}

fn verify_staged_files(repo: &Path, unit: &CommitUnit) -> Result<(), GitError> {
    let staged = list_staged_files(repo)?;
    if staged.is_empty() {
        return Err(GitError::StagedDiffEmpty { id: unit.id.clone() });
    }

    let expected: HashSet<String> = unit.files.iter().cloned().collect();
    let actual: HashSet<String> = staged.iter().cloned().collect();
    if !actual.is_subset(&expected) || actual.is_empty() {
        return Err(GitError::StagedFilesMismatch {
            id: unit.id.clone(),
            expected: unit.files.clone(),
            actual: staged,
        });
    }
    Ok(())
}

fn list_staged_files(repo: &Path) -> Result<Vec<String>, GitError> {
    let output = run_git_with_extra_paths(repo, &["diff", "--staged", "--name-only", "-z"], &[], true)?;
    let mut files = Vec::new();
    for entry in output.split('\0') {
        if !entry.is_empty() {
            files.push(entry.to_string());
        }
    }
    Ok(files)
}

fn commit_unit(repo: &Path, unit: &CommitUnit) -> Result<String, GitError> {
    let header = commit_header(unit);
    let cmd_string = format!("git commit -m {}", header);
    let mut cmd = Command::new("git");
    cmd.current_dir(repo).arg("commit").arg("-m").arg(&header);
    for line in &unit.body {
        cmd.arg("-m").arg(line);
    }
    let output = cmd.output().map_err(|source| GitError::CommandIo {
        cmd: cmd_string.clone(),
        source,
    })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(GitError::CommandFailed {
            cmd: cmd_string,
            stderr,
        });
    }

    let hash = run_git(repo, &["rev-parse", "HEAD"])?;
    Ok(hash.trim().to_string())
}

fn commit_header(unit: &CommitUnit) -> String {
    let type_str = commit_type_str(&unit.type_);
    match unit.scope.as_deref() {
        Some(scope) => format!("{type_str}[{scope}]: {}", unit.summary),
        None => format!("{type_str}: {}", unit.summary),
    }
}

fn commit_type_str(commit_type: &CommitType) -> &'static str {
    match commit_type {
        CommitType::Feat => "feat",
        CommitType::Fix => "fix",
        CommitType::Refactor => "refactor",
        CommitType::Style => "style",
        CommitType::Docs => "docs",
        CommitType::Test => "test",
        CommitType::Chore => "chore",
        CommitType::Build => "build",
        CommitType::Perf => "perf",
        CommitType::Ci => "ci",
    }
}

fn diff_files(diff: &str) -> HashSet<String> {
    let mut files = HashSet::new();
    for line in diff.lines() {
        if let Some(rest) = line.strip_prefix("diff --git ") {
            let mut parts = rest.split_whitespace();
            let a_path = parts.next();
            let b_path = parts.next();
            if let Some(path) = normalize_diff_path(a_path, b_path) {
                files.insert(path);
            }
        }
    }
    files
}

fn normalize_diff_path(a_path: Option<&str>, b_path: Option<&str>) -> Option<String> {
    let candidate = b_path.or(a_path)?;
    let stripped = candidate
        .strip_prefix("b/")
        .or_else(|| candidate.strip_prefix("a/"))
        .unwrap_or(candidate);
    if stripped == "/dev/null" || stripped.is_empty() {
        None
    } else {
        Some(stripped.to_string())
    }
}

fn run_git_with_extra_paths(
    repo: &Path,
    args: &[&str],
    extra_paths: &[PathBuf],
    allow_exit_1: bool,
) -> Result<String, GitError> {
    let mut cmd = Command::new("git");
    cmd.current_dir(repo).args(args);
    cmd.args(extra_paths);
    let cmd_string = format!(
        "git {}{}",
        args.join(" "),
        if extra_paths.is_empty() {
            "".to_string()
        } else {
            format!(
                " -- {}",
                extra_paths
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>()
                    .join(" ")
            )
        }
    );

    let output = cmd.output().map_err(|source| GitError::CommandIo {
        cmd: cmd_string.clone(),
        source,
    })?;

    // `git diff` exits 1 when changes are present; allow that for diff commands.
    let status_ok =
        output.status.success() || (allow_exit_1 && matches!(output.status.code(), Some(1)));
    if !status_ok {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(GitError::CommandFailed {
            cmd: cmd_string,
            stderr,
        });
    }

    String::from_utf8(output.stdout).map_err(|_| GitError::OutputNotUtf8)
}

fn run_git(repo: &Path, args: &[&str]) -> Result<String, GitError> {
    run_git_with_extra_paths(repo, args, &[], false)
}

fn push_if_non_empty(target: &mut Vec<String>, diff: String) {
    if !diff.trim().is_empty() {
        target.push(diff);
    }
}

fn run_git_diff(repo: &Path, args: &[&str], extra_paths: &[PathBuf]) -> Result<String, GitError> {
    run_git_with_extra_paths(repo, args, extra_paths, true)
}
