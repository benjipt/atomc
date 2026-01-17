# atomc

Local-first atomic commit planner and executor for git diffs.

## Overview
atomc accepts a repo path and/or a git diff, uses a local LLM to group
changes into atomic commits, and can optionally execute those commits.
It is designed for safe, deterministic local workflows and for easy
integration with coding agents (Codex, Claude Code, etc.).

## Status
Spec-first MVP in progress. See the docs for current behavior and
contracts; implementation is forthcoming.

## Key Concepts
- JSON-first CLI output for automation.
- Repo-derived diffs by default (mode=all, include_untracked=true).
- Clear separation between planning and execution.
- Safe apply flow with diff snapshot verification.

## Requirements (MVP)
- macOS
- git
- Local LLM runtime (Ollama by default)
- Rust toolchain (for building from source)

## Installation
Implementation is in progress. Planned installation options:
- `cargo install` (first release)
- Prebuilt binaries (later)

## Quickstart (planned)
```
atomc plan --repo . --format json
atomc apply --repo . --dry-run
atomc apply --repo . --execute
atomc serve --port 49152
```

## Configuration
Configuration is file-based by default.

Default config path (macOS):
- `~/Library/Application Support/atomc/config.toml`

Defaults and configuration precedence are in `docs/02_cli_spec.md`.

## Docs
- `docs/00_architecture.md` — architecture and API contract
- `docs/01_commit_strategy.md` — atomic commit rules
- `docs/02_cli_spec.md` — CLI behavior, flags, and defaults
- `docs/03_schema.md` — JSON contract and schemas
- `docs/04_llm_prompting.md` — LLM prompt templates
- `docs/05_git_adapter.md` — git staging and verification strategy
- `docs/06_testing.md` — test plan and spec coverage

## Safety Notes
atomc snapshots the diff used to plan commits and verifies staged diffs
match the plan before committing. If the working tree changes between
planning and apply, the operation aborts to avoid unintended commits.

## Log Policy
Diff contents are redacted from logs by default. Use `--log-diff` or set
`LOCAL_COMMIT_LOG_DIFF=true` only when you are sure the diff does not
contain sensitive information.
