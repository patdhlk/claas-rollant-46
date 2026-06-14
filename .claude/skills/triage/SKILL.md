---
name: triage
description: Drive issues through the triage state machine - route incoming issues to needs-info, ready-for-agent, ready-for-human, or wontfix, and keep states legal on both backends (sphinx-needs status edits or GitHub labels). Use when the user wants to triage issues, review the backlog, prepare issues for an agent, or says "triage" / "go through the open issues".
---

# Triage

One state machine, schema-enforced on the local backend (ADR_0005):

```
needs-triage ──► needs-info | ready-for-agent | ready-for-human | wontfix
needs-info ──► (answered) ──► needs-triage
ready-for-* ──► in-progress ──► done | wontfix
```

`/triage` owns the routing decisions; `in-progress`/`done` belong to whoever
does the work. Never jump states (e.g. `needs-triage → done`) — the enum
won't catch it, you must.

## Workflow

### 1. Resolve configuration and load the queue

Read `[tool.patdhlk-skills]` from `ubproject.toml` (missing → point to
`/setup-patdhlk-skills`).

- **sphinx-needs**: `pds status` first — it rebuilds needs.json (ADR_0006)
  and emits the per-status counts, the "what needs attention" buckets, in
  one query. Then the queue = issues with `status == "needs-triage"` (jq
  over the fresh needs.json), plus `needs-info` ones whose question got
  answered since.
- **github**: `gh issue list --label needs-triage` **plus** open issues with
  no state label at all (treat unlabeled as needs-triage; offer to label
  them). Ensure the five state labels exist (`gh label create`, idempotent).

Empty queue → say so and stop.

### 2. Route each issue

Before routing, gather search evidence for each issue (sphinx-needs backend
only). If `pds` is not on PATH, print a loud warning pointing to
`/setup-patdhlk-skills` and skip this enrichment step.

```bash
pds search "<issue title> <issue body>" --config ubproject.toml
# exit 0 → hits JSON; exit 2 → stop and escalate
```

Use the full title + body as the query (ADR_0021 saturation). The top hit
is the issue under triage itself — **skip it**; judge the remaining
hits. A high-scoring hit on a `done` issue or an
`arch-decision` means the report is already shipped; a high-scoring hit on
another open issue means it may be a duplicate. Feed both signals into
rule 1 below. The github backend keeps its existing `gh issue list`
judgment; `pds search` is not available there.

Apply in order — first match wins:

