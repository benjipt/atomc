# JSON Schemas

Versioned schemas live under `schemas/v1/` and are referenced by
`atomc-core` at compile time. The `$id` fields are canonical identifiers
and do not need to match the repo layout.

When introducing a new schema version:
- Add a new `schemas/vX/` directory.
- Keep previous versions intact.
- Update `atomc-core` includes to point at the new version.
