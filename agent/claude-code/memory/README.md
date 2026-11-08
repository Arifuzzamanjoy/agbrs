# Agent memory snapshot

`gba-takehome-state.md` is the memory file Claude Code wrote for itself
during the 2026-11-07 sessions, copied as it was at the end of session
`01dfc9aa` (with the same path and account redactions as the
transcripts). It is the agent's working notes, not documentation, so some
items in it were open at that point and were handled afterwards:

| Item in the snapshot | Where it was handled |
|---|---|
| Earlier handoff claims flagged as unverifiable (commit `5fd7684`, missing `results/summary.json`, transcripts that were write-ups, `waitstates.rs` "mirroring Mesen2") | Corrected and stated openly: `agent/antigravity/README.md` (note at the top), `docs/DECISIONS_AND_FAILURES.md`, `results/`, and `HANDOFF.md` "Disclosures" |
| "Two author TODOs left" (prior experience, human time) | Filled in: `HANDOFF.md` "Disclosures" and `docs/WORKFLOW_METADATA.md` §3 |
| `[[gba-handoff-honesty]]` | A link to a note the agent planned but never wrote; there is no such file |
| ">2 threads OOMs" | Later runs showed even 2 threads can run out of memory on that 8 GB machine; the re-grade in `results/recheck-2026-11-08/` used 1 |
