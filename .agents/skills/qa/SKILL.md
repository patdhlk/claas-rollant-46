---
name: qa
description: Interactive QA session - the user reports bugs and observations conversationally while testing, and each report is filed as an issue on the configured backend with duplicate detection. Use when the user wants to do a QA session, report bugs conversationally, file issues while testing, or says "QA session" / "let me report some bugs".
---

# QA

The user is testing; you are the scribe. Keep filing friction near zero —
short confirmations, no interrogations. Everything files at
``needs-triage``: routing is `/triage`'s job, not yours (ADR_0005).

## Workflow

### 1. Open the session

Read `[tool.patdhlk-skills]` from `ubproject.toml` (missing → point to
`/setup-patdhlk-skills`).

- **sphinx-needs**: read glossary terms from needs.json (ADR_0006) so
  reports get written in the repo's domain language. `pds` missing from
  PATH → emit one loud line pointing at `/setup-patdhlk-skills`, then
  degrade to a one-sentence jq title scan over needs.json.
- **github**: `gh issue list --state all --limit 200` title matching.
  (`pds dedup` exits 2 on the github backend — no v1 driver.)

Skim the codebase for the named components when it helps phrase a report
precisely — in the background, never blocking the user.

### 2. The loop — per report

1. **Capture**: title (symptom, not diagnosis), what happened vs what was
   expected, repro steps as far as known. `kind`: `bug` for defects,
   `feature`/`improvement` for ideas mid-session.
2. **Clarify at most once** — only if the report is unfilable as heard
   (e.g. no clue what action triggered it). Otherwise file what's known;
   `/triage` will route thin reports to `needs-info` later.
3. **Dedup** — on the sphinx-needs backend, run `pds dedup "<full draft>"`
   (title + body — never a bare title; short queries over-gate). Branch on
   exit code:
   - **exit 0**: proceed to file.
   - **exit 1**: duplicate verdict — build one short status-aware ask from
     the hits JSON (`{id, type, status, title, score}`): if the top hit is
     an open issue, default ask is "append detail to ISSUE_xxxx instead?";
     if the top hit is done or an ADR, ask "ISSUE_xxxx (done) — may already
     be shipped — file as regression?". Never a hard block; never silent
     filing.
   - **exit 2**: tool/config error — stop and escalate.
   On the github backend `pds dedup` exits 2; fall back to title matching
   against the `gh issue list` corpus. Also consider this session's
   declined and filed reports — `pds` rebuilds needs.json after each
   filing, but declined reports are not in the corpus.
4. **File** after a one-line confirmation ("Filing: <title> — ok?"):

   - *sphinx-needs*: rebuild needs.json, allocate dense max+1 ID
     (ADR_0008), append to `issue_doc`:

     ```rst
     .. issue:: Theme toggle resets after logout
        :id: ISSUE_0013
        :status: needs-triage
        :kind: bug

        Logout and back in: theme is light again although dark was
        chosen. Expected: choice survives sessions. Seen on Safari 17.
     ```

     Link `:links: REQ_xxxx` only when the user names the behavior a
     requirement covers — don't go hunting.
   - *github*: `gh issue create` with the `needs-triage` label (create the
     label if missing).

5. Acknowledge with the ID and return to listening. Never editorialize on
   severity or cause mid-session.

### 3. Close the session

When the user is done: run the strict gate over any local mutations
(ADR_0007, ADR_0017) —

```bash
pds check
# no pds: uv run sphinx-build -W -b html <spec_dir> <spec_dir>/_build/html
```

— exit 1 means fix the corpus and re-run, exit 2 means stop and escalate —
then summarize: filed issues (ID + title), deduped reports, anything the
user mentioned but declined to file. Suggest `/triage` as the natural next
step — the session deliberately leaves everything at `needs-triage`.

## Hard rules

- Everything files at `needs-triage` — `/qa` never routes, never sets any
  other status.
- One clarifying question per report, maximum. Thin reports still get
  filed; that's what `needs-info` exists for.
- Always dedup before filing; never silently file a known twin.
- The strict gate must pass before the session summary claims success.
