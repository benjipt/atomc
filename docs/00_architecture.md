# Local Atomic Commit Service - Architecture / Tech Spec

## Summary
Build a local-first service in Rust that accepts a repo path and/or a git diff, computes the diff locally when needed, produces an atomic commit plan via a local LLM (DeepSeek Coder by default), and optionally executes `git add` / `git commit` for each atomic unit. The service supports a CLI by default and an optional localhost HTTP server for repeated calls.

## Goals
- Easy local setup and distribution (single Rust binary).
- Deterministic and safe git behavior (dry-run, clear staging).
- Pluggable LLM runtime (Ollama first, with optional llama.cpp).
- High-quality atomic commit grouping + conventional commit messages.
- Clear separation between planning and execution for safety.

## Non-Goals
- Remote SaaS hosting or multi-tenant deployment.
- Full git workflow automation beyond commit creation (rebase, push, PRs).
- UI/desktop app in the initial version.

## High-Level Architecture
```
   [Codex / CLI caller]  --->  [CLI Interface]  --->  [Core Orchestrator]
                                 |                       |
                                 |                       +--> [LLM Adapter] -> [Runtime: Ollama/llama.cpp]
                                 |                       |
                                 |                       +--> [Git Adapter] -> git commands
                                 |
                              [HTTP Server] (optional)
```

## Components

### 1) CLI Interface
- Default entrypoint.
- Accepts input diff via stdin/`--diff-file` or computes diff from `--repo`.
- Supports diff mode and untracked inclusion for repo-derived diffs.
- Accepts repo path and flags (dry-run, execute).
- Emits plan (JSON or human-friendly).

### 2) HTTP Server (optional)
- `serve` subcommand starts a local JSON API.
- Keeps the LLM runtime warm across repeated calls.
- Exposes endpoints for planning and execution.

### 3) Core Orchestrator
Responsibilities:
- Validate inputs.
- Build prompt/context payload for the LLM.
- Parse model output into a structured `CommitPlan`.
- Optionally execute the plan via Git Adapter.

### 4) LLM Adapter
- Abstracts the runtime backend (Ollama first).
- Handles request/response translation.
- Enforces a strict JSON output schema.

### 5) Git Adapter
- Wraps `git` CLI commands.
- Supports staging by file, hunk, or path.
- Ensures commit atomicity by validating staged diffs.
- Supports dry-run mode.

## Data Flow
1. Caller provides repo path and/or diff; if repo path is provided and diff is absent, the service computes it (mode=all, include_untracked=true).
2. Orchestrator validates inputs and builds the prompt.
3. LLM returns a structured `CommitPlan`.
4. Orchestrator validates the plan and returns it.
5. If execution requested, Git Adapter performs:
   - Stage relevant files/hunks
   - Verify `git diff --staged`
   - Commit with specified message
   - Repeat for each atomic unit

## API Contract (Draft)

### CLI
```
atomc plan --repo . --format json
atomc apply --repo . --dry-run
atomc apply --repo . --execute
git diff | atomc plan --format json
```

### HTTP
`POST /v1/commit-plan`
```json
{
  "repo_path": "/path/to/repo",
  "diff": "<optional git diff text>",
  "diff_mode": "all",
  "include_untracked": true,
  "log_diff": false,
  "git_status": "<optional git status>",
  "model": "qwen2.5-coder:14b",
  "dry_run": true
}
```
If `diff` is omitted, the server computes it from the repo using
`diff_mode` and `include_untracked`.

Response:
```json
{
  "schema_version": "v1",
  "request_id": "req_123",
  "input": {
    "source": "repo",
    "diff_mode": "all",
    "include_untracked": true,
    "diff_hash": "sha256:..."
  },
  "plan": [
    {
      "id": "commit-1",
      "type": "feat",
      "scope": "auth",
      "summary": "add token refresh handler",
      "body": [
        "Add refresh flow in auth service",
        "Wire refresh to login middleware"
      ],
      "files": ["src/auth.rs", "src/middleware.rs"],
      "hunks": []
    }
  ],
  "warnings": []
}
```

`POST /v1/commit-apply`
```json
{
  "repo_path": "/path/to/repo",
  "diff": "<optional git diff text>",
  "diff_mode": "all",
  "include_untracked": true,
  "log_diff": false,
  "plan": [ /* optional; same as above */ ],
  "execute": true
}
```
If `plan` is omitted, the server computes a plan from `diff` or the repo.
If `diff` is also omitted, the server computes the diff from the repo
using `diff_mode` and `include_untracked`.

## Commit Message Rules
- Conventional commits: `type[scope]: summary`
- 50-72 char summary limit.
- Body bullet list (1-3 bullets).
- Optional "Assisted by: <model>" line appended.
- Scope is required unless change is truly global.

## Prompting Strategy (Outline)
- System prompt: instructs JSON-only output matching schema.
- Provide diff, status, and repo hints (if given).
- Explicitly ask for atomic commits with no cross-cutting changes.

## Safety and Guardrails
- `--dry-run` by default in early versions.
- Validate that staged diff matches planned files/hunks.
- Refuse to commit if staged changes drift from plan.
- Cleanup staging on error (optional flag).

## Configuration
- `LOCAL_COMMIT_MODEL`: e.g., `qwen2.5-coder:14b`
- `LOCAL_COMMIT_RUNTIME`: `ollama` | `llama.cpp`
- `LOCAL_COMMIT_OLLAMA_URL`: base URL for Ollama or llama.cpp (default `http://localhost:11434`)
- `LOCAL_COMMIT_MAX_TOKENS`, `LOCAL_COMMIT_TEMPERATURE`
- `LOCAL_COMMIT_LLM_TIMEOUT_SECS`, `LOCAL_COMMIT_MAX_DIFF_BYTES`
- `LOCAL_COMMIT_DIFF_MODE`, `LOCAL_COMMIT_INCLUDE_UNTRACKED`

## Observability
- Structured logs (JSON optional).
- Log request/response IDs.
- Redact diffs unless `log_diff` is enabled.

## Testing
- Unit tests: plan parsing, schema validation.
- Golden tests: diff-to-plan with fixtures.
- Integration tests: temp git repo commit application.

## Open Questions
- Should Git Adapter use `git add -p` for hunk-level staging?
- How strict should schema validation be for LLM output?

## Milestones
1. CLI plan-only flow with Ollama backend.
2. Apply flow with git staging + commit.
3. HTTP server mode.
4. Optional llama.cpp backend.
