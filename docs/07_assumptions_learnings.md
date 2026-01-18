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

## Assumption: Schema-valid output implies useful plan quality
- Observation: Model returned schema-valid JSON with placeholder fields
  (`file1`, generic summaries) that did not match the diff.
- Resolution: Pending. Next steps:
  - Add semantic validation that file paths must appear in the diff.
  - Tighten the prompt to require file paths taken verbatim from the diff.
- Status: Investigating.

## Assumption: Model adheres to commit message conventions by default
- Observation: Output used section labels and bullet formatting in body lines,
  and sometimes included `type[scope]:` in summary text.
- Resolution: Pending. Tracked in `docs/08_todo.md` under plan quality.
- Status: Investigating.

## Assumption: deepseek-coder:latest follows JSON-only output with schema
- Observation: Returned prose or schema-mismatched JSON despite JSON mode,
  including alternate top-level keys (`commits`) and non-JSON output.
- Resolution: Moved default model away from deepseek-coder:latest; added
  schema-based formatting and stricter prompts.
- Status: Mitigated; no longer default.

## Assumption: deepseek-coder:6.7b yields stable commit grouping
- Observation: Produced inconsistent commit grouping and formatting, including
  section-labeled body lines and over-splitting small doc changes.
- Resolution: Use qwen2.5-coder:14b as the default while evaluating models.
- Status: Mitigated; no longer default.
