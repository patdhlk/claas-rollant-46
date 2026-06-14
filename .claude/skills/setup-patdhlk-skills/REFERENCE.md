# Templates for /setup-patdhlk-skills

Adapt names/paths to the repo. `<spec>` = the chosen spec dir (default `spec`).

## §1 Spec skeleton (when no sphinx-needs project exists)

```
ubproject.toml          # repo root — single source of truth
pyproject.toml          # uv-managed, package = false
<spec>/conf.py
<spec>/schemas.json
<spec>/index.rst
<spec>/requirements/    # req directives (role: requirement)
<spec>/architecture/    # decision directives (role: decision)
<spec>/glossary.rst     # term directives (role: term)
<spec>/issues/          # only with the sphinx-needs issue backend (§3)
```

Toolchain: `uv init` (then set `[tool.uv] package = false`), `uv add sphinx
"sphinx-needs>=8,<9" furo sphinx-autobuild`, commit `uv.lock`.

`ubproject.toml` (minimal — extend, never replace, an existing one):

```toml
"$schema" = "https://ubcode.useblocks.com/ubproject.schema.json"

[project]
name = "<repo-name>"
srcdir = "<spec>"

[needs]
id_required = true

[[needs.types]]
directive = "req"
title = "Requirement"
prefix = "REQ_"
color = "#BFD8D2"
style = "node"

[[needs.types]]
directive = "arch-decision"
title = "Architecture Decision"
prefix = "ADR_"
color = "#A6C8E0"
style = "node"

[[needs.types]]
directive = "term"
title = "Glossary Term"
prefix = "GLOSS_"
color = "#D8D8D8"
style = "node"

[needs.links.satisfies]
incoming = "is satisfied by"
outgoing = "satisfies"

[needs.links.refines]
incoming = "is refined by"
outgoing = "refines"

[needs.links.implements]
incoming = "is implemented by"
outgoing = "implements"

[needs.links.verifies]
incoming = "is verified by"
outgoing = "verifies"
```

`<spec>/conf.py`:

```python
import json
from pathlib import Path

project = "<repo-name> — Specification"
extensions = ["sphinx_needs"]
exclude_patterns = ["_build", ".venv"]
source_suffix = {".rst": "restructuredtext"}

needs_from_toml = "../ubproject.toml"

needs_schema_validation_enabled = True
with (Path(__file__).parent / "schemas.json").open("r", encoding="utf-8") as _fh:
    needs_schema_definitions = json.load(_fh)

html_theme = "furo"
```

`<spec>/schemas.json` starts as `{"schemas": []}`; add ID-pattern rules per
type the repo uses (see patdhlk/skills `spec/schemas.json` for the shape).

## §2 The config block

Append to `ubproject.toml` (update keys in place on re-run):

```toml
[tool.patdhlk-skills]
issue_backend = "sphinx-needs"   # or "github"
spec_dir = "<spec>"
builder = "ubc"                  # or "sphinx-build"
issue_doc = "<spec>/issues/index.rst"   # sphinx-needs backend only
# features_dir = "<spec>/features"      # written by /to-prd on first use
# decisions_doc = "<spec>/architecture/index.rst"  # written by /decide on first use
# terms_doc = "<spec>/glossary.rst"        # written by /grill-with-docs on first use

[tool.patdhlk-skills.roles]
# only roles that resolved to a directive — never invent entries
issue = "issue"
feature = "feat"
requirement = "req"
decision = "arch-decision"
term = "term"
test = "test"
```

## §3 The issue type (sphinx-needs backend, on consent)

Type + fields for `ubproject.toml`:

```toml
[needs.fields.kind]
description = "Issue kind: bug | feature | improvement | chore | question"
nullable = true
schema = { type = "string", enum = ["bug", "feature", "improvement", "chore", "question"] }

[needs.fields.github]
description = "GitHub issue number this need is tracked by (github backend only)"
nullable = true
schema = { type = "string" }

[[needs.types]]
directive = "issue"
title = "Issue"
prefix = "ISSUE_"
color = "#E8C8A0"
style = "node"
```

