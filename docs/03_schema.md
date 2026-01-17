# Schema Specification

This document defines the JSON contracts returned by `atomc` and the
HTTP server. It is the canonical reference for agent integrations.

## Versioning
- Every JSON response includes `schema_version`.
- Initial value: `v1`.
- Clients must ignore unknown fields for forward compatibility.

## Common Response Fields
All responses include `schema_version`. `request_id` and `warnings` may
be present when available.

```json
{
  "schema_version": "v1",
  "request_id": "req_123",
  "warnings": []
}
```

Fields:
- `schema_version` (string, required)
- `request_id` (string, optional): trace id for logs.
- `warnings` (array, optional): non-fatal issues.

### Warning Object
```json
{
  "code": "scope_missing",
  "message": "Commit scope omitted for a global change",
  "details": {
    "commit_id": "commit-2"
  }
}
```

Fields:
- `code` (string, required)
- `message` (string, required)
- `details` (object, optional)

## Request ID
- CLI: generated per command invocation for JSON output.
- Server: generated per HTTP request; if `X-Request-Id` is provided,
  it is echoed back in responses.
- Format: opaque string; UUID or ULID recommended. A `req_` prefix is
  allowed but not required.

## Commit Plan Response
Returned by `atomc plan` and `/v1/commit-plan`.

```json
{
  "schema_version": "v1",
  "request_id": "req_123",
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

### Commit Unit
Fields:
- `id` (string, required): unique within the plan.
- `type` (string, required): conventional commit type.
- `scope` (string or null, required): kebab-case or null for global.
- `summary` (string, required): 50-72 chars, imperative.
- `body` (array of strings, required): 1-3 bullet lines.
- `files` (array of strings, required): repo-relative paths.
- `hunks` (array, required): optional hunk targets (may be empty).

### Commit Types
Allowed values for `type`:
`feat`, `fix`, `refactor`, `style`, `docs`, `test`, `chore`, `build`,
`perf`, `ci`.

### Hunk Target (optional)
Hunks allow more precise staging. In MVP this may be empty.

```json
{
  "file": "src/auth.rs",
  "header": "@@ -10,6 +10,12 @@",
  "id": "hunk-1"
}
```

Fields:
- `file` (string, required): repo-relative path.
- `header` (string, required): git hunk header line.
- `id` (string, optional): stable identifier for the hunk.

## Commit Apply Response
Returned by `atomc apply` and `/v1/commit-apply`.

```json
{
  "schema_version": "v1",
  "request_id": "req_456",
  "plan": [ /* same as Commit Plan */ ],
  "results": [
    {
      "id": "commit-1",
      "status": "applied",
      "commit_hash": "abc123",
      "error": null
    }
  ],
  "warnings": []
}
```

### Result Object
Fields:
- `id` (string, required): commit id from the plan.
- `status` (string, required): `planned`, `applied`, `skipped`, `failed`.
- `commit_hash` (string, optional): git hash when applied.
- `error` (object or null, optional): error details if failed.

## Error Response
Used for any failure; never mixed with a success payload.

```json
{
  "schema_version": "v1",
  "request_id": "req_789",
  "error": {
    "code": "input_invalid",
    "message": "stdin is empty",
    "details": {
      "hint": "pipe a git diff or use --diff-file"
    }
  }
}
```

### Error Object
Fields:
- `code` (string, required): machine-readable error code.
- `message` (string, required): human-readable summary.
- `details` (object, optional): additional context.

### Error Codes (initial)
- `usage_error`
- `input_invalid`
- `config_error`
- `llm_runtime_error`
- `llm_parse_error`
- `git_error`
- `timeout`

## Notes
- Scope is required unless the change is truly global; set `scope` to
  null for global changes.
- Clients should treat unknown fields as optional for forward
  compatibility.
