# claas-rollant-46

## Agent skills (patdhlk-skills)

- Issue backend: **sphinx-needs**. Config + role map: `ubproject.toml` →
  `[tool.patdhlk-skills]`. Resolve directives ONLY through the role map.
- Query the needs corpus via needs.json, never by grepping RST:
  `ubc build needs --outpath spec/_build/needs/needs.json` (or
  `uv run sphinx-build -b needs spec spec/_build/needs`), then `jq` on
  `spec/_build/needs/needs.json`. Rebuild before every query.
  (`pds build` once pds is installed.)
- New need IDs: dense max+1 per prefix, from a fresh needs.json.
- Every spec mutation must end with the strict gate:
  `uv run sphinx-build -W -b html spec spec/_build/html` (or `pds check`
  once pds is installed). Exit 0 = clean, non-zero = fix the corpus and
  re-run.
- Issues live in `spec/issues/index.rst`; `:status:` carries the triage
  state machine: needs-triage → needs-info | ready-for-agent |
  ready-for-human → in-progress → done | wontfix. Edit status in place —
  git history is the audit trail.
