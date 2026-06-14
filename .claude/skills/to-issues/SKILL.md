---
name: to-issues
description: Slice a PRD's requirements into independently-grabbable issues on the configured backend - sphinx-needs issue directives or GitHub issues with two-way traceability links. Use when the user wants to break a PRD/feature/plan into issues, create implementation tickets, or says "turn this into issues" / "create the tickets".
---

# To Issues

Turn a feature's requirements into work items someone (human or agent) can
grab and finish independently. Where the issues land depends on
`issue_backend` (ADR_0003); the requirements they trace to are always
sphinx-needs.

## Workflow

### 1. Resolve configuration and scope

Read `[tool.patdhlk-skills]` from `ubproject.toml`: `issue_backend`,
`spec_dir`, `builder`, `issue_doc`, role map. Missing config or unmapped
`requirement` role (plus `issue` role on the sphinx-needs backend) → stop,
point to `/setup-patdhlk-skills`.

Identify the target feature from conversation (or ask). Build a fresh
needs.json (ADR_0006) and collect its requirements: needs of the
requirement type whose `satisfies` (or `links`) includes the feature ID.
Skip reqs already covered — an existing issue `implements` them (local) or
they carry a `:github:` reference (github backend).

### 2. Slice into vertical slices — not 1:1 req→issue

Group requirements into issues that each deliver **observable behavior end
to end** (a tracer bullet first: the thinnest path through the whole stack).
Split a req that hides two work items; bundle reqs that only make sense
shipped together. Every issue must state:

- what to build (1–3 sentences, self-contained — readable without the PRD),
- which requirement IDs it implements,
- how to verify it (the demo/test that proves it done).

Mark issues an agent can finish unattended `ready-for-agent`; ones needing
human judgment, credentials, or design taste `ready-for-human`.

**Dedup pass (sphinx-needs backend only).** Before presenting the plan, run
`pds dedup "<slice title + body draft>"` (full text, not bare title — short
queries over-gate per ADR_0021) for each candidate slice. Branch on exit code
(ADR_0014):

- Exit 0 — slice is clean; include it as-is.
- Exit 1 — collision detected; annotate the slice row in the plan table:
  - Open-issue hit: "slice N collides with ISSUE_XXXX — propose extending its
    `:implements:` instead."
  - Done-issue or ADR hit: "slice N matches ISSUE_XXXX (done) / ADR_XXXX
    (accepted) — requirement may already be shipped/decided; consider dropping
    this slice."
- Exit 2 — stop and escalate.

If `pds` is missing from PATH, emit one loud line pointing to
`/setup-patdhlk-skills` and skip the dedup enrichment (the skill functioned
without dedup until this version).

On the **github backend** `pds dedup` exits 2 (no v1 driver); note this in
the plan and rely on the existing manual judgment — unchanged.

Present the slicing plan (issue titles + req IDs each covers, plus any
collision annotations) and get the user's go-ahead **before** filing anything.

### 3a. File — sphinx-needs backend

Allocate dense max+1 IDs from the fresh needs.json (ADR_0008) and append
`issue` directives to `issue_doc`:

```rst
.. issue:: Theme toggle with persistence
   :id: ISSUE_0007
   :status: ready-for-agent
   :kind: feature
   :implements: REQ_0017, REQ_0018

   Add the toggle to settings, persist the choice, restore on launch.
   Verify: relaunch keeps the chosen theme.
```

Use `:implements:` when the repo declares it, else `:links:`.

### 3b. File — github backend

Create via `gh issue create`, one per slice. Body template:

```markdown
Add the toggle to settings, persist the choice, restore on launch.

**Verify:** relaunch keeps the chosen theme.

Implements: REQ_0017, REQ_0018
```

The `Implements:` line is the greppable forward link (ADR_0009). Label with
the triage state (`ready-for-agent` / `ready-for-human`); create missing
labels with `gh label create`. Then write the back-links: add
`:github: <number>` to each implemented req directive (comma-separate
multiple issue numbers).

### 4. Validate and report

Any spec mutation (3a always; 3b's back-links) ends with the strict gate
(ADR_0007, ADR_0017):

```bash
pds check
# no pds: uv run sphinx-build -W -b html <spec_dir> <spec_dir>/_build/html
```

Exit 0 means proceed; exit 1 means fix the corpus and re-run; exit 2 means
stop and escalate.
Report the created issue IDs/URLs, which reqs each covers, and any reqs
left unsliced (with why). Suggest `/triage` if anything was filed
`needs-triage`.

## Hard rules

- Confirm the slicing plan with the user before filing (step 2).
- Both link directions are YOUR job on the github backend: `Implements:`
  in the body AND `:github:` on the reqs — never just one.
- An issue nobody can pick up without reading the PRD is sliced wrong.
- A failed strict gate is YOUR bug to fix before reporting success.
