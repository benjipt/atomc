# Assumptions and Learnings

This document tracks assumptions discovered during implementation and testing,
the evidence that disproved or confirmed them, and the resulting resolution.

## Assumption: Ollama returns JSON-only when prompted
- Observation: Model replied with prose like "Here is a JSON representation..."
  and JSON parse failed at column 1.
- Resolution: Force JSON output via Ollama `format` field. Start with `"json"`,
  then upgrade to the full CommitPlan JSON schema to enforce keys.
- Status: Resolved; keep format schema in requests.

## Assumption: JSON mode guarantees schema-compliant keys
- Observation: Model produced `{"commits": ...}` instead of the required
  `{"schema_version":"v1","plan":[...]}` despite JSON mode.
- Resolution: Embed CommitPlan JSON schema in the Ollama `format` field and
  tighten the system prompt to explicitly require `schema_version` and `plan`.
- Status: Resolved; schema-based format now enforced.

## Assumption: Schema-valid plan will pass semantic validation
- Observation: Model returned schema-valid JSON but with empty `id` and `scope`,
  triggering semantic validation errors.
- Resolution: Strengthened the prompt with explicit non-empty `id`/`scope`
  requirements and added a single retry that re-prompts with semantic error
  details.
- Status: Mitigated; re-test with Ollama to confirm.

## Assumption: Schema formatting enforces scope constraints
- Observation: Model returned schema-valid JSON but with a non-kebab-case
  `scope`, triggering semantic validation errors.
- Resolution: Added a kebab-case regex pattern to the schema for `scope` to
  align schema enforcement with semantic validation, and clarified the prompt
  with an explicit definition and example.
- Status: Mitigated; re-test with Ollama to confirm.

## Assumption: JSON schema formatting prevents invalid JSON output
- Observation: Model returned invalid JSON (unterminated strings) likely due to
  verbose output and inclusion of diff-like text.
- Resolution: Emphasized empty `hunks` and no patch text in the prompt, added
  a schema constraint (`hunks` maxItems=0), and introduced a retry on parse
  errors with stricter instructions.
- Status: Mitigated; re-test with Ollama to confirm.