Status state machine (ADR_0005) for `schemas.json` — append to `"schemas"`:

```json
{
  "id": "issue-status",
  "message": "issue :status: must be a triage state: needs-triage | needs-info | ready-for-agent | ready-for-human | in-progress | done | wontfix",
  "select": { "properties": { "type": { "const": "issue" } } },
  "validate": {
    "local": {
      "properties": {
        "status": {
          "enum": ["needs-triage", "needs-info", "ready-for-agent",
                   "ready-for-human", "in-progress", "done", "wontfix"]
        }
      },
      "required": ["status"]
    }
  }
}
```

`<spec>/issues/index.rst`:

```rst
Issues
======

.. needtable::
   :types: issue
   :columns: id;title;status;kind
   :style: table
```

Add `issues/index` to the root toctree.

## §4 ubc install (Linux)

```bash
curl -fsSL "https://download.useblocks.com/ubc/0.29.3/ubc-linux-$(uname -m | sed 's/x86_64/x64/;s/aarch64/arm64/')-0.29.3" \
  -o /usr/local/bin/ubc && chmod +x /usr/local/bin/ubc
```

(Or copy `install-ubc.sh` from github.com/patdhlk/sphinx-needs-starter —
the devcontainer in §5 runs it automatically.) macOS/Windows hosts: persist
`builder = "sphinx-build"`.

ubc usage note: `ubc build needs --outpath <file>` — the flag is `--outpath`
and the target directory must already exist.

## §4a pds install (the gate-and-query CLI)

`pds` (ADR_0017) is the strict gate (`pds check`) and the needs builder
(`pds build`); it drives whichever `builder` is configured. Detect with
`command -v pds`. When missing, offer the GitHub Releases shell installer
(with consent):

```bash
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/patdhlk/skills/releases/latest/download/pds-cli-installer.sh | sh
```

Fallback (any platform with a Rust toolchain): `cargo install pds-cli`.

macOS note: the shell installer sidesteps Gatekeeper (curl does not set the
quarantine flag); if a copied binary is ever quarantined, see the Gatekeeper
section of `cli/README.md` for the `xattr -d` / ad-hoc-sign fix.

