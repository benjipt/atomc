# CLI Specification

## Goals
- Provide a stable, agent-friendly CLI with JSON-first output.
- Keep MVP scope simple while preserving a path for future expansion.
- Make local execution safe by default.

## Binary Name
- Current: `atomc`
- Note: Keep commands and flags consistent if the name changes later.

## Command Overview
```
atomc plan  [options]
atomc apply [options]
atomc serve [options]
```

## Standard Flags
- `--help`: show command usage.
- `--version`: show version and exit.
- `--config <path>`: explicit config file path.
- `--log-level trace|debug|info|warn|error`
- `--quiet`: suppress non-error logs.
- `--no-color`: disable ANSI color in human output.
- `--timeout <seconds>`: LLM request timeout (plan/apply, and per-request
  in serve).

## Global Conventions
- Output format defaults to JSON on stdout.
- Logs and diagnostics go to stderr.
- Diff input can be provided or computed from `--repo`.
- If both diff and repo are provided, the explicit diff is used.
- `apply` defaults to dry-run; use `--execute` to commit.

## Input
### Diff Source
- By default, read diff from stdin.
- Optional: `--diff-file <path>` to read from a file.
- If both stdin and `--diff-file` are provided, error.
- `--diff-file -` is treated as stdin.
- If no diff is provided and `--repo` is set, atomc computes the diff.

### Diff Mode (repo-derived diffs)
- `--diff-mode worktree|staged|all` (default: all)
- `all` includes both staged and unstaged changes.
- `--include-untracked` (default) includes new files.
- `--no-include-untracked` excludes untracked files.

### Repo Path
- `--repo <path>` is optional for `plan` unless no diff is provided.
- `--repo <path>` is required for `apply` (even in dry-run).
- Repo-derived diffs use `diff_mode` and `include_untracked`.

### Input Rules
- If neither a diff nor `--repo` is provided, error.
- If a diff is provided, `diff_mode` and `include_untracked` are ignored.
- Empty diffs are rejected (including repo-derived diffs).
- Diff size is bounded by `max_diff_bytes` (config/env); default is 2,000,000.

## Output
### JSON (default)
- `plan` returns a JSON `CommitPlan` payload.
- `apply` returns a JSON report with plan, actions, and results.
- `plan`/`apply` include `input` metadata when available (source, mode,
  untracked, diff hash).
- All JSON outputs include `schema_version` and `request_id` when available.
- CLI generates `request_id` per invocation for JSON output; `serve`
  echoes `X-Request-Id` or generates one when absent.
- Errors are JSON with machine-readable codes.

### Schema Versioning
- `schema_version` is required on all JSON responses.
- Initial value: `v1`.

### Error Schema
```
{
  "schema_version": "v1",
  "request_id": "req_123",
  "error": {
    "code": "input_invalid",
    "message": "stdin is empty",
    "details": {
      "hint": "pipe a git diff, use --diff-file, or pass --repo"
    }
  }
}
```

### Error Codes (initial)
- `usage_error`
- `input_invalid`
- `config_error`
- `llm_runtime_error`
- `llm_parse_error`
- `git_error`
- `timeout`

### Human-readable
- `--format human` prints summaries and bullets for direct use.
- Intended for manual CLI usage; less stable for automation.

## Commands

### `plan`
Generate an atomic commit plan from a diff (provided or derived).

Required:
- Diff via stdin/`--diff-file`, or `--repo` to compute one.

Options:
- `--repo <path>`: optional repo metadata or diff source.
- `--diff-mode worktree|staged|all` (repo diff only)
- `--include-untracked` / `--no-include-untracked` (repo diff only)
- `--format json|human` (default: json)
- `--model <name>` (overrides config/env)
- `--dry-run` (no side effects; default behavior)
- `--timeout <seconds>` (overrides config/env)

### `apply`
Generate a plan and optionally execute it via git.

Required:
- `--repo <path>` (repo to operate on)
- Diff via stdin/`--diff-file` is optional; if absent, atomc computes it.

