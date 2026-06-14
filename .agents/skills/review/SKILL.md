---
name: review
description: Review the changes since a fixed point (commit, branch, tag, or merge-base) along two axes in parallel - Standards (does the code follow this repo's documented rules?) and Spec (does it deliver what the originating issue and its linked requirements asked for?). Use when the user wants to review a branch, a PR, or work-in-progress changes, or says "review since X" / "review this before I merge".
---

# Review

Two questions, answered independently and reported side by side: is the
code *well made* (Standards), and is it *the right thing* (Spec)? One
review that mixes them lets charm on one axis hide failure on the other.

## 1. Fix the review point

Resolve what "the changes" means: an explicit commit/branch/tag from the
user, else the merge-base with the default branch
(`git merge-base HEAD origin/main`). The diff since that point — plus
untracked files — is the entire review surface. State the resolved range
before reviewing.

## 2. Run both axes — as parallel sub-agents

Dispatch two sub-agents, each with the diff and its own brief; neither
sees the other's findings (independence is the point).

**Standards axis.** Reviews against what the repo *documents*: CLAUDE.md /
AGENTS.md rules, CONTRIBUTING, lint/format configs, glossary terms for
naming (a fresh needs.json supplies them, ADR_0006). Personal taste that
no document backs is out of bounds — at most a closing note suggesting the
rule be recorded (`/decide` or a term) if it keeps coming up.

**Spec axis.** Walks the traceability chain: the originating issue
(cited `ISSUE_xxxx` via needs.json, or GitHub `#n` via `gh issue view` +
its `Implements:` line) → the linked requirements' shall-statements → any
accepted decisions touching this area. Then three checks against the diff:

- **Delivered?** Each shall-statement demonstrably satisfied (or
  explicitly deferred).
- **In scope?** Changes beyond what the issue asked for are flagged —
  even good ones; they belong in their own issue.
- **Consistent?** Nothing contradicts an accepted decision. A necessary
  contradiction is a finding that routes to `/decide`, never a silent
  pass.

No originating issue resolvable → say so and run the Spec axis against
the user's stated intent instead, marked as such.

## 3. Report side by side

One verdict per axis (pass / pass-with-findings / fail), then findings as
`file:line — what — why it matters — suggested fix`, each tagged
**[standards]** or **[spec]**. No finding without a location; no "consider
maybe possibly" hedging — either it's a finding or it isn't.

## 4. Findings with consequences

Review mutates nothing — code and issue statuses stay untouched (the
worker skills own those). But offer routing for what surfaced:

- defects out of this change's scope → file via the issue backend
  (`needs-triage`, as `/qa` would),
- an ambiguous requirement that caused a spec miss → `needs-info` on it,
- a contradicted or missing decision → `/decide`.

## Hard rules

- The two axes run independently — never let one sub-agent see the
  other's findings before both report.
- Standards findings cite the document/config they enforce; undocumented
  taste is not a finding.
- Scope creep is a finding even when the extra code is good.
- Review never flips issue or requirement statuses — it only recommends.
