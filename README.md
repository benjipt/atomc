# atomc

Local-first atomic commit planner and executor for git diffs.

## Overview
atomc is a local-first commit assistant that plans atomic commits from
the current working directory and can optionally execute those commits.
It is designed for safe, deterministic local workflows with a focus on
human-friendly usage.

## Status
Paused. This repository captures a spec-first MVP attempt and the
learnings from building and testing it. I plan to revisit the project
as local LLMs and runtimes improve.

## Key Concepts
- Human-first CLI output for manual use.
- Repo-derived diffs from the current working directory.
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

## Goals and Learnings
This project started as a spec-first MVP to validate whether a local
LLM could reliably plan atomic commits from a git diff and optionally
execute them. It surfaced practical constraints that make the current
approach a poor user experience without stronger local models and
faster runtimes:

- Local models struggled to produce consistently atomic commit plans and
  high-quality commit messages compared to hosted or agent-guided tools.
- Running the tool required a local LLM server, and the end-to-end
  latency was higher than expected for interactive use.
- The JSON-first, agent-friendly output default did not match the
  intended human workflow for the MVP.

The specs and docs remain as a reference for anyone exploring similar
local-first commit tooling.

## Future MVP (human-first)
If/when the project resumes, the MVP will prioritize a simple, human
workflow over agent integration:

- One primary command that defaults to a dry-run and then asks for
  confirmation to apply.
- Human-readable output by default; JSON support only as an opt-in.
- Minimal setup: a single CLI binary plus a local LLM runtime (no
  separate atomc server required).
- Faster feedback: smaller prompts, tighter timeouts, and fewer retries.
- Stronger quality gates: explicit validation for atomicity and clear
  messaging before any commits are applied.

See `docs/09_mvp_human_first.md` for the current MVP spec draft.

## Configuration
Configuration is file-based by default.

Default config path (macOS):
- `~/Library/Application Support/atomc/config.toml`

Defaults and configuration precedence are in `docs/02_cli_spec.md`.

### Runtime Notes
Ollama is the default runtime. To use llama.cpp, point the base URL at a
compatible server (OpenAI-style `/v1/chat/completions`) and set:
```
LOCAL_COMMIT_RUNTIME=llama.cpp
LOCAL_COMMIT_OLLAMA_URL=http://localhost:8080
```

## Docs
Note: `docs/00_` through `docs/08_` are legacy and outdated. Start with
`docs/09_mvp_human_first.md`.

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
