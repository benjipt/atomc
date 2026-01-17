# Git Adapter Specification

This document defines how atomc interacts with git to stage, verify, and
commit atomic units safely.

## Goals
- Stage only the changes described by a commit unit.
- Verify staged content matches the planned diff.
- Keep the repo consistent on errors (cleanup is explicit).
- Support both file- and hunk-level staging in the future.

## Scope
Applies to `apply` and server-side commit execution.

## Inputs
- `repo_path` (required)
- `plan` (required)
- `execute` (bool)
- `cleanup_on_error` (bool)
- `input` metadata (optional): diff hash, source, mode, untracked

## Safety Model
- The adapter snapshots the diff used to generate the plan (or receives
  it from the orchestrator) and computes `diff_hash` (SHA-256) for the
  full diff.
- Before apply and before each commit, recompute the current diff using
  the same diff settings and compare against `input.diff_hash`.
- Before each commit, verify the staged diff matches the plan’s
  file/hunk selection.

## Staging Strategy (MVP)
- Stage by file path only.
- For each commit unit:
  1) Clear index for target files: `git reset -q -- <files>`.
  2) Stage target files: `git add -- <files>`.
  3) Verify `git diff --staged` matches expected file list and plan selection.
- Hunk-based staging is deferred; `hunks` should be empty in MVP.

## Verification Rules
- If `input.diff_hash` is present, compute a fresh hash of the current
  repo diff and compare; mismatch aborts.
- Staged diff must only include files listed in the commit unit.
- For file-level staging, staged diff must be a subset of the original
  plan diff used to generate the plan.

## Execution Flow
For each commit unit in order:
1) Stage relevant files.
2) Verify staged diff.
3) Commit with conventional message from the plan.
4) If any step fails:
   - Abort apply.
   - If `cleanup_on_error` is set, reset index for files staged by atomc.

## Commit Message Construction
- Format: `type[scope]: summary`.
- Body is 1–3 lines from `body`.
- Append `Assisted by: <model>` only if provided by the caller.

## Untracked Files
- When `include_untracked` is true, untracked files listed in `files`
  are allowed and staged via `git add -- <files>`.
- If an untracked file is not in the plan, it must not be staged.

## Command Inventory (MVP)
- `git status --porcelain=v1`
- `git diff`
- `git diff --staged`
- `git add -- <files>`
- `git reset -q -- <files>`
- `git commit -m <summary> -m <body>`

## Error Handling
- If verification fails, emit `git_error` with context (file list,
  expected hash, actual hash).
- If git returns non-zero, include stderr in `details`.
