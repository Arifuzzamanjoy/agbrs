# Handoff: GBA Emulator (agbrs)

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
├── lib.rs          # WASM ABI exports (10 C-ABI functions)
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
│   └── waitstates.rs # ROM waitstate configuration
├── ppu/
│   └── mod.rs      # Scanline renderer, all BG modes, sprites
├── dma/
│   └── mod.rs      # 4-channel DMA controller
├── timer/
│   └── mod.rs      # 4 cascading timers
└── apu/
    └── mod.rs      # PSG + DirectSound FIFO audio
```

Total: ~4,500 lines of Rust.

## 2026-11-07 pass: timing, audio and differential testing

This pass was done with Claude Code (model: Claude Opus 5.5) driven by the
repository owner. Goal: check the repo against the take-home requirements,
find why the score was low, and raise it with validated changes.

### Score (gba-eval grader, native build, reference = bundled Mesen2)

| Run | Overall | Replay (60%) | Procedural (20%) | Audio (20%) |
|---|---|---|---|---|
| Baseline (`dist/` wasm at commit e549c6a) | 0.3789 | 0.4606 | 0.4261 | 0.0865 |
| Intermediate (v8: APU rewrite, access timing, frame/boot alignment) | 0.5655 | 0.5537 | 0.5861 | 0.5802 |
| v11 (+ lockstep-derived IRQ/DMA/prefetch rules, input latching) | 0.6775 | 0.7137 | 0.5861 | 0.6605 |
| **Final** (`dist/gba_emu.wasm`, sha256 b2705a31…) | **0.6900** | **0.7181** | **0.5861** | **0.7096** |

Per-testcase tables: `results/` (baseline, v8, final), produced by
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

## Results before this pass (original handoff)

Overall score: **0.3789** (Replay: 0.4606, Procedural: 0.4261, Audio: 0.0865).
See `results/summary.json` for full per-testcase breakdowns.

### Tests passing at 1.0 (perfect match vs Mesen2):
- aw-arm-alu, aw-arm-ldr-str, aw-arm-ldm-stm
- aw-thumb-alu, aw-thumb-ldr-str, aw-thumb-ldm-stm
- memory, dma-priority, mgba-carry, mgba-shifter

### Games running:
- BulletGBA: 0.9934 video SSIM / 0.5645 audio
- Goodboy Advance: 0.9190 video SSIM
- tonc-snd1: 0.9833 video
- Collie Defense: 0.7872 video
- PIU GBA: 0.5011 video
- Waimanu, Spout: running with partial sync

## Known Limitations

1. **Score below baseline** — the 0.53 baseline has tighter edge-case handling on timers and cycle alignment
2. **Timer tests failing** — timer reload/start-stop tests need more precise cycle-level timing of when timer values latch
3. **mgba-multiply-long, mgba-dma** — each has minor flag and open-bus edge cases
4. **ppu-vram-mirror** — VRAM mirroring in the upper 32KB region needs fine-tuning for Mode 3
5. **Replay sync drift** — Celeste, Varooom 3D, and Heartwrench desync after early frames, likely due to subtle interrupt latency or HBlank DMA timing differences
6. **Audio quality is low** — PSG envelope and sweep aren't fully accurate, FIFO sample timing has slight drift

## What I'd Work On Next

In priority order (by expected score impact):

1. **Fix timer latch timing** — timer-reload and timer-start-stop are 0.0; cycle-accurate latching will boost procedural score significantly
2. **HBlank DMA timing** — many gameplay replays desync because DMA doesn't fire at the exact cycle boundaries expected by games
3. **Interrupt delivery latency** — ppu-vram-mirror and game input loops depend on precise IRQ servicing delay
4. **Audio envelope accuracy** — PSG channel sweep/envelope timing affects audio scores across all 9 audio testcases
5. **Prefetch buffer emulation** — some commercial/homebrew titles rely on ROM waitstate prefetch timing

## Build & Grade

See [`docs/REPRODUCIBILITY.md`](docs/REPRODUCIBILITY.md) for complete instructions.

```bash
# build
rustup target add wasm32-unknown-unknown
cargo build --release --lib --target wasm32-unknown-unknown

# the wasm output
ls target/wasm32-unknown-unknown/release/gba_emu.wasm

# grade (requires gba-eval repo)
cd /path/to/gba-eval
./quickstart/grade.sh /path/to/agbrs/dist/gba_emu.wasm my-run
cat results/my-run/summary.json
```

## AI Orchestration & Process Deliverables

Detailed logs, prompts, and failure records requested for the evaluation handoff:
- **Reproduction Guide**: [`docs/REPRODUCIBILITY.md`](docs/REPRODUCIBILITY.md)
- **Decisions & Failed Attempts**: [`docs/DECISIONS_AND_FAILURES.md`](docs/DECISIONS_AND_FAILURES.md)
- **Workflow & Environment Metadata**: [`docs/WORKFLOW_METADATA.md`](docs/WORKFLOW_METADATA.md)
- **Agent Prompts & Transcripts**: [`agent/antigravity/`](agent/antigravity/)
- **Compiled WASM Artifact**: [`dist/gba_emu.wasm`](dist/gba_emu.wasm)

## Tools Used

- **Editor & Orchestration**: Google DeepMind Antigravity framework
- **AI Models**: Claude 3.5 Sonnet / Gemini 1.5 Pro / Claude Opus for implementation guidance, debugging, and code review
- **Reference**: GBATEK (Martin Korth), ARM7TDMI datasheet, gba-eval oracle tool
- **Testing**: gba-eval grader + oracle for differential frame comparison against Mesen2
