---
name: tdd
description: Test-driven development in vertical slices - red-green-refactor against real behavior, wired into the issue lifecycle (picks up an issue, sets in-progress, closes done). Use when the user wants to build a feature or fix a bug test-first, mentions TDD or red-green-refactor, or hands over an issue ID to implement.
---

# TDD

Red, green, refactor — in vertical slices, against behavior. As the worker
skill it owns the issue transitions `/triage` never makes: `in-progress`
when work starts, `done` when it's verified.

## Picking up the work

When the task cites an issue (an `ISSUE_xxxx` ID or a GitHub `#n`), resolve
it through the configured backend (`[tool.patdhlk-skills]` in
`ubproject.toml`):

- **sphinx-needs**: fresh needs.json (ADR_0006) → the issue's body, its
  `implements` links, and those requirements' shall-statements. The reqs
  are the acceptance criteria.
- **github**: `gh issue view` → body, plus the need IDs from its
  `Implements:` line, resolved against needs.json the same way.

Set the issue `in-progress` before the first test (local: edit `:status:`
+ strict gate; github: swap the state label). No cited issue → work from
the conversation; the loop below is unchanged.

## The loop — per slice

Slice the work vertically: each slice is one behavior a user (or caller)
can observe end to end — never "the models, then the handlers, then the
UI".

1. **RED** — write one test for the next behavior. Run it. Watch it fail,
   and fail *for the right reason* (asserting the missing behavior, not a
   typo). A test you never saw fail proves nothing.
2. **GREEN** — write the minimum code that passes. Resist building ahead
   of the test. Run the whole suite, not just the new test.
3. **REFACTOR** — with everything green: remove duplication, name things
   after the domain (the glossary's terms, if the repo has one), simplify.
   Suite stays green throughout.
4. **Commit** the green slice before starting the next.

Test behavior, not implementation: prefer the realest level you can afford
(integration over mocked-out units); assert on observable outcomes, not on
internals having been called. Mock only at boundaries you don't own.

## Closing the work

When all acceptance criteria are demonstrably met (run the suite, show the
output — claims without runs don't count):

- Set the issue `done` (local: `:status:` edit + strict gate; github:
  close the issue, the `Implements:` links keep traceability).
- If the work completed a requirement still marked `draft`, offer to flip
  it to `implemented` — ask, don't assume; the user may want review first.
- Report: slices shipped, suite status, issue + req transitions made.

## Hard rules

- Never write production code without a failing test demanding it.
- Never weaken, skip, or delete a test to get to green — if the test is
  wrong, say so and fix it as its own visible step.
- One slice in flight at a time; the suite is green between slices.
- `done` requires a run with passing output in the transcript, not an
  assertion that it "should pass".
