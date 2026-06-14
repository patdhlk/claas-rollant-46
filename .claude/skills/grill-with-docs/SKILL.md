---
name: grill-with-docs
description: Grilling session that challenges a plan against the project's recorded domain model - existing decisions, requirements, and glossary terms from the sphinx-needs spec - and updates term and decision directives inline as the discussion crystallises. Use when the user wants to stress-test a plan against their project's documented language and decisions, or says "grill me with docs" / "check this plan against our decisions".
---

# Grill With Docs

`/grill-me` with the spec loaded. The interview rules are the same
(research first, one question at a time, always recommend, order by
dependency, respect diverged answers, flag assumptions — see the `grill-me`
skill); what changes is that the project's recorded domain model sits on
the table, and the session **writes back to it as it goes**.

## Setup

Read `[tool.patdhlk-skills]` from `ubproject.toml`; the `term` and
`decision` roles must be mapped (missing → `/setup-patdhlk-skills`). Build
a fresh needs.json (ADR_0006) and load:

- all **terms** — the project's vocabulary,
- all **decisions** with status and links,
- the **features/requirements** the plan touches.

Glossary location: `terms_doc` from the config when set; otherwise find
where term directives live, ask once, persist as `terms_doc`. Decisions go
to `decisions_doc` (same mechanism, shared with `/decide`).

## The three doc-grounded behaviors

Layered on top of the normal grilling:

1. **Challenge with the record.** When the plan touches ground covered by
   an accepted decision, quote it by ID: "ADR_0006 says reads go through
   needs.json — your plan greps RST. Change the plan, or supersede the
   decision?" A plan is never allowed to silently contradict the record:
   the contradiction is resolved in-session, one way or the other.

2. **Sharpen terminology.** When the user's words drift from a recorded
   term, hold up the glossary definition and ask which is right. When the
   session coins a term that earns a definition (used three times, or
   load-bearing in a decision), write it inline as a term directive — at
   the moment it crystallises, not in an end-of-session sweep.

3. **Record decisions inline.** When a branch of the interview closes with
   a real decision, capture it immediately as a decision directive,
   following the `/decide` conventions exactly: rejected alternative
   required, a ❌ consequence required, supersede-never-rewrite. Confirm
   title and links with the user, write, move to the next branch.

Every inline write ends with the strict gate before the interview continues
(ADR_0007, ADR_0017):

```bash
pds check
# no pds: uv run sphinx-build -W -b html <spec_dir> <spec_dir>/_build/html
```

Exit 0 means proceed; exit 1 means fix the corpus and re-run; exit 2 means
stop and escalate.
A grilling session that leaves the spec broken has negative value.

## Ending the session

As `/grill-me`: stop when the tree is walked, deliver the shared
understanding with assumptions flagged. Additionally list the session's
spec mutations — terms written, decisions recorded, decisions superseded —
by ID. What crystallised is already durable; only what stayed soft needs a
follow-up (`/to-prd` if the plan became a feature).

## Hard rules

- Never let the plan and an accepted decision contradict each other past
  the question that exposed it.
- Write terms and decisions when they crystallise, not in a batch at the
  end — inline capture is the point of this skill.
- Inline decisions obey ALL of `/decide`'s hard rules.
- A failed strict gate pauses the interview until fixed.
