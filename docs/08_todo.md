# TODOs

## Plan Quality Improvements
- Enforce file paths in `files[]` must appear in the diff file list; reject and
  retry on violations.
- Reject body lines that start with `-` or include section labels (e.g.
  `Resolution:`); require short imperative statements.
- Prefer minimal commits for single-file or single-concern diffs; if all units
  touch the same file set, collapse to a single commit (via retry).
- Reject summaries that include a `type[scope]:` prefix; summary should be the
  message text only.
