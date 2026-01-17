# LLM Prompting Specification

This document defines the prompt contract used to produce atomic commit
plans from a diff. It is designed for local LLMs (Ollama by default) and
prioritizes determinism, safety, and strict JSON output.

## Goals
- Produce a valid `CommitPlan` JSON object every time.
- Enforce atomic commit grouping rules from `docs/01_commit_strategy.md`.
- Keep output deterministic and easy to parse.
- Avoid leaking diffs or repo details unless explicitly requested.

## Prompt Principles
- JSON-only output; no prose, Markdown, or code fences.
- Strict adherence to the schema in `docs/03_schema.md`.
- Prefer fewer commits if it maintains atomicity.
- Foundations first, integrations last.
- Do not include "Assisted by" in plan output (added at commit time).

## System Prompt (Template)
```
You are a local commit planning assistant.
Return a single JSON object that matches the CommitPlan schema.
Do not include Markdown, comments, or any extra text.
Follow atomic commit rules:
- Each commit must do exactly one thing.
- Split unrelated concerns into separate commits.
- Foundations first, integrations last.
- Avoid bundling refactors with feature changes.
Commit message rules:
- Use conventional commits: type[scope]: summary
- Scope is required unless the change is truly global.
- Summary is imperative, 50-72 chars.
- Body is 1-3 short lines (no leading hyphens).
If any required field is unknown, infer the best value.
```

## User Prompt (Template)
```
You will be given a git diff and optional repo metadata.
Produce an atomic commit plan as JSON only.

Context:
- repo_path: {{repo_path | ""}}
- diff_mode: {{diff_mode | ""}}
- include_untracked: {{include_untracked | ""}}
- git_status: {{git_status | ""}}

Diff:
{{diff}}
```

## Output Contract
The response must be a JSON object matching `CommitPlan`:

```json
{
  "schema_version": "v1",
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
  ]
}
```

Notes:
- `schema_version` must be `v1`.
- `scope` may be null only for truly global changes.
- `body` entries are plain strings, no leading bullets.
- `files` must be repo-relative paths.
- `hunks` may be empty in MVP.

## Validation Rules
A plan is rejected if:
- Output is not valid JSON.
- Required fields are missing or mis-typed.
- `plan` is empty.
- Any commit mixes unrelated concerns.
- Any summary violates the 50-72 char rule.

## Examples

### Example A: Repo-derived diff
Input:
- diff_mode: all
- include_untracked: true
- diff: <git diff text>

Output:
```json
{
  "schema_version": "v1",
  "plan": [
    {
      "id": "commit-1",
      "type": "docs",
      "scope": "cli",
      "summary": "document plan and apply flags",
      "body": [
        "Add CLI usage examples",
        "Clarify diff input options"
      ],
      "files": ["docs/02_cli_spec.md"],
      "hunks": []
    }
  ]
}
```

### Example B: Multiple atomic commits
Output:
```json
{
  "schema_version": "v1",
  "plan": [
    {
      "id": "commit-1",
      "type": "refactor",
      "scope": "core",
      "summary": "extract plan parser helper",
      "body": [
        "Move JSON parsing into a dedicated module",
        "Keep external behavior unchanged"
      ],
      "files": ["src/plan.rs"],
      "hunks": []
    },
    {
      "id": "commit-2",
      "type": "feat",
      "scope": "cli",
      "summary": "add plan output format flag",
      "body": [
        "Expose --format json|human",
        "Wire format selection into CLI"
      ],
      "files": ["src/cli.rs"],
      "hunks": []
    }
  ]
}
```

## Retry Guidance
If the model output fails JSON validation:
- Re-run with a stricter system prompt that repeats "JSON only".
- Lower temperature or increase max tokens if the output is truncated.
- Do not attempt to auto-correct non-JSON prose; reject and retry.
