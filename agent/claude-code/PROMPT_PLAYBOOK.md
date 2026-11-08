# Prompt playbook

> **This is a paraphrase, not a transcript.** It restates the prompts I gave
> Claude Code as a reusable sequence, condensed and with typos fixed. The
> exact wording, including the pasted take-home brief (redacted) and every
> follow-up, is in [`../transcripts/claude-code/`](../transcripts/claude-code/).
> The session and event index (0-based line of the `.jsonl`) given for each
> step point to the original prompt.

The loop below is what produced the 0.3789 → 0.6900 improvement. It needs no
custom prompts, skills or hooks: a stock Claude Code session in auto
permission mode, with the gba-eval checkout next to this repo.

## 1. Audit against the brief

*Session `02b30b4e`, event 2.*

> Here is the take-home brief and the repo. Check whether the codebase meets
> what the task asks for. The benchmark score is low: look at current
> methods (papers, open-source projects) for getting closer to the
> reference, then implement them, validating each change.

What the agent did with it:
- read `gba-eval` (`spec/ABI.md`, `corpus/`, the grader) and the repo
- graded the wasm as received to get a baseline (0.3789)
- listed root causes before changing code (`HANDOFF.md`, "What was wrong")

## 2. Iterate with the grader, not by reading code

No separate prompt; this is the working rule the session settled on. For
each fix:

1. build the wasm
2. `tools/quick-grade.sh <wasm> <run> <testcases…>` on the affected subset
3. if replays drift: `tools/lockstep` against `reference/mesen.wasm` to find
   the first PC/cycle divergence
4. fix, add a case to `tests/regressions.rs`
5. full `tools/grade.sh` before keeping the change; keep every run in
   `results/`, regressions included

## 3. Commit and push

*Session `02b30b4e`, events 2442–2543.*

> Push these changes to my repo with a short, plain commit message and no
> AI co-author line. … Give
> me the manual push commands instead; I'll run them from my own terminal.

## 4. Re-audit and write up the gaps

*Session `02b30b4e`, events 2555–2607; session `01dfc9aa`, event 2.*

> Here is the brief again. Check whether the repo now meets it, add a
> section on what still does not, and add every missing handoff item you
> can (transcripts, all grading runs, logs, toolchain pin, disclosures).

## Reusing this

For a similar take-home, steps 1, 2 and 4 carry over unchanged. Step 2 is
the part that matters: a differential tool against the reference (here
`tools/lockstep`) turned timing work from guessing into measuring, and
grading every build kept regressions visible (see `v6-full` in
`docs/DECISIONS_AND_FAILURES.md`).
