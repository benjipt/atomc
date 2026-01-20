# Commit Strategy Specification

> Note: This document is outdated. The current MVP spec starts at `docs/09_mvp_human_first.md`.

## Purpose
Define the canonical rules for creating atomic commits from a diff. This document mirrors the rules in the atomic-commit-agent skill and serves as the policy layer for the local commit service.

## Scope
Applies to all commits generated or executed by the service, whether in CLI or server mode.

## Atomicity Rules
- Each commit must do exactly one thing.
- If changes span multiple concerns, split them into separate commits.
- When working bottom-up, commit foundations first and integration last.
- Avoid bundling refactors with feature changes.

## Foundations vs. Integration
Foundations are changes that support later integration and do not break existing functionality on their own. A foundation commit must keep the project in a working state.

Examples of foundations:
- Documentation updates for a planned feature.
- A new utility/helper module with no runtime wiring yet.
- Constants/config additions used by a later integration step.
- A new component file not yet integrated into a live service or app.

Examples of integration:
- Wiring a new utility into CLI/HTTP handlers.
- Enabling a feature flag or routing path that activates new behavior.

## Workflow
1. Inspect current changes (status + diff).
2. Group changes into atomic units.
3. For each unit:
   - Stage only relevant files/hunks.
   - Verify staged diff matches the plan.
   - Commit with the message format below.

## Commit Message Format
Use conventional commits with a required scope:

```
type[scope]: imperative summary (50-72 chars)

- Bullet describing what changed
- CLI/tools used if relevant
- LLM/model used if AI-assisted (only if provided)
```

### Commit Types
- feat: new feature
- fix: bug fix
- refactor: restructure without behavior change
- style: formatting only
- docs: documentation only
- test: tests
- chore: maintenance/tooling
- build: build/deps
- perf: performance
- ci: CI/CD

### Scope Guidelines
- Use lowercase, kebab-case scopes (example: [auth], [contact-form]).
- Omit scope only if the change is truly global.

## Model Attribution
- The invoking prompt must specify which model assisted in generating code.
- Do not co-author commits. Add an `Assisted by: <model>` line at the end.
- If no model is specified, the system must request it before committing.
- Never use the runner's own model name.

Example:
```
Assisted by: GPT-5
```

## Verification Checklist
1. Atomicity: the commit does exactly one thing.
2. Completeness: no broken imports or syntax errors.
3. Separation: unrelated changes are split.
4. Layering: foundations are not bundled with integration.

## Do Not Commit
- Mixed feature and refactor changes.
- Incomplete implementations or broken states.
- Debug statements or stray console logs.
- Unrelated formatting bundled with logic.
- Foundational logic bundled with UI/routing/integration.

## Failure Handling
- If staged diff does not match the planned unit, abort and report.
- If scope/model attribution is missing, request clarification before commit.
- If the plan contains mixed concerns, reject and re-plan.
