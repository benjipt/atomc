# Human-First MVP Draft

> Note: This document is outdated. The current MVP spec starts at `docs/09_mvp_human_first.md`.

## Documentation Status
This document supersedes the earlier specs in `docs/00_` through `docs/08_`.
Those documents are now considered outdated. New work should start from this
MVP draft only.

## Purpose
Define a simplified, human-first MVP for atomc that prioritizes ease of use,
fast feedback, and clear commit quality over agent-friendly automation.

## Audience
- Developers who want a local commit assistant for daily use.
- Engineers evaluating the practicality of local-first LLM tooling.

## Non-Goals
- Agent-first JSON contracts.
- Full HTTP server workflows as the primary UX.
- Advanced staging (hunks) or multi-repo orchestration.

## Primary Workflow
One command, one flow:
1) Run a dry-run plan from the current repo diff.
2) Show a human-friendly plan.
3) Prompt the user to apply or abort.

Example UX:
```
atomc

Apply plan (2 commits):
1. docs[readme]: clarify paused status
   Add goals and learnings section
   Update future MVP expectations
   files: README.md

2. feat[core]: validate commit files against diff list
   Reject commits that reference files outside the diff
   Add semantic validation coverage
   files: crates/atomc-core/src/semantic.rs, crates/atomc-core/tests/semantic_validation.rs

Apply these commits? [y/N]
```

## MVP Command Surface
- `atomc` (default dry-run + prompt; uses current working directory only)
- `--execute` (apply immediately; skips prompt)
- `--diff-mode worktree|staged|all`
- `--no-include-untracked` (opt-out)
- `--timeout <seconds>`

## Output Defaults
- Human-readable output by default.
- JSON output is out of scope for the MVP.
- Logs to stderr, output to stdout.

## Setup Expectations
- Single CLI binary.
- Local LLM runtime required (Ollama or llama.cpp).
- No atomc server required for the primary workflow.

## Quality Gates
The plan is rejected and retried if any of the following fail:
- Any commit unit omits required human fields (type/scope, summary, 1–3 body
  lines, files list) or uses malformed values.
- Commit files are not present in the diff.
- Summary/body formatting rules are violated.
- Empty plan or non-atomic grouping is detected.

## Performance Constraints
Targets for interactive use:
- Median response under 5s for small diffs.
- One retry maximum on validation failures.
- Prompt size trimmed; avoid embedding long patch text when possible.

## Open Questions
- What model/runtime combinations consistently meet the quality gates?
- How strict should atomicity heuristics be before deferring to the user?
