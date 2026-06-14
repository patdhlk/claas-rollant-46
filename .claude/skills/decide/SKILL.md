---
name: decide
description: Record a design decision as a traceable sphinx-needs architecture decision - Context/Decision/Consequences structure, alternatives considered, and links to the needs it affects. Use when a design decision was made and should be captured, when the user says "record this decision" / "write an ADR", or at the end of a design discussion worth keeping.
---

# Decide

A decision that lives only in conversation is a decision the next session
will re-make. Capture it as a decision need in the spec — linked, validated,
findable (ADR_0002). Standalone: no external tooling beyond the spec build.

## Workflow

### 1. Resolve configuration

Read `[tool.patdhlk-skills]` from `ubproject.toml`; the `decision` role must
be mapped (missing → `/setup-patdhlk-skills`). Target document:
`decisions_doc` from the config when set; otherwise find where existing
decision directives live (often `<spec_dir>/architecture/`), ask the user
once, persist as `decisions_doc`. When the user names a topic page
("put it with the codegen decisions"), honor that for this call.

### 2. Extract the decision — interview for what's missing

From the conversation, assemble:

- **Title** — the decision as a verdict, not a topic ("Parser separated
  from codegen", not "Parser architecture").
- **Context** — the forces: what problem, which constraints.
- **Decision** — what was chosen, concretely.
- **Alternatives** — what was NOT chosen and why. *A decision without
  rejected alternatives is a description, not a decision* — if the
  conversation has none, ask for at least one.
- **Consequences** — both directions: what gets better (✅), what is paid
  (❌). A consequences list with no ❌ is unfinished.

Status: `accepted` for a settled decision (the normal case), `proposed`
when the user marks it tentative.

### 3. Link it into the graph

Build a fresh needs.json (ADR_0006). Link the directive to the needs the
decision affects — the feature it `:refines:`, requirements it constrains
(`:links:`). If it replaces an earlier decision: set the old one's
`:status:` to `superseded` and link the new one to it. Ask the user to
confirm the link set; do not link speculatively.

### 4. Write, validate, report

Allocate a dense max+1 ID (ADR_0008) and append:

```rst
.. arch-decision:: Parser separated from codegen
   :id: ADR_0014
   :status: accepted
   :refines: FEAT_0050

   **Context.** A monolithic crate would conflate parsing, IR, and
   backend concerns; backends churn faster than the parser.

   **Decision.** Three crates: parser (no_std), IR + backend trait,
   concrete backend. Considered and rejected: single crate with feature
   flags (feature interactions untestable).

   **Consequences.** ✅ Each layer has one job; backends are swappable.
   ❌ Three versions to release in lockstep.
```

Then the strict gate (ADR_0007, ADR_0017) — the decision is not recorded
until it passes:

```bash
pds check
# no pds: uv run sphinx-build -W -b html <spec_dir> <spec_dir>/_build/html
```

Exit 0 records the decision; exit 1 means fix the corpus and re-run; exit 2
is a tool/config error — stop and escalate. Report the new ID and its links.

## Hard rules

- No decision without at least one rejected alternative in the text.
- No consequences section without a ❌.
- Supersede, never silently rewrite: history of changed minds stays
  readable.
- The user confirms title, links, and status before the write.
- A failed strict gate is YOUR bug to fix before reporting success.
