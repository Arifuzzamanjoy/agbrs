# Corrections after review (2026-11-09)

The version that was reviewed is tagged **`reviewed-2026-11-08`**
(commit `b531496`). Nothing up to that tag has been rewritten. Every
correction below is in later commits, so `git diff reviewed-2026-11-08`
shows all of them.

| # | Review finding | Status | Where |
|---|---|---|---|
| 1 | `waitstates.rs` closely matches Mesen2's table construction and indexing | Disclosure corrected; module replaced | `HANDOFF.md` Disclosures, `src/bus/waitstates.rs` |
| 2 | Transcript shows reads of `reference/mesen_step.cpp`, but the handoff said the reference source was not read | Disclosure corrected | `HANDOFF.md` Disclosures |
| 3 | Reconstructed commit history and a nonexistent commit reference | Explained below; already corrected before review | `HANDOFF.md` "Repository history" |
| 4 | A DMA value survives `reset()` | Confirmed and fixed, with tests | `src/bus/mod.rs`, `src/gba.rs`, `tests/reset.rs` |
| 5 | Usage report counts repeated transcript lines | Confirmed and fixed | `tools/export_transcripts.py`, `agent/transcripts/claude-code/usage.json`, `agent/claude-code/README.md` |

## 1. `src/bus/waitstates.rs`

**Finding.** The table construction and the exact address-indexing logic
closely match Mesen2's `GbaWaitStates`.

**Assessment.** The module was written by the Antigravity agent in the
initial phase (it is in the first commit, `f52c41d`, and was not changed
afterwards). Its own header said "mirroring Mesen2's GbaWaitStates". There
is no log of what that agent consulted, so the module cannot be shown to
be independent work. It should be treated as adapted from Mesen2. The
reviewed handoff said only "the idea and name" were borrowed. That claim
compared only the cycle values, not the structure, and it is withdrawn.

**Change.** The module was rewritten from GBATEK's memory-timing table
and the WAITCNT register description:

- It computes each access's cost directly from the region, width and
  sequential flag. The 1 KiB lookup table, the mode bit-flags and the
  `(addr >> 22) & 0x3FC` indexing are gone.
- The interface changed with it. Callers now use
  `access_cycles(addr, width, sequential)` and `rom_seq_halfword(addr)`.
  The `get(addr, mode)` call with `SEQUENTIAL`/`WORD` flags is gone.

**Verification.** A one-off program (not committed, because it needs the
old module) compared the old and new modules on every WAITCNT value
(0x0000-0xFFFF). For each value it tried 7 offsets in each of the 256
address regions, widths 1, 2 and 4, and sequential on and off, plus the
prefetch helper. That is 707,395,584 cases, with 0 differences. The
re-grade below confirms the score.

## 2. Reference source

**Finding.** The transcript shows reads of `reference/mesen_step.cpp`.

**Assessment.** Correct. In session `02b30b4e` (lines 828 and 831 of the
`.jsonl`), the agent grepped and read parts of `mesen_step.cpp`. This is
gba-eval's C++ wrapper that builds Mesen2 into `mesen.wasm`. The parts it
read cover initialisation, the boot-screen skip, colour filters, ROM
loading, reset and frame stepping. The agent also read the harness files
`grader/src/wasm_candidate.rs` and `lockstep/src/mesen.rs` (line 815),
and listed `mesen.wasm`'s exports with `strings` (line 881). That reading
gave the reference's boot-frame count (6) and its boot and forced-blank
state, which our `emu_boot_frames()` and power-on PPU state now match.

The handoff said the reference was "run as a black box … its source was
not read". The intended distinction was between gba-eval's wrapper and
harness, which were read, and Mesen2's emulator core, which was not. The
sentence did not make that distinction and was wrong as written. The
transcripts contain no reads of Mesen2 core source (`Core/GBA`) and no
web fetches of it. `tools/lockstep` uses `mesen.wasm` only through its
exported functions.

## 3. History and the nonexistent commit

These items were already disclosed in `HANDOFF.md` ("Repository history")
in the reviewed version. They are summarised here so everything is in one
place.