1. **Duplicate / invalid / out of scope** → `wontfix`, with a one-line
   reason (and the duplicate's ID if any). Use the search hits above as
   evidence — cite the twin's ID when recommending duplicate.
2. **Cannot be acted on as written** — missing repro, ambiguous scope, no
   way to verify → `needs-info`, writing the concrete question.
3. **An agent can finish it unattended** — clear acceptance criteria, no
   unresolved design decisions, no credentials or human taste required →
   `ready-for-agent`.
4. **Otherwise** → `ready-for-human` (say what the human must decide).

Propose the routing for the whole queue in one table (ID, title, verdict,
reason) and get the user's go-ahead — they may override any verdict —
**before** applying. An override into an in-scope status still authors a
fail-closed verdict (§3a).

### 3. Apply transitions

- **sphinx-needs**: edit each `:status:` in place in the issue's RST —
  git history is the audit trail, no changelog field. For `needs-info`,
  append the question to the directive body under a ``**Needs info.**``
  lead-in. For `wontfix`, append the one-line reason the same way.
- **github**: swap the state label (remove old, add new). For `needs-info`
  and `wontfix`, post the question/reason as a comment; close `wontfix`
  issues as not-planned (`gh issue close --reason "not planned"`).

#### 3a. Author or update the triage verdict (sphinx-needs backend only)

Fire **only** when routing into `ready-for-agent`, `ready-for-human`, or
`in-progress`. Routing to `needs-info`, `wontfix`, or `needs-triage`
authors nothing.

**Ordering:**

1. Apply the `:status:` edit and any title/body tightening this pass
   makes (step 3 above).
2. Run `pds fingerprint <id>` **last** — after every title/body edit:
   ```bash
   pds fingerprint ISSUE_0042 --config ubproject.toml
   # prints {"schema":1,"verb":"fingerprint","id":"ISSUE_0042","fingerprint":"sha256:..."}
   ```
   Only title+body enter the hash (ADR_0015), so the `:status:` edit's
   position does not matter — but a fingerprint taken before a body edit
   is born stale, which the gate then flags. Compute it after the issue
   is in its final shape.
3. Write or edit `VERDICT_<id>` in `spec/verdicts/index.rst`. The derived
   ID is one slot — edit in place if it already exists (re-triage after a
   `needs-info` round-trip); git history is the audit trail (ADR_0005).
   The directive must carry: `:rubric: triage`, the computed `:fingerprint:`,
   `:axes_failed:` per the fail-closed rule, and per-axis reasoning as body
   prose.
4. Run `make strict` (step 4).

**The four triage axes** (each is an affirmative claim; ADR_0016):

- **category** — exactly one correct `:kind:` for the content (e.g.
  `bug`, `improvement`, `feature`).
- **state** — the routing target is justified. For `ready-for-agent` this
  means: clear acceptance criteria, no unresolved design decision, no
  credentials or human-taste required (route rule 3 — an agent finishes
  unattended).
- **actionability** — the body is self-contained to the AGENT-BRIEF.md
  bar: a fresh agent can execute without conversation history.
- **duplicate-check** — `pds search`/`dedup` was run and the top non-self
  hits were judged not duplicate and not already-shipped.

**Fail-closed rule:** any axis the triager cannot affirmatively confirm
goes into `:axes_failed:` with a finding sentence in the body explaining
why — never omitted, never optimistically passed. A passing (empty
`:axes_failed:`) verdict asserts all four were positively cleared.

**Worked RST example** (mirror the `spec/verdicts/index.rst` tracer shape):

```rst
.. verdict:: Triage verdict for ISSUE_0042
   :id: VERDICT_ISSUE_0042
   :links: ISSUE_0042
   :rubric: triage
   :fingerprint: sha256:abcd1234ef567890

   *This was generated by AI during triage.*

   Judged on <date>. **category**: improvement, clear-cut — a bounded
   prose change with no ambiguous type. **state**: ready-for-agent —
   acceptance criteria are independently verifiable, no open design
   decisions, no credentials required. **actionability**: body is
   self-contained; a fresh agent executes without conversation history.
   **duplicate-check**: ``pds search`` top non-self hits judged not
   duplicate and not already-shipped.
```

**Quick-override path** ("move #42 to ready-for-agent", skip-grill): still
authors a verdict, but **fail-closed** — all unevaluated axes go in
`:axes_failed:` with the finding "routed by maintainer override, not
triage-judged", so the gate flags it for a real triage pass rather than
rubber-stamping. A failing verdict lists the failed axes in
`:axes_failed:` (comma-separated):

```rst
.. verdict:: Triage verdict for ISSUE_0042
   :id: VERDICT_ISSUE_0042
   :links: ISSUE_0042
   :rubric: triage
   :fingerprint: sha256:abcd1234ef567890
   :axes_failed: category, state, actionability, duplicate-check

   *This was generated by AI during triage.*

   Routed to ready-for-agent by maintainer override, not triage-judged:
   no axis was affirmatively checked. Needs a real triage pass.
```

### 4. Validate and report

Local mutations end with the strict gate (ADR_0007, ADR_0017):

```bash
pds check
# no pds: uv run sphinx-build -W -b html <spec_dir> <spec_dir>/_build/html
```

Exit 0 means proceed; exit 1 means fix the corpus and re-run; exit 2 means
stop and escalate.
Report counts per verdict (`pds status` after the gate shows the new
buckets) and the `ready-for-agent` list — that's the pickup queue for the
next work session; `pds next` returns its lowest-ID member as JSON.

For each issue in `ready-for-agent` or `ready-for-human`, name its verdict
in the report: ID + pass (empty `:axes_failed:`) or fail (list the failing
axes). Example: "ISSUE_0042 → VERDICT_ISSUE_0042 (pass)". A green
`make strict` guarantees a passing, fresh verdict on every in-scope issue —
that is the pickup contract.

## Hard rules

- Only the transitions drawn above; `/triage` never sets `in-progress` or
  `done`.
- Every `needs-info` carries a concrete, answerable question; every
  `wontfix` carries a reason. No silent state changes.
- The user confirms the routing table before anything is applied.
- A failed strict gate is YOUR bug to fix before reporting success.
- Never author a passing verdict for an axis you did not affirmatively
  check — uncertain is a failing axis with a finding, never a silent pass.
