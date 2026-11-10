# Handoff: GBA Emulator (agbrs)

> **Corrected 2026-11-09 after review.** See [`CORRECTIONS.md`](CORRECTIONS.md)
> for what changed and why. The reviewed version is tag `reviewed-2026-11-08`.

## Overview

Original Game Boy Advance emulator built from scratch in Rust, targeting `wasm32-unknown-unknown`. Implements all major GBA hardware subsystems: ARM7TDMI CPU (ARM + Thumb), memory bus, PPU, DMA, timers, interrupts, and audio (PSG + DirectSound FIFO).

## Approach

### Phase 1: Research & scaffold
Read through GBATEK and the ABI spec. Set up the Cargo project with correct `cdylib` crate type and the 10 required C-ABI exports. Got a blank wasm compiling first before writing any emulation logic.

### Phase 2: CPU + Memory (highest leverage)
Implemented the ARM7TDMI with all data processing, multiply, single/block transfer, and branch instructions for both ARM and Thumb modes. Built the memory bus with correct address mapping for all GBA regions (BIOS, EWRAM, IWRAM, IO, Palette, VRAM, OAM, ROM, SRAM). Loaded the provided BIOS stub at 0x00000000.

Key decisions:
- 3-stage pipeline model: PC always reads as current_instruction + 8 (ARM) or + 4 (Thumb)
- Wrapping arithmetic everywhere to satisfy the zero-panic policy
- Open-bus reads for unmapped regions instead of crashing
- 16-bit IO bus routing: `read_io8` delegates to `read_io16` and extracts the right byte

### Phase 3: PPU
Scanline-based renderer running at cycle 960 (end of HDRAW). Supports:
- Modes 0-2 (tiled backgrounds with regular and affine layers)
- Modes 3-5 (bitmap modes)
- OAM sprites (regular and affine, all sizes)
- Windowing (WIN0, WIN1, OBJ window)
- Alpha blending and brightness effects
- Priority-based layer compositing

### Phase 4: DMA, Timers, Audio
- 4 DMA channels with priority ordering and audio FIFO triggers
- 4 cascading timers with prescaler dividers
- PSG channels 1-4 (square waves, wave RAM, noise)
- DirectSound FIFO A and B at 32768 Hz stereo

### Phase 5: Debug & iterate
Used the grader and oracle tools to identify failing tests and fix specific issues:
- LDM/STM block transfer had wrong base address calculation for DA/DB modes
- Thumb Format 8 had swapped operands (LDSB vs LDRH)
- IO register reads needed 16-bit bus alignment
- DMA registers returned wrong open-bus values
- Register-specified shifts needed PC+12 for R15 reads

## Architecture

```
src/
├── lib.rs          # WASM ABI exports (10 required + emu_boot_frames + 3 debug)
├── gba.rs          # Top-level Gba struct, frame loop
├── cpu/
│   ├── mod.rs      # Pipeline, mode switching, interrupt handling
│   ├── arm.rs      # ARM instruction decoder/executor
│   ├── thumb.rs    # Thumb instruction decoder/executor
│   ├── alu.rs      # ALU operations with flag computation
│   └── registers.rs # Banked register definitions
├── bus/
│   ├── mod.rs      # Memory map, read/write dispatch
│   ├── io.rs       # IO register bitfield constants
│   └── waitstates.rs # Memory waitstate table (WAITCNT)
├── ppu/
│   └── mod.rs      # Scanline renderer, all BG modes, sprites
├── dma/
│   └── mod.rs      # 4-channel DMA controller
├── timer/
│   └── mod.rs      # 4 cascading timers
└── apu/
    └── mod.rs      # PSG + DirectSound FIFO audio
```

Total: ~5,600 lines of Rust in `src/` (plus ~270 in `tools/lockstep`).

## 2026-11-07 pass: timing, audio and differential testing

I did this pass with Claude Code (model: Claude Opus 5.5). Goal: check the repo against the evaluation requirements,
find why the score was low, and raise it with validated changes.

**How I worked with the agent.** My prompts are short and informal, and
they are kept verbatim in the transcripts, typos included. The first one
gives the agent the brief and the repo and asks three things: does the code
meet the brief, why is the score low, and which better methods exist,
implemented with validation. I kept it open-ended so the agent would work
out the plan from the brief and the grader itself. My part was steering and
review: I read the diffs in my IDE and the grading results as they came in,
rather than writing long prompts. The later prompts are about re-checking
the brief and pushing. Commits use a plain style without the client's
automatic `Co-Authored-By` line (my prompts ask for that), because the AI
use is disclosed in full here and in `agent/` rather than per commit. A
cleaned-up version of the same prompt sequence, usable as a template, is in
[`agent/claude-code/PROMPT_PLAYBOOK.md`](agent/claude-code/PROMPT_PLAYBOOK.md).

### Score (gba-eval grader, native build, reference = bundled Mesen2)

