---
name: diagnose
description: Disciplined diagnosis loop for bugs and regressions - reproduce, minimise, hypothesise, instrument, fix, regression-test - wired into the issue lifecycle like /tdd. Use when the user reports a bug, says something is broken/throwing/failing, describes a performance regression, or hands over a bug issue to fix.
---

# Diagnose

Debugging is hypothesis testing, not code staring. The loop below is
sequential and rigid on purpose — the steps people skip (reproduce,
minimise) are the ones that prevent fixing the wrong thing.

## Picking up the work

Same worker contract as `/tdd`: a cited issue (`ISSUE_xxxx` or GitHub
`#n`) is resolved through the configured backend — local via a fresh
needs.json (the issue body usually carries the repro; `/qa` wrote it that
way), github via `gh issue view`. Set it `in-progress` before starting
(local: `:status:` edit + strict gate; github: label swap).

## The loop

1. **Reproduce.** Get the failure happening on demand — a command you can
   re-run, ideally as a failing test. No reliable repro → that IS the task;
   don't hypothesise about a failure you can't summon. If a repro is truly
   out of reach, say so and downgrade all conclusions accordingly.

2. **Minimise.** Shrink input, config, and code path until everything left
   is load-bearing. Every removed element is a hypothesis eliminated for
   free. For regressions: `git bisect` is minimisation over history — use
   it before reading code.

3. **Hypothesise.** State the suspected mechanism *before* looking deeper
   — written, falsifiable, one at a time, most-likely-first. "Something
   async" is not a hypothesis; "the cache returns the stale entry because
   invalidation runs before the write commits" is.

4. **Instrument.** Add the observation that would prove or disprove the
   current hypothesis (log line, assertion, debugger breakpoint, trace).
   Run the repro. Disproved → back to 3 with the new evidence; never patch
   "while you're in there".

5. **Fix.** Only with a confirmed mechanism: the minimal change addressing
   the cause, not the symptom. Remove the instrumentation. If the real fix
   contradicts a recorded decision, stop and raise it (`/decide` to
   supersede) rather than quietly working around it.

6. **Regression-test.** The repro from step 1 becomes a permanent test
   that fails on the old code and passes on the new — run both directions
   when feasible (stash the fix, watch it fail). Whole suite green.

## Closing the work

As `/tdd`: set the issue `done` only with the regression test passing in
the transcript; report mechanism (one paragraph: cause → effect → fix),
the test that guards it, and any decisions the fix raised.

## Hard rules

- No fix without a confirmed mechanism — "it stopped happening" is not
  confirmation, it's a lost repro.
- One hypothesis under test at a time; record disproved ones so they stay
  disproved.
- The regression test is not optional; a fix without one is a fix on
  borrowed time.
- Performance regressions follow the same loop with measurements as the
  repro — numbers before and after, same machine, same load.
