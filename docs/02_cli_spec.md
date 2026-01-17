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
- Input diff is required for `plan` and `apply`.
- The tool does not compute diffs in MVP; caller provides diff.
- `apply` defaults to dry-run; use `--execute` to commit.

## Input
### Diff Source
- By default, read diff from stdin.
- Optional: `--diff-file <path>` to read from a file.
- If both stdin and `--diff-file` are provided, error.
- `--diff-file -` is treated as stdin.

### Repo Path
- `--repo <path>` is optional for `plan`.
- `--repo <path>` is required for `apply` (even in dry-run).
- The CLI does not compute a diff from the repo in MVP.

### Input Rules
- If stdin is empty and `--diff-file` is not provided, error.
- Empty diffs are rejected.
- Diff size is bounded by `max_diff_bytes` (config/env); default is 2,000,000.

## Output
### JSON (default)
- `plan` returns a JSON `CommitPlan` payload.
- `apply` returns a JSON report with plan, actions, and results.
- All JSON outputs include `schema_version` and `request_id` when available.
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
      "hint": "pipe a git diff or use --diff-file"
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
Generate an atomic commit plan from a diff.

Required:
- Diff via stdin or `--diff-file`.

Options:
- `--repo <path>`: optional repo metadata.
- `--format json|human` (default: json)
- `--model <name>` (overrides config/env)
- `--dry-run` (no side effects; default behavior)
- `--timeout <seconds>` (overrides config/env)

### `apply`
Generate a plan and optionally execute it via git.

Required:
- Diff via stdin or `--diff-file`.
- `--repo <path>` (repo to operate on)

Options:
- `--execute` (perform git staging + commits)
- `--format json|human` (default: json)
- `--model <name>` (overrides config/env)
- `--cleanup-on-error` (optional; defaults off)
- `--timeout <seconds>` (overrides config/env)

Behavior:
- Without `--execute`, the command produces a plan and reports
  intended actions without modifying the repo.
- With `--execute`, commits are created in plan order.

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

### Default Config Location
- Convention: user-scoped by default, OS-native when possible, and
  override-friendly via `--config` or environment variables.
- macOS: `~/Library/Application Support/atomc/config.toml`
- Linux (future): `$XDG_CONFIG_HOME/atomc/config.toml`,
  or `~/.config/atomc/config.toml`

### Environment Variables (initial)
- `LOCAL_COMMIT_MODEL`
- `LOCAL_COMMIT_RUNTIME`
- `LOCAL_COMMIT_OLLAMA_URL`
- `LOCAL_COMMIT_MAX_TOKENS`
- `LOCAL_COMMIT_TEMPERATURE`
- `LOCAL_COMMIT_LLM_TIMEOUT_SECS`
- `LOCAL_COMMIT_MAX_DIFF_BYTES`
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
git diff | atomc plan --format json
git diff | atomc apply --repo . --dry-run
git diff | atomc apply --repo . --execute
atomc serve --port 49152
```