| Run | Overall | Replay (60%) | Procedural (20%) | Audio (20%) |
|---|---|---|---|---|
| Baseline (`dist/` wasm as first committed, sha256 9608c373…) | 0.3789 | 0.4606 | 0.4261 | 0.0865 |
| Intermediate (v8: APU rewrite, access timing, frame/boot alignment) | 0.5655 | 0.5537 | 0.5861 | 0.5802 |
| v11 (+ lockstep-derived IRQ/DMA/prefetch rules, input latching) | 0.6775 | 0.7137 | 0.5861 | 0.6605 |
| **Final** as reviewed (sha256 b2705a31…) | **0.6900** | **0.7181** | **0.5861** | **0.7096** |
| **Corrected 2026-11-09** (`dist/gba_emu.wasm`, sha256 c4f53a24…; reset fix, waitstates rewrite) | **0.6900** | **0.7181** | **0.5861** | **0.7096** |

Per-testcase tables: `results/` (baseline, v8, v11, final), produced by
`tools/scores.py`.

### What was wrong (root causes found)
1. **Timing model unused.** A waitstate table existed but no instruction
   was ever charged memory waitstates; every instruction cost a fixed 1-3
   cycles, so games ran several times too fast relative to the PPU.
2. **HALT never halted.** HALTCNT was stored but the CPU kept running.
3. **APU clocked for the Game Boy, not the GBA.** Square/wave/noise timers
   used GB multipliers (pitch 4x too high), the frame sequencer
   (length/envelope/sweep) did not exist, and timers advanced at most one
   step per call. tonc-snd1 audio went 0.10 → 1.00 after the rewrite.
4. **Branch detection bug.** `step()` decided "no branch" by comparing r15
   before/after; a BX to the next word or `b .` refills to the same r15,
   so an extra ARM word was fetched. chip-advance hung in its crt0 because
   of this (video 0.03 before).
5. **Frame boundary and boot alignment.** The reference ends frames at
   VBlank start and reports 6 boot frames (`emu_boot_frames`); we ended
   frames at line 0 and reported 0, so every replay was phase-shifted.
6. **Input applied one frame early.** The reference latches keys at the
   frame boundary; we applied them immediately (goodboy: 840 → 48
   differing frames of 900).
7. **Fixed 32768 Hz output.** Games selecting 65536 Hz via SOUNDBIAS get
   twice the samples from the reference; the grader compares streams
   sample-for-sample (piugba audio 0.06 → 0.70).
8. **Mosaic not implemented; darken rounding off by one** (chip-advance
   fades).
9. Many smaller hardware rules (VRAM mirror hole in bitmap modes, open
   bus = last prefetched opcode, BIOS read protection, ROM out-of-range
   reads, DMA masks/latch/ROM-increment, DISPSTAT byte writes clearing
   status bits, PPU lagging one scanline per call after long DMAs,
   write-only DMA registers reading 0, timer start/stop latency).

### Method
- Baseline first, then every change validated with the real grader
  (`tools/quick-grade.sh` on subsets, `tools/grade.sh` for full runs).
- **Instruction-level lockstep against the reference** (`tools/lockstep`):
  runs `reference/mesen.wasm` one instruction at a time beside our core and
  reports the first PC/cycle divergence with per-access logs. This turned
  timing work from guesswork into measurement; see `docs/TIMING_MODEL.md`
  for every rule and its evidence. goodboy-advance now runs 8M+
  instructions in exact PC lockstep.
- Regression tests for each fixed bug: `tests/regressions.rs`.

### Known limitations after this pass
See `docs/TIMING_MODEL.md` "Known gaps": PPU/CPU VRAM contention not
modelled, prefetch-buffer and DMA edge cases (±1 cycle), a few IRQ
boundaries, timer reload-while-running, multiply-long carry flag,
mgba-io-read (48/130), mgba-dma (1248/1256).

### What I'd do next
1. Model VRAM/palette/OAM contention from the PPU's fetch schedule (the
   tracer's `--vram-stats` gives the target histogram). This is now the
   largest source of drift in mode-0 games.
2. Fit the remaining IRQ sampling rule with `--irq-report` data across
   all games.
3. Finish prefetch/DMA edge cases with `--branch-stats`-style histograms.
4. Remaining procedural tests (timer reload, multiply-long carry, I/O
   read-back) — each is pass/fail and worth ~0.01-0.02 overall.

## Repository history

The history is the one the work was actually committed with, in a private
working repository that was later published as this one:

| Commit | In transcripts | What |
|---|---|---|
| `f52c41d` | `228706c` | Initial implementation (Antigravity phase), committed as one commit |
| `98de5e2` | `e549c6a` | First handoff docs, baseline wasm (0.3789); its results and limitations are superseded by the 2026-11-07 pass above |
| `f19e5d8` | `e84af87` | 2026-11-07 timing/APU pass with Claude Code (0.6900) |
| latest | — | Handoff completion: transcripts, all grading runs, logs, doc fixes |

The initial implementation was not committed step by step, so the history
cannot show its intermediate states; `docs/DECISIONS_AND_FAILURES.md`
describes them instead. Between `f19e5d8` and the last commit, a few
packaging commits in the working repo were squashed: they added the
transcripts before the take-home brief in them was redacted, and that
version should not be republished.

