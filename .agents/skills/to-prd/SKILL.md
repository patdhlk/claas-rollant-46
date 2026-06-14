---
name: to-prd
description: Turn the current conversation context into a PRD as a living sphinx-needs spec document - one feature directive plus child requirement directives, traceably linked. Use when the user wants to create a PRD, capture a feature plan as requirements, or says "turn this into a PRD" / "write this up as a spec".
---

# To PRD

A PRD is not a frozen memo — it is one RST document in the spec: a single
*feature* need carrying the prose (motivation, scope, non-goals) plus child
*requirement* needs linked back to it (ADR_0010). This is true regardless of
the issue backend; `/to-issues` handles slicing later.

## Workflow

### 1. Resolve configuration

Read `[tool.patdhlk-skills]` from `ubproject.toml` (repo root, then `spec*/`,
`docs/`): `spec_dir`, `builder`, and the role map. The roles `feature` and
`requirement` must be mapped — if the table or either role is missing, stop
and tell the user to run `/setup-patdhlk-skills` first. Below, `feat`/`req`
mean *whatever directives the role map names*.

### 2. Synthesize, then confirm

Draft the PRD from the conversation: title, motivation, scope, non-goals,
and 3–10 requirements. Each requirement is **one testable shall-statement**
— split anything with an "and" joining two behaviors; park open questions in
the prose, not in requirements. Present the outline (title + requirement
one-liners) and get the user's go-ahead **before** touching files.

### 3. Allocate IDs

Build a fresh needs.json — never grep RST, never cache (ADR_0006):

```bash
pds build   # fresh needs.json at <spec_dir>/_build/needs/needs.json
# no pds: ubc build needs --outpath <spec_dir>/_build/needs/needs.json
#         (or uv run sphinx-build -b needs <spec_dir> <spec_dir>/_build/needs)
```

Take the highest numeric suffix per prefix and allocate dense max+1
(ADR_0008), reading each type's prefix from `[needs.types]`.

### 4. Write the document

Target directory: `features_dir` from `[tool.patdhlk-skills]` when set.
Otherwise, if the spec already has a PRD-like directory (`features/`,
`prd/`, `epics/`, ...), ask the user which to use — recommending the
existing one — and persist the answer as `features_dir` so it is asked only
once. Default for fresh specs: `<spec_dir>/features/`.

Create `<features_dir>/<kebab-slug>.rst` (make the directory and an
`index.rst` with a toctree on first use; register it in the root toctree).
Shape:

```rst
Dark mode
=========

.. feat:: Dark mode
   :id: FEAT_0005
   :status: draft

   **Motivation.** Users working at night...

   **Scope.** Theme toggle, persistence, system-preference default.

   **Non-goals.** Per-component theme overrides.

.. req:: Theme toggle persists across sessions
   :id: REQ_0017
   :status: draft
   :satisfies: FEAT_0005

   The app shall persist the selected theme and restore it on next launch.
```

(One `req` shown; a real PRD has 3–10, all `:satisfies:` the same feature.)

Both feature and requirements start `:status: draft`. Link reqs with
`:satisfies:` when the repo declares that link type, else `:links:`.

### 5. Validate and hand off

Run the strict gate — the mutation is not done until it passes (ADR_0007,
ADR_0017):

```bash
pds check
# no pds: uv run sphinx-build -W -b html <spec_dir> <spec_dir>/_build/html
```

Exit 1 means fix the corpus and re-run; exit 2 means stop and escalate.
Report the new need IDs and offer the natural next step: `/to-issues` to
slice the requirements into grabbable issues.

## Hard rules

- Confirm the outline with the user before writing files (step 2).
- IDs come only from a fresh needs.json build; no guessing, no gaps.
- Respect the role map — never hardcode directive names.
- A failed strict gate is YOUR bug to fix before reporting success.
