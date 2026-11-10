# Claude Code: configuration, sessions and usage

This folder covers the 2026-11-07 work done with **Claude Code** (desktop
app, Code tab). The earlier implementation work used a different tool; see
[`../antigravity/`](../antigravity/README.md).

## Setup

| Item | Value |
|---|---|
| Client | Claude Code 2.1.317, desktop app (Linux host, `uname -sr` = Linux 6.8.0-147-generic) |
| Model | `claude-opus-5-5` (Claude Opus 5.5) for every assistant turn; no other model was used |
| Subagents | None. Both sessions ran single-agent (`isSidechain` events = 0). |
| Custom prompts / skills / hooks | None. No `CLAUDE.md`, no project `.claude/` settings, no custom skills, hooks or MCP servers for emulator work. The system prompt is the stock Claude Code one and appears in the transcripts. |
| Persistent memory | One memory file written by the agent, copied into [`memory/`](memory/) (same path and account redactions as the transcripts; [`memory/README.md`](memory/README.md) notes which of its open items were handled later). It carries state from session 1 to session 2. |
| Permission mode | `auto` in both sessions (recorded as `permissionMode` in the transcripts) |
| Tools used | Shell (cargo, the gba-eval grader/oracle, python), file read/edit/write, WebSearch/WebFetch (used to find hardware *test ROM* sources and docs, not emulator source code) |

The working loop is described in `HANDOFF.md` ("Method") and
`docs/REPRODUCIBILITY.md` section 5: build → `tools/quick-grade.sh` on a
subset → `tools/lockstep` against `reference/mesen.wasm` to find the first
divergence → fix → regression test → full `tools/grade.sh`.

## Sessions ([`../transcripts/claude-code/`](../transcripts/claude-code/))

| Session id | Wall clock (UTC) | Purpose | Files |
|---|---|---|---|
| `02b30b4e-600f-4585-ac58-ba7f06db074a` | 2026-11-07 11:40 → 16:04 (4 h 23 min) | Requirements check, root-causing the low score, timing/APU/PPU fixes, lockstep tool, grading runs v1–v13, handoff docs (commit `f19e5d8`, cited as `e84af87` in the transcript) | `.jsonl` (full event stream), `.md` (readable rendering) |
| `01dfc9aa-7822-48ad-9e5d-c1c7d6557493` | 2026-11-07 16:05 → 16:14 | Final compliance audit against evaluation requirements; added transcripts, intermediate results, logs, toolchain pin, doc corrections | `.jsonl`, `.md` |

Personal and unrelated details in the logs are replaced by `[redacted-…]` markers; everything else is as recorded. A paraphrased version of the prompts is in [`PROMPT_PLAYBOOK.md`](PROMPT_PLAYBOOK.md).

## Model usage (from the transcripts' `usage` fields; see [`../transcripts/claude-code/usage.json`](../transcripts/claude-code/usage.json))

| Session | API responses | Input | Cache write | Cache read | Output |
|---|---|---|---|---|---|
| 02b30b4e (timing pass) | 318 (+2 synthetic) | 662 | 563,443 | 114,273,049 | 321,411 |
| 01dfc9aa (handoff audit) | 31 | 64 | 82,227 | 2,910,435 | 25,570 |

Counted once per API response (unique message id). The reviewed version
of this table (tag `reviewed-2026-11-08`) summed every transcript line;
Claude Code repeats a response's usage on each of its content blocks, so
those figures were 2.3-2.9x too high (e.g. output 927,208 instead of
321,411). See `CORRECTIONS.md`.

Cost in dollars is not recorded by the client and is not estimated here.
