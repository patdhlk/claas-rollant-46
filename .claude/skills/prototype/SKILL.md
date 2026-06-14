---
name: prototype
description: Build a throwaway prototype to answer a specific design question before committing to an approach - a runnable terminal app for state/data-model/algorithm questions, or several radically different UI variations toggleable from one route. Use when the user wants to prototype, sanity-check a data model or state machine, mock up a UI, explore design options, or says "prototype this" / "let me play with it".
---

# Prototype

A prototype is an experiment, not a draft of the product. The prototype
dies; the **answer** survives — as a decision, a feature spec, or a
discarded option with a reason.

## 1. Name the question

Before writing anything, state the design question this prototype answers
("does optimistic locking survive concurrent edits of the same block?",
"which of these three layouts makes the hierarchy scannable?"). No
question, no prototype — that's just coding without a spec. Confirm the
question with the user; it decides the branch.

## 2. Pick the branch

**Logic branch** — for state machines, data models, algorithms, API
shapes: the smallest *runnable* terminal program that exercises the real
question with hardcoded data. Print state transitions; make it
interactive (read stdin) when the user should drive. No UI, no
persistence, no error handling beyond what the question needs.

**UI branch** — for look-and-feel and interaction questions: build **3–4
radically different** variations, not one safe design with color tweaks —
different layouts, different interaction models, different information
hierarchies. All toggleable from one route/entry point so comparison is
one keystroke. Real-looking data (from the domain, using the glossary's
terms), faked backend.

## 3. Throwaway discipline

- Lives outside the production tree: `.prototype/<question-slug>/` (or
  `/tmp` for one-shots). Production code never imports from it.
- Exempt from `/tdd` — no tests, no lint perfection; speed of learning is
  the only quality metric. This exemption NEVER travels back to
  production code.
- Hand the user run instructions immediately ("`uv run
  .prototype/locking/main.py`, then edit block 2 in both windows").
  Iterate on their reactions; mutate variations live.

## 4. Harvest the answer

When the user has seen enough:

- The question's answer, with what the prototype demonstrated as
  evidence, goes to `/decide` — rejected variations are exactly the
  *rejected alternatives* a decision needs (this is the cheapest
  alternatives section you will ever write).
- A validated design that should now be built → `/to-prd`, with the
  prototype's lessons in the feature prose.
- Then delete the prototype directory (or leave it with a `DO NOT SHIP`
  note if the user insists) — never merge it, never "clean it up into"
  production code. Production starts fresh, test-first.

## Hard rules

- One named question per prototype; a second question is a second
  prototype.
- UI variations must be radically different — three shades of the same
  idea answer nothing.
- Prototype code never reaches production by refactoring; the knowledge
  moves, the code does not.
- The session ends with the answer captured durably (`/decide` /
  `/to-prd`) or explicitly declared open — never with "interesting,
  anyway".
