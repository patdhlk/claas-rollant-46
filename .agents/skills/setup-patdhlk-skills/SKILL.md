---
name: setup-patdhlk-skills
description: Configure a repo for the patdhlk-skills workflows - reads or scaffolds ubproject.toml, persists the issue backend (github or sphinx-needs) and the role map, sets up a sphinx-needs spec via uv, detects/installs ubc, and offers devcontainer and CI. Use when the user runs /setup-patdhlk-skills or asks to set up patdhlk-skills, configure the issue backend, or scaffold a sphinx-needs spec for agent workflows.
disable-model-invocation: true
---

# Setup patdhlk-skills

Run once per repo. Adapt to what exists — **never impose a need-type catalog**
(ADR_0004): read the user's `ubproject.toml` and map roles onto *their*
directives. Only add what is missing, and only with consent.

All templates referenced below live in [REFERENCE.md](REFERENCE.md).

## Workflow

### 1. Detect existing state

- `ubproject.toml` at repo root (or `spec*/`, `docs/`)? Parse `[needs.types]`
  and any existing `[tool.patdhlk-skills]` (re-run = update, keep prior answers
  as defaults).
- A Sphinx project using `sphinx_needs` (a `conf.py` mentioning it)? Note its
  source dir — that becomes `spec_dir`.
- `uv` available? A `pyproject.toml`? A git remote GitHub can see (`gh repo
  view`)? `ubc` on PATH? `pds` on PATH (`command -v pds`)?

### 2. Choose the issue backend

Ask the user: `github` or `sphinx-needs` (ADR_0003). Recommend `github` when
the repo has a GitHub remote with collaborators/public visibility; recommend
`sphinx-needs` for solo or spec-driven repos. Persist as `issue_backend`.

### 3. Ensure the spec exists

Requirements, ADRs, and glossary terms are **always** sphinx-needs regardless
of backend (ADR_0002), so a spec is required either way. If none exists,
scaffold per REFERENCE.md §1: root `ubproject.toml`, `spec/conf.py`,
`spec/schemas.json`, `spec/index.rst` + subdirs — and pin the toolchain with
`uv init` (non-package) + `uv add sphinx "sphinx-needs>=8,<9" furo
sphinx-autobuild`, committing `uv.lock`.

### 4. Build and persist the role map

For each role — issue, feature, requirement, decision, term, test — propose
the best-matching directive from the user's `[needs.types]` (by directive
name, then title, then prefix). Confirm the full map with the user in one
question; unmatched roles stay unmapped unless the user names a directive.
Persist under `[tool.patdhlk-skills.roles]` (REFERENCE.md §2).

If `issue_backend = "sphinx-needs"` and no directive maps to `issue`: offer to
add the `issue` type plus the status state-machine schema (ADR_0005) and an
`issues/` doc — templates in REFERENCE.md §3.

### 5. Configure the builder and the gate CLI

`ubc` on PATH → persist `builder = "ubc"`. Missing → on Linux offer the
install script (REFERENCE.md §4); otherwise persist `builder =
"sphinx-build"` and mention ubc is faster when available (ADR_0006).

`pds` is the gate-and-query CLI (ADR_0017): `pds check` is the strict gate,
`pds build` produces a fresh needs.json. It runs whichever builder is
configured above. Detect with `command -v pds`; when missing, OFFER the
GitHub Releases installer one-liner with consent (REFERENCE.md §4a), and
mention the `cargo install pds-cli` fallback. The raw `sphinx-build` /
`ubc` commands remain the no-pds fallback everywhere the gate appears.

### 6. Optional extras (each opt-in, ask once)

- **Devcontainer**: copy the sphinx-needs-starter devcontainer
  (REFERENCE.md §5) — skip if `.devcontainer/` exists; offer to merge instead.
- **CI**: strict-build workflow on push/PR (REFERENCE.md §6).
- **Agent block**: append `## Agent skills` to `CLAUDE.md`/`AGENTS.md`
  (whichever exists; create `CLAUDE.md` if neither) per REFERENCE.md §7.
- **Makefile**: `html` / `strict` / `needs` / `serve` / `clean` targets
  (REFERENCE.md §8) — if a Makefile exists, offer to append the targets.
- **pds config tables**: offer to scaffold `[tool.patdhlk-skills.gate]`
  (optional `needs_json` / `sphinx_command` / `exempt_statuses` keys;
  REFERENCE.md §9) and the `[tool.patdhlk-skills.lint]` table (shipped;
  REFERENCE.md §9a) — rules `required_sections`, `nontrivial_body`,
  `max_body_length`, `weasel_words`, `unenumerated_quantifiers`; `pds check`
  runs lint automatically when the table is present. Also the forward-looking
  `rubrics` / `verdicts` tables plus the `verdict` type and role-map entry
  (REFERENCE.md §10). An absent table simply means that feature is off
  (ADR_0014) — never add a table the user declines.

### 7. Validate and report

Run the strict gate: `pds check` (ADR_0007, ADR_0017) — no pds: `uv run
sphinx-build -W -b html <spec_dir> <spec_dir>/_build/html`. It must pass
(exit 0) before reporting success; exit 1 means fix the corpus and re-run,
exit 2 means a tool/config error to resolve. Then summarize: backend, role
map, builder, whether pds is installed, what was scaffolded vs reused, and
the build/query commands now available.

## Hard rules

- Never overwrite an existing `ubproject.toml` section the user authored —
  edit additively, show diffs for anything touched.
- Never add need types without explicit consent (step 4 is the only place).
- A failed strict gate at step 7 is YOUR bug to fix before finishing.
- Re-running this skill must be safe (idempotent updates, not duplication).