In this repo specifically, the in-tree crate is the no-install fallback:
`cd cli && cargo run -q -p pds-cli -- <verb>` — run from cli/ so rustup picks
up the pinned toolchain; pds finds the project root by walk-up (this is
exactly what the Makefile's `PDS` variable falls back to).

## §5 Devcontainer

Copy from github.com/patdhlk/sphinx-needs-starter (`.devcontainer/`):
`devcontainer.json` (image `ghcr.io/patdhlk/sphinx-needs-starter:latest`,
`onCreateCommand` runs `install-ubc.sh`, `postCreateCommand` runs `uv sync`),
`Dockerfile`, `install-ubc.sh`. Adjust `esbonio.sphinx.confDir` to `<spec>`.

## §6 CI workflow

The gate job runs `pds check` (ADR_0017). Install `pds` from the Releases
shell installer (§4a); the raw `sphinx-build` lines are the no-pds fallback.

`.github/workflows/spec.yml`:

```yaml
name: spec
on:
  push:
    branches: [main]
  pull_request:
jobs:
  gate:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: astral-sh/setup-uv@v5
      - run: uv sync
      - name: Install pds
        run: |
          curl --proto '=https' --tlsv1.2 -LsSf \
            https://github.com/patdhlk/skills/releases/latest/download/pds-cli-installer.sh | sh
      - name: Strict gate (pds check)
        run: pds check
        # no pds: uv run sphinx-build -W -b html <spec> <spec>/_build/html
        #         && uv run sphinx-build -b needs <spec> <spec>/_build/needs
```

## §7 CLAUDE.md agent block

```markdown
## Agent skills (patdhlk-skills)

- Issue backend: **<backend>**. Config + role map: `ubproject.toml` →
  `[tool.patdhlk-skills]`. Resolve directives ONLY through the role map.
- Query the needs corpus via needs.json, never by grepping RST: `pds build`
  (no pds: `ubc build needs --outpath <spec>/_build/needs/needs.json`, or
  `uv run sphinx-build -b needs <spec> <spec>/_build/needs`), then `jq` on
  `<spec>/_build/needs/needs.json`. Rebuild before every query.
- New need IDs: dense max+1 per prefix, from a fresh needs.json.
- Every spec mutation must end with the strict gate: `pds check` (no pds:
  `uv run sphinx-build -W -b html <spec> <spec>/_build/html`). Exit 0 =
  clean, 1 = violations (read the JSON findings on stdout, fix the corpus),
  2 = tool/config error (stop and escalate). Branch on the exit code.
- Issues (sphinx-needs backend) live in `<issue_doc>`; `:status:` carries
  the triage state machine: needs-triage → needs-info | ready-for-agent |
  ready-for-human → in-progress → done | wontfix. Edit status in place —
  git history is the audit trail.
- Issues (github backend): `gh` CLI; triage states are labels; issue bodies
  carry `Implements: <NEED-IDs>`; needs carry `:github:` back-references.
```

## §8 Makefile

The gate (`strict`/`needs`) runs through `pds`: use it from PATH when
present, else fall back to the in-tree crate via `cargo run` so the gate
works for any contributor with Rust. `pds` owns the per-builder adapter
(ubc preferred, sphinx fallback), so there is no hand-rolled branching.

```make
SOURCEDIR = <spec>
BUILDDIR  = <spec>/_build

# `pds` from PATH if present, else a quiet `cargo run` against the in-tree
# crate — from cli/ so rustup honours cli/rust-toolchain.toml (cwd-based).
PDS = $(shell command -v pds 2>/dev/null || echo "cd cli && cargo run -q -p pds-cli --")

.PHONY: html strict needs serve clean

html:  ## Build the HTML spec (NOT the gate — ADR_0017)
	uv run sphinx-build -b html "$(SOURCEDIR)" "$(BUILDDIR)/html"

strict:  ## Strict gate — every spec mutation must pass this (ADR_0007, ADR_0017)
	$(PDS) check

needs:  ## Build a fresh needs.json for querying (ADR_0006)
	$(PDS) build

serve:  ## Live preview with auto-rebuild (port 8000)
	uv run sphinx-autobuild "$(SOURCEDIR)" "$(BUILDDIR)/html"

clean:  ## Remove build artifacts
	rm -rf "$(BUILDDIR)"
```

(If `pds` is published and always installed in your environment, the `PDS`
fallback to the in-tree crate can be dropped — keep it for repos that build
`pds` from source, like patdhlk/skills itself.)

When appending to an existing Makefile, rename colliding targets with a
`spec-` prefix (`spec-html`, `spec-strict`, ...).

## §9 The gate config table (`[tool.patdhlk-skills.gate]`)

Optional. `pds check` / `pds build` work with no table — defaults derive
from `spec_dir` and `builder`. Offer to scaffold it only when the repo
needs to override a path or builder invocation:

```toml
[tool.patdhlk-skills.gate]
# needs_json   = "<spec>/_build/needs/needs.json"   # default: <spec>/_build/needs/needs.json
# sphinx_command = ["uv", "run", "sphinx-build"]    # default: how pds invokes the sphinx builder (array, one element per argv)
# exempt_statuses = ["done", "wontfix"]             # default: statuses fully skipped by lint (and future checks)
```

All keys are optional; an absent `[tool.patdhlk-skills.gate]` table means
`pds check` runs with its built-in defaults (ADR_0014). `exempt_statuses`
defaults to `["done", "wontfix"]`; a need with NO status is never exempt.

## §9a The lint config table (`[tool.patdhlk-skills.lint]`)

**Shipped in `pds` (ISSUE_0013).** `pds lint` runs the rules; `pds check`
also runs lint automatically when `[tool.patdhlk-skills.lint]` is present.
An absent table means lint is off — clean exit 0 without building.

All rule keys are optional (rule is off when the key is absent). Every
directive name used as a key must be declared in `[[needs.types]]`; an
undeclared name is a config error (exit 2). Findings carry
`{"check": "lint:<rule>", "severity": "error", "need": "<ID>", "message": "…"}`.

### Rule key shapes

```toml
# required_sections: per-directive map -> list of bold lead-ins that must
# appear in each non-exempt body. Accepts `**Section.**` or `**Section**`.
[tool.patdhlk-skills.lint.required_sections]
arch-decision = ["Context", "Decision", "Consequences"]

# nontrivial_body: per-directive map -> minimum body length in characters.
# (Paired with max_body_length if you also want a ceiling.)
[tool.patdhlk-skills.lint.nontrivial_body]
issue = 200

# max_body_length: per-directive map -> maximum body length in characters.
[tool.patdhlk-skills.lint.max_body_length]
issue = 5000

# weasel_words: flag vague terms in targeted directives.
# A word is exempt when its sentence contains a digit or a comparison cue
# (`<`, `>`, `=`, "at least", "at most", "within", "no more than", "exactly").
[tool.patdhlk-skills.lint.weasel_words]
words      = ["significant", "appropriate", "robust"]
directives = ["req"]

# unenumerated_quantifiers: flag quantifiers not followed by an enumeration cue
# ("of the", "of these", "listed", "in the", "declared", "configured",
# "below", "above") within the same sentence.
[tool.patdhlk-skills.lint.unenumerated_quantifiers]
quantifiers = ["all", "every", "each"]
directives  = ["req"]
```

### Worked example (this repo's actual config)

```toml
# gate.exempt_statuses is the default ["done","wontfix"]; needs with those
# statuses are fully skipped by lint.
[tool.patdhlk-skills.lint.required_sections]
arch-decision = ["Context", "Decision", "Consequences"]

[tool.patdhlk-skills.lint.nontrivial_body]
# Shortest active issue body: 539 chars — threshold 200 gives ~2.7× headroom.
issue = 200
```

`pds lint` exits 0 with `{"findings":[]}` when all non-exempt needs pass.
Raise `nontrivial_body.issue` above 539 and lint flags `ISSUE_0018` with
`"check":"lint:body-length", "need":"ISSUE_0018"` — the dogfood bites.

weasel_words and unenumerated_quantifiers are not enabled here because this
repo has no active `req` directives in the corpus; enabling them would
require targeting `issue` or `arch-decision` bodies, which are narrative
rather than formal requirements.

## §10 Forward-looking pds config (rubrics / verdicts — not yet shipped)

These tables let later `pds` verbs (verdict-check, not yet shipped) read
project policy from config. Today's `pds` IGNORES them — offer to scaffold
only with the user's understanding that they are forward-looking. Absent
tables = those future checks are off (ADR_0014). Mark them clearly.

```toml
# Forward-looking — declared for future `pds` verbs; today's pds ignores these.

# ADR_0016: rubric axes are DECLARED in config; their semantics live in the
# review skills, not here.
[tool.patdhlk-skills.rubrics.<name>]
# axes = ["clarity", "testability", ...]

[tool.patdhlk-skills.verdicts]
# verdict policy for the future `pds verdict-check` verb (ADR_0016):
# require = { req = "quality" }   # map need type -> required rubric name
```

Verdicts (ADR_0015) are derived sphinx-needs records — IDs
`VERDICT_<judged-id>`, fields `:rubric:` / `:axes_failed:` /
`:fingerprint:`, living under `<spec>/verdicts/`. To prepare the corpus,
offer the type + role-map entry:

```toml
[[needs.types]]
directive = "verdict"
title = "Verdict"
prefix = "VERDICT_"
color = "#E0A6C8"
style = "node"

[tool.patdhlk-skills.roles]
verdict = "verdict"
```
