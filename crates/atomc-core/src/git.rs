use std::path::{Path, PathBuf};
use std::process::Command;

use crate::config::DiffMode;

#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("git command failed: {cmd}")]
    CommandFailed { cmd: String, stderr: String },
    #[error("git command io error: {cmd}: {source}")]
    CommandIo { cmd: String, source: std::io::Error },
    #[error("git output was not utf-8")]
    OutputNotUtf8,
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

fn run_git_with_extra_paths(
    repo: &Path,
    args: &[&str],
    extra_paths: &[PathBuf],
    allow_exit_1: bool,
) -> Result<String, GitError> {
    let mut cmd = Command::new("git");
    cmd.current_dir(repo).args(args);
    if !extra_paths.is_empty() {
        for path in extra_paths {
            cmd.arg(path);
        }
    }
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

    let status_ok = output.status.success()
        || (allow_exit_1 && matches!(output.status.code(), Some(1)));
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
