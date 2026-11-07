# Decision Log & Intermediate Failed Attempts

This document tracks significant design choices, intermediate hypotheses, failed implementations, and how each issue was diagnosed and resolved during development.

---

## 1. Architectural Decisions

### Decision 1: Pure State Encapsulation vs Global Statics
- **Initial Thought**: Use direct `static mut` arrays for ROM, RAM, VRAM, and CPU registers to simplify pointer returns to the C-ABI.
- **Problem**: Caused state pollution across consecutive `emu_reset()` and `emu_load_rom()` cycles, violating test harness determinism.
- **Solution**: Encapsulated the entire system state into an owned struct `Gba` (`src/gba.rs`). The C-ABI in `src/lib.rs` holds a single `Option<Box<Gba>>` that is safely recreated on reset. Large buffers (`rom_buffer`, `audio_buffer`) use heap allocation (`Vec` and `Box`) to avoid stack overflows and static bloat.

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
- **Fix**: Updated `src/cpu/arm.rs` so that register-specified shifts treat `R15` operand reads as PC + 12 (Commit `5fd7684`).

---

## 3. Preserved Known Limitations (Active Backlog)

1. **Timer Latching Edge Cases**: The reload and start/stop latch latency currently does not match sub-cycle timing, causing `timer-reload` and `timer-start-stop` to fail.
2. **HBlank DMA Cycle Alignment**: HBlank DMA fires at scanline boundary instead of exact cycle 1006, causing replay desync in games that perform palette swaps mid-scanline.
3. **APU PSG Sweep & Envelope Accuracy**: Sound channel envelopes currently step on simplified intervals rather than cycle-exact 64Hz/256Hz dividers, resulting in low audio log-mel scores.