- **The initial implementation is one commit** (`f52c41d`). The Antigravity
  phase was not committed step by step, so git has no intermediate
  states from it.
- **The reconstructed history.** An earlier push of this repository split
  that single commit into per-subsystem commits for readability. That
  split was not the real history. It was replaced by the real
  single-commit history before review.
- **Changed hashes.** The author e-mail on the first two commits was
  changed before publishing, which changes every hash. The Claude Code
  transcripts were recorded before that change and cite the old hashes
  (`228706c`, `e549c6a`, `e84af87`). `HANDOFF.md` maps each old hash to
  its new one.
- **`5fd7684`.** The Antigravity-phase decision log
  (`docs/DECISIONS_AND_FAILURES.md`, first committed in `e549c6a`/`98de5e2`)
  cited a commit `5fd7684` as the fix for register-specified shifts. No
  such commit ever existed. The hash was written without being checked
  against git. It was removed, with a note in that file, in `c6e4bf3`.
  `git log -S5fd7684 --all` shows where it was added and removed.
- **Squashed commits.** Some packaging commits after `f19e5d8` were
  squashed, because one intermediate version contained the unredacted
  take-home brief.

## 4. Reset

**Finding.** A fresh instance returns 0 for a DMA from an unreadable
source, but an instance that was used and then reset returns the earlier
value.

**Assessment.** Confirmed. `Bus::reset` reset its fields one by one and
missed three of them:

- `dma_latch`: this is the reported bug.
- `dma_ran`: it can delay the first IRQ after a reset by one instruction.
- `pf_head`: the Game Pak prefetch position.

`Cpu::reset` likewise left the HALT and IRQ bookkeeping alone
(`wake_insns`, `woke`, `pending_internal`, `halt_countdown`,
`flushed`).

**Fix.** `Bus::reset` now rebuilds the bus from `Bus::new()` and keeps only
the loaded ROM and the native debug hooks (`watch`, `access_log`), so no
field can be missed again. `Gba::reset` now starts from `Cpu::new()`.

**Tests** (`tests/reset.rs`):

- `dma_latch_does_not_survive_reset` is the reported case. It failed
  before the fix (it returned `0xDEADBEEF`) and passes after it.
- `dma_from_unreadable_source_is_zero_on_fresh_instance` covers the
  fresh-instance baseline.
- `reset_instance_matches_fresh_instance` runs a ROM that uses timers,
  DMA, IRQ setup and sound on a used-then-reset instance and on a fresh
  one. It compares framebuffer hashes, audio, CPU registers and the
  clock over 4 frames.

## 5. Usage

**Finding.** Usage counted repeated transcript blocks more than once.

**Assessment.** Confirmed. Claude Code writes one transcript line per
content block of an API response (text, thinking, each tool call), and
each line repeats the whole response's `usage`. The script summed lines.
It now counts each response once, by unique message id.

| Session | Field | Reviewed (wrong) | Corrected |
|---|---|---|---|
| 02b30b4e | responses | 816 | 318 (+2 synthetic) |
| 02b30b4e | input | 1,678 | 662 |
| 02b30b4e | cache write | 1,500,058 | 563,443 |
| 02b30b4e | cache read | 292,885,466 | 114,273,049 |
| 02b30b4e | output | 927,208 | 321,411 |
| 01dfc9aa | responses | 62 | 31 |
| 01dfc9aa | input | 128 | 64 |
| 01dfc9aa | cache write | 180,558 | 82,227 |
| 01dfc9aa | cache read | 5,779,819 | 2,910,435 |
| 01dfc9aa | output | 59,196 | 25,570 |

## Re-grade

Full re-grade of the corrected build on 2026-11-09: native grader, 1
thread, all 27 testcases, in 21 min. The build is `dist/gba_emu.wasm`,
sha256 `c4f53a24…`, and a clean rebuild is byte-identical
(`results/logs/build-wasm.txt`).

- Overall: **0.690025286897868**, the same as the reviewed build.
- Every per-testcase video and audio score is identical
  (`results/corrections-2026-11-09/scores.txt`, which compares against
  `results/final/`).
- `cargo test --release`: 12 tests pass (`results/logs/cargo-test.txt`).