The author e-mail on the first two commits was changed to my GitHub address
before publishing. That changes every hash, so the transcripts (recorded
before the change) cite the hashes in the second column; contents, messages
and dates are unchanged.

An earlier push of this repository had the initial commit split into
per-subsystem commits for readability. It was replaced by the real history
above, so any other commit hashes seen for this project refer to that
version.

## Build, grade and deliverables

Build and grading steps: [`docs/REPRODUCIBILITY.md`](docs/REPRODUCIBILITY.md).
Where each requested deliverable is: the table in [`README.md`](README.md).

## Tools Used

- **2026-11-07 pass**: Claude Code 2.1.317 with Claude Opus 5.5 (`claude-opus-5-5`), single agent, no subagents — see `agent/claude-code/`
- **Initial implementation**: Google Antigravity, with Claude Opus 4.6 / Gemini 3.8 Flash for implementation, debugging and review (from memory; no logs from that phase exist)
- **Reference**: GBATEK (Martin Korth), ARM7TDMI datasheet, gba-eval oracle tool
- **Testing**: gba-eval grader + oracle for differential frame comparison against Mesen2

## Disclosures

- **Prior experience with GBA Eval**: I had read about gba-eval before
  this take-home, but had not used it, had not looked at the published
  attempts (`mechanize-work/gba-eval-attempts`), and had not built or
  worked on a GBA or any other emulator. The Claude Code transcripts show
  the agent read only the gba-eval README mention of `gba-eval-attempts`;
  it did not open those repositories.
- **`src/bus/waitstates.rs` and Mesen2** *(corrected 2026-11-09, see
  `CORRECTIONS.md`)*: the version reviewed (tag `reviewed-2026-11-08`) was
  written by the Antigravity agent in the initial phase. Its header said
  "mirroring Mesen2's GbaWaitStates", and its table construction and
  address indexing closely match Mesen2's. No log exists of what that
  agent consulted, so it cannot be shown to be independent; it should be
  treated as adapted from Mesen2. The earlier statement here ("only the
  idea and name were borrowed") was not supported and has been withdrawn.
  The module has been replaced by one written from GBATEK's timing tables
  with a different structure (direct per-access computation, no lookup
  table); an exhaustive check over all 65,536 WAITCNT values showed the
  same cycle costs as before.
- **Outside sources read during the 2026-11-07 pass** (all visible in the
  transcripts): GBATEK and similar hardware documentation; the C sources of
  the NanoBoyAdvance *hardware test ROMs* (`nba-emu/hw-test`, timer tests)
  that the corpus itself uses, to learn what each test checks; web search
  results describing emulators. No emulator source code was copied.
- **gba-eval's reference wrapper and harness** *(corrected 2026-11-09)*:
  the agent read parts of gba-eval's own source, not just its outputs:
  `reference/mesen_step.cpp` (gba-eval's C++ wrapper that builds Mesen2
  into `mesen.wasm`: initialisation, boot-screen skip, colour filters off,
  ROM loading, reset and frame stepping; transcript `02b30b4e`, lines
  828 and 831), the harness files `grader/src/wasm_candidate.rs` and
  `lockstep/src/mesen.rs` (which give the reference's boot-frame count
  of 6), and the export names in `mesen.wasm` (`strings`). These informed
  `emu_boot_frames() = 6`, the boot/forced-blank state and how the
  lockstep tool drives the reference. The reviewed version said the
  reference's "source was not read"; that was inaccurate. What is
  accurate: no Mesen2 emulator-core source (the `Core/GBA` code) was read
  in the Claude Code sessions, and `tools/lockstep` uses `mesen.wasm`
  only through its exported functions.

## Omissions, unavailable items and external dependencies

| Item | Status |
|---|---|
| Raw Antigravity conversation exports (initial implementation) | **Not included.** `agent/antigravity/conversations/` holds condensed write-ups and summaries, not exports. |
| Take-home brief pasted into my prompts | Shown as a `[redacted: take-home brief …]` marker in the transcripts; a paraphrased version of the prompts is in `agent/claude-code/PROMPT_PLAYBOOK.md`. |
| Later Claude Code sessions about packaging only (git push help, transcript formatting, repo publishing; 2026-11-07 16:17 → 2026-11-08) | Not exported: they contain no emulator work. |
| Model usage / cost for the Antigravity phase | Not available. |
| Claude Code usage | Token counts per session in `agent/transcripts/claude-code/usage.json`; dollar cost not recorded. |
| Grader screenshots/WAVs (`*.cand.png`, `*.ref.wav`, ...) | Not committed (size). Regenerate with `tools/grade.sh`. |
| gba-eval harness, corpus ROMs, reference caches, `reference/mesen.wasm` | Not vendored. Public: `github.com/mechanize-work/gba-eval` (needs Git LFS). |
| Network services | None needed to build or run. Grading needs the gba-eval checkout; Docker is optional (native path in `docs/REPRODUCIBILITY.md` §5). |
| Credentials and private information | None in the repo. Personal and unrelated details in the transcripts are replaced by `[redacted-…]` markers. |