Options:
- `--execute` (perform git staging + commits)
- `--diff-mode worktree|staged|all` (repo diff only)
- `--include-untracked` / `--no-include-untracked` (repo diff only)
- `--format json|human` (default: json)
- `--model <name>` (overrides config/env)
- `--cleanup-on-error` (optional; defaults off)
- `--timeout <seconds>` (overrides config/env)

Behavior:
- Without `--execute`, the command produces a plan and reports
  intended actions without modifying the repo.
- With `--execute`, commits are created in plan order.
- atomc snapshots the diff and aborts if the worktree changes or the
  staged diff does not match the plan (regardless of diff source).

### `serve`
Run a local HTTP server for repeated requests.

Options:
- `--host <addr>` (default: 127.0.0.1)
- `--port <port>` (default: 49152)
- `--model <name>` (overrides config/env)
- `--log-format json|text`
- `--request-timeout <seconds>` (default: 60)

Notes:
- Keeps the LLM runtime warm across calls.
- Intended for agent integrations (Codex, Claude Code, etc.).

## Configuration

### Precedence
1) CLI flags
2) Environment variables
3) Config file
4) Defaults

### Defaults (MVP)
Defaults apply when a value is not provided via CLI, env, or config.

| Setting | Default | Notes |
| --- | --- | --- |
| model | deepseek-coder | LLM model name |
| runtime | ollama | LLM runtime backend |
| ollama_url | http://localhost:11434 | LLM base URL (Ollama or llama.cpp) |
| max_tokens | 2048 | Tokens per request |
| temperature | 0.2 | Low randomness for stable plans |
| llm_timeout_secs | 60 | Seconds |
| max_diff_bytes | 2000000 | Bytes |
| diff_mode | all | worktree, staged, or all |
| include_untracked | true | Include new files in repo-derived diffs |

Rationale: a low temperature favors consistent, conservative commit
planning in the MVP while still allowing minor variation in phrasing.

### Default Config Location
- Convention: user-scoped by default, OS-native when possible, and
  override-friendly via `--config` or environment variables.
- macOS: `~/Library/Application Support/atomc/config.toml`
- Linux (future): `$XDG_CONFIG_HOME/atomc/config.toml`,
  or `~/.config/atomc/config.toml`

### Environment Variables (initial)
- `LOCAL_COMMIT_MODEL`
- `LOCAL_COMMIT_RUNTIME`
- `LOCAL_COMMIT_OLLAMA_URL` (base URL for Ollama or llama.cpp)
- `LOCAL_COMMIT_MAX_TOKENS`
- `LOCAL_COMMIT_TEMPERATURE`
- `LOCAL_COMMIT_LLM_TIMEOUT_SECS`
- `LOCAL_COMMIT_MAX_DIFF_BYTES`
- `LOCAL_COMMIT_DIFF_MODE`
- `LOCAL_COMMIT_INCLUDE_UNTRACKED`
- `LOCAL_COMMIT_AGENT_CONFIG` (explicit config file path)

### Config File Format
Example `config.toml`:
```toml
model = "deepseek-coder"
runtime = "ollama"
ollama_url = "http://localhost:11434"
max_tokens = 2048
temperature = 0.2
llm_timeout_secs = 60
max_diff_bytes = 2000000
diff_mode = "all"
include_untracked = true
```

## Exit Codes (MVP)
- `0`: success
- `2`: usage/argument error
- `3`: input validation error (diff, repo path, config)
- `4`: LLM runtime error (request/transport)
- `5`: LLM output parse/validation error
- `6`: git error (stage/commit/verify)
- `7`: config error (missing/invalid)
- `130`: interrupted (SIGINT)

## Examples
```
atomc plan --repo . --format json
atomc apply --repo . --dry-run
atomc apply --repo . --execute
git diff | atomc plan --format json
atomc plan --repo . --diff-mode staged --no-include-untracked
```
