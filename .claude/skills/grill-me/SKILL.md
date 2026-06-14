---
name: grill-me
description: Interview the user relentlessly about a plan or design until reaching shared understanding, walking every branch of the decision tree and resolving dependencies between decisions one by one. Use when the user wants to stress-test a plan, get grilled on a design, or says "grill me" / "poke holes in this".
---

# Grill Me

Interview the user relentlessly about their plan until you reach a shared
understanding. The output is not code — it is a decision tree with every
branch walked and every dependency between decisions resolved.

## Rules

1. **Research before you ask.** If a question can be answered by exploring
   the codebase, referenced repos, or documentation — explore instead of
   asking. Open the interview only when you know enough for your questions
   to be informed. Never ask the user to describe what you can read.

2. **One question at a time.** Each question targets exactly one decision.
   No batched questionnaires — the answer to this question shapes the next
   one. That sequencing is the whole method.

3. **Always recommend.** Every question comes with your recommended answer
   and the reasoning, listed first. Recommendations give the user something
   to push against; pushback is signal.

4. **Order by dependency, not importance.** Start with the decision that
   the most other decisions hang on (scope, then architecture seams, then
   details). When an answer invalidates an earlier branch, say so and
   revisit — don't quietly carry the contradiction.

5. **Chase the answer down the branch.** An answer usually opens
   sub-decisions; resolve those before jumping to a new branch. Announce
   branch switches ("That settles the read path. Next branch: writes.").

6. **Track diverged answers.** When the user rejects your recommendation,
   that is a decision too — note it and respect it for the rest of the
   session; do not re-litigate.

7. **Surface your assumptions.** Anything you folded in without asking gets
   flagged explicitly (marked *assumption*) so the user can veto it cheaply.

## Ending the session

Stop when the tree is walked — when remaining questions would only produce
detail the user would rather decide during implementation. Then deliver the
shared understanding:

- every decision with its chosen answer (flag where the user diverged from
  your recommendation),
- all assumptions, marked, for cheap veto,
- open ends explicitly listed as open.

Ask whether anything is still soft before treating the design as settled.

## Afterwards

The understanding is conversation context — durable only once captured.
Offer the natural sink: `/to-prd` for a feature design, `/decide` for
architecture decisions, or both.
