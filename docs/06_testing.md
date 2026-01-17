# Testing and Spec Coverage

This document outlines the recommended test strategy for atomc, with a
focus on correctness, safety, and deterministic behavior.

## Goals
- Validate strict schema compliance for all JSON outputs.
- Ensure atomic commit planning rules are enforced.
- Verify safe git staging and commit behavior.
- Keep tests deterministic without network calls.

## Test Types

### Unit Tests
- Schema validation for plan/apply/error responses.
- Prompt assembly and output parsing.
- Diff hash computation.
- Input rules (diff vs repo, diff_mode, include_untracked).

### Golden Tests
- Diff-to-plan with fixed fixtures.
- Validate output plan equality against golden JSON.
- Cover foundational vs integration ordering.

### Integration Tests
- CLI plan/apply flows with a temporary git repo.
- Git adapter staging and verification behavior.
- HTTP server endpoints (plan/apply) with mocked LLM.

## Test Fixtures

### Diff Fixtures
Store in `tests/fixtures/diffs/`:
- `simple_feature.diff`
- `mixed_concerns.diff`
- `refactor_plus_feature.diff`
- `untracked_files.diff`

### Plan Fixtures
Store in `tests/fixtures/plans/`:
- `simple_feature.plan.json`
- `mixed_concerns.plan.json`
- `refactor_plus_feature.plan.json`

## LLM Mocking
- Use a deterministic mock that returns fixture JSON.
- For error cases, return malformed JSON or schema-invalid output.
- Avoid calling Ollama during tests.

## CLI Test Coverage
- `plan` with stdin diff.
- `plan` with `--diff-file`.
- `plan` with `--repo` and computed diff (diff_mode all, include_untracked).
- `apply` dry-run with repo-derived diff.
- `apply` execute with staged diff verification.
- Verify `--format human` output is non-JSON.

## HTTP Test Coverage
- `POST /v1/commit-plan` with diff provided.
- `POST /v1/commit-plan` with repo-only (server computes diff).
- `POST /v1/commit-apply` with plan provided.
- `POST /v1/commit-apply` with diff only (server computes plan).
- Request ID echo via `X-Request-Id`.

## Git Adapter Coverage
- Stage only files listed in a commit unit.
- Reject staged diffs that include extra files.
- Abort if worktree changes after planning.
- Verify untracked file handling when enabled/disabled.
- Cleanup behavior when `cleanup_on_error` is set.

## Error Handling Coverage
- Input validation errors (no diff and no repo).
- LLM runtime errors (mocked transport failure).
- LLM parse errors (invalid JSON).
- Git errors (non-zero exit codes).
- Timeout behavior for LLM calls.

## Spec Compliance Checks
- Enforce `schema_version` on all responses.
- Enforce summary length 50–72 chars.
- Enforce `body` line count 1–3.
- Ensure `scope` is present unless global (null).

## Test Utilities
- `tempfile`-based git repo helper.
- Fixture loader for diffs/plans.
- JSON schema validator helper.

## Future Coverage
- Hunk-based staging once supported.
- Multi-commit plan apply with partial failure simulation.
- Performance tests for large diffs.
