# Decision Log & Intermediate Failed Attempts

This document tracks significant design choices, intermediate hypotheses, failed implementations, and how each issue was diagnosed and resolved during development.

---

## 1. Architectural Decisions

### Decision 1: Pure State Encapsulation vs Global Statics
- **Initial Thought**: Use direct `static mut` arrays for ROM, RAM, VRAM, and CPU registers to simplify pointer returns to the C-ABI.
- **Problem**: Caused state pollution across consecutive `emu_reset()` and `emu_load_rom()` cycles, violating test harness determinism.
- **Solution**: Encapsulated the entire system state into an owned struct `Gba` (`src/gba.rs`). The C-ABI in `src/lib.rs` keeps one `static mut GBA: Option<Box<Gba>>`, created in `emu_init()`; `emu_reset()` and `emu_load_rom()` reset that struct in place, so no state lives outside it. Large buffers (`rom_buffer`, `audio_buffer`) use heap allocation (`Vec` and `Box`) to avoid stack overflows and static bloat.

### Decision 2: CPU Pipeline Simulation
- **Initial Thought**: Single-cycle instruction fetch and immediate execution without modeling the ARM7TDMI 3-stage fetch-decode-execute pipeline.
- **Problem**: Instructions reading `R15` (PC) obtained incorrect values. In ARM mode, PC is 2 instructions ahead (`+8` bytes); in Thumb mode, it is 2 instructions ahead (`+4` bytes).
- **Solution**: Implemented a 2-instruction pipeline buffer `pipe: [u32; 2]` in `src/cpu/mod.rs`. `flush_pipeline` pre-fills the pipeline on branches/interrupts, and PC increments accurately reflect 3-stage execution.

---

## 2. Failed Attempts & Resolved Bugs

### Bug 1: ARM LDM/STM Block Data Transfer Base Address Drift
- **Symptom**: `aw-arm-ldm-stm` failed completely in early grading runs.
- **Hypothesis**: The register list loop was executing in the wrong order.
- **Root Cause**: For Decrement After (DA) and Decrement Before (DB) addressing modes, the base calculation was subtracting after each store rather than calculating the target memory range upfront, leaving the base register with an improper final offset when writeback was enabled.
- **Fix**: Refactored `exec_block_transfer` in `src/cpu/arm.rs` to compute the low and high memory bounds upfront based on the number of set bits in `r_list`, writing back the correctly computed base pointer.

### Bug 2: Thumb Format 8 (Load/Store Sign-Extended Byte/Halfword) Operand Inversion
- **Symptom**: Thumb memory access tests failed on halfword and sign-extended loads (`aw-thumb-ldr-str`).
- **Hypothesis**: Address offset calculation was wrong.
- **Root Cause**: In opcode bitfield decoding `0x5000 | (h_bit << 11) | (s_bit << 10)`, the conditions for `LDSB` vs `LDRH` had inverted branch checks.
- **Fix**: Re-aligned Thumb opcode masks with the ARM7TDMI reference manual, correcting `LDSB`, `LDRH`, and `LDSH` decoding.

### Bug 3: 8-bit / 16-bit MMIO Bus Routing & Open-Bus Reads
- **Symptom**: `mgba-io-read` returned mismatched values on 8-bit reads from 16-bit registers.
- **Hypothesis**: MMIO registers needed dedicated 8-bit read matches for all addresses.
- **Root Cause**: GBA IO registers are physically connected to a 16-bit bus. Reading an odd byte from a 16-bit register returns bits 8..15 of that register, not zero or unmapped memory. Unmapped registers also return the internal bus open-bus value rather than 0x00.
- **Fix**: Refactored `read_io8` in `src/bus/mod.rs` to delegate to `read_io16(addr & !1)` and extract the requested byte. Added proper open-bus masking for write-only registers (e.g. DMA controls, sound FIFOs).

### Bug 4: Register-Specified Shifts with R15 (PC) Prefetch Delta
- **Symptom**: Arithmetic and logical shifts using a register operand (`LSL Rs`, `ASR Rs`) produced incorrect results when `Rs = R15` or `Rm = R15`.
- **Hypothesis**: ALU shift logic was mishandling shift counts ≥ 32.
- **Root Cause**: On ARM7TDMI, when a register-specified shift is used, an internal cycle is spent reading the shift amount. During this extra cycle, the PC has advanced an extra instruction (+12 instead of +8).
- **Fix**: Updated `src/cpu/arm.rs` so that register-specified shifts treat `R15` operand reads as PC + 12. *(An earlier version of this file cited commit `5fd7684`; no such commit exists. The initial work was committed as a single commit (`f52c41d`, `228706c` in the transcripts); see `HANDOFF.md` "Repository history".)*

---

## 3. 2026-11-07 pass (Claude Code): approach changes and failed attempts

Full detail is in the transcript (`agent/transcripts/claude-code/02b30b4e-….md`)
and every graded build is in `results/`. Main changes of approach:

- **From reading code to measuring against the reference.** Early fixes were
  guided by grader scores alone. After frame 0 of every replay differed, the
  agent built an oracle frame-diff loop, and later `tools/lockstep`, which
  steps `reference/mesen.wasm` beside our core and stops at the first PC or
  cycle divergence. Most timing rules in `docs/TIMING_MODEL.md` came from it.
- **Regression: v6-full (0.284 overall, below the 0.379 baseline).** Adding
  waitstate-accurate memory timing fixed procedural tests (0.43 → 0.59) but
  replay dropped 0.46 → 0.19, because frames were still cut at line 0 and
  boot frames were reported as 0, so every replay was phase-shifted against
  the reference. Fixing frame/boot alignment produced v8 (0.566).
- **Suspicious identical scores (v10-full).** v10 scored the same as v8 to
  four decimals despite large timing changes. Checking showed the binaries
  differed, and the real cause was input latency: keys took effect one frame
  early. Latching keys at the frame boundary (v11) took goodboy from ~840 to
  48 differing frames of 900.
- **Subset runs that scored low by design.** `v3-proc`, `v4-t`, `v5-t`,
  `v7-dma`, `v9-proc`, `v10-io` graded only a few testcases, so their
  "overall" is not comparable to full runs. `v5-games` was stopped before it
  wrote a summary.
- **Left unfinished:** mgba-dma (1248/1256 subtests), timer-reload,
  mgba-io-read, VRAM contention. See `HANDOFF.md`.
