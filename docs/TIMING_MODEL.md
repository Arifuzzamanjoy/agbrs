# Timing model and how it was derived

The grader compares every frame (and every audio frame) against Mesen2.
Games are deterministic functions of *when* things happen, so frame-exact
output needs the CPU, DMA, IRQ and PPU timing to agree with the reference
at the cycle level. This document records the model the core implements
and the evidence for each rule.

## Method: instruction-level lockstep against the reference

`tools/lockstep/` (crate `gba-lockstep`) loads `reference/mesen.wasm`
under wasmtime and uses its debug exports (`mesen_exec_one`, `mesen_pc`,
`mesen_master_clock`) to run Mesen2 one instruction at a time next to our
core (linked natively). After every instruction it compares r15 and the
absolute cycle counter, and reports:

- the first PC divergence, with the last 14 steps of context
  (cycles per step on both sides, CPSR, IME/IE/IF, scanline.dot);
- every instruction whose cycle cost differs, with our per-access log
  (address, width, kind, cost);
- `--irq-report`: for every IRQ Mesen takes, the line-rise time and the
  boundaries it skipped / took;
- `--frames-probe N`: per-frame end clock and audio sample counts.

This is the standard "step-and-compare against a golden model" approach
used for CPU verification (see e.g. arXiv:2510.10225, arXiv:2105.14273),
applied to a software reference. It turned vague symptoms ("goodboy fades
a frame late") into exact, testable facts. Every rule below was adopted
only when it removed a divergence without introducing another.

Result at the time of writing: `goodboy-advance` runs **8,000,000+
instructions in exact PC lockstep** with Mesen2 (cycle drift bounded at a
few cycles, which is IRQ-attribution noise). The other games run 230k-490k
instructions (the first 1-2 frames after boot) before an IRQ-boundary edge
case diverges; see "Known gaps".

## Rules (all in `src/cpu/mod.rs`, `src/bus/mod.rs`, `src/gba.rs`, `src/ppu/mod.rs`)

### Power-on and frame boundary
- Boot skip like the reference: the first ROM opcode executes at clock 21
  with BIOS-initialised stacks (`Cpu::reset_skip_bios`).
- PPU starts at line 225 (`RESET_VCOUNT`) so that line 0 begins at clock
  3696 and the first VBlank at ~200816, matching the reference's
  frame-end clocks to within instruction granularity.
- `run_frame` ends when the PPU enters VBlank (line 160), which is where
  the reference ends its frames (measured: frame-end phase 200815-200827).
- `emu_boot_frames()` returns 6, the value the reference wasm exports, so
  the grader burns the same six power-on frames on both sides.

### Input and audio stream alignment (frame-level, not lockstep)
- `emu_set_keys` is latched: keys become visible at the end of the next
  `run_frame` (the VBlank boundary). Applying them immediately made every
  input take effect one frame before the reference (frame-diff against
  the oracle: our frame N matched its frame N+1 after each key change).
  goodboy went from ~840 to 48 differing frames out of 900.
- Output sample rate follows SOUNDBIAS bits 14-15 (32/65/131/262 kHz with
  9/8/7/6-bit resolution). The reference emits samples at that rate while
  still reporting 32768, and the grader compares the streams sample-for-
  sample, so games that select 65536 Hz (piugba, chip-advance) scored
  ~0.06 audio until this was matched (piugba → 0.70).

### Memory access timing
- Every CPU/DMA access is charged `1 + waitstates` from GBATEK;
  Game Pak N/S distinction by address continuity (128 KiB boundaries
  force N).
- Internal (I) cycles happen **before** the instruction's opcode fetch,
  and the fetch after an I cycle is non-sequential. (Evidence: Thumb
  `LSL Rd,Rs` and `MUL` cost 2 more in the reference than "fetch then I";
  `LDR`'s following instruction is sequential.)
- A pipeline flush costs one extra cycle charged as a **sequential access
  at the branch destination in the new state's width**, followed by N+S
  refill fetches (e.g. BX into Thumb ROM = 3+5+3, into IWRAM = 1+1+1).
- Game Pak prefetch buffer (WAITCNT bit 14): 8 halfwords, filled while the
  cartridge bus is idle (including the 1-cycle CPU fetch that hits the
  buffer); sequential code fetches hitting it cost 1 cycle, otherwise the
  CPU waits for the in-flight halfword. A Game Pak *data* access that
  interrupts an in-flight prefetch costs 1 extra cycle. A pipeline flush
  discards the buffer (the refill is a normal N fetch even if the target
  was buffered). (`--branch-stats` in the tracer was used to test, and
  reject, an extra flush penalty: it overcharged 63 branches for every 3
  it fixed.)

### DMA
- Source and destination are independent sequential streams:
  2N + 2(n-1)S, plus `DMA_OVERHEAD_CYCLES` = 4 (2-cycle start delay + 2I).
  (Evidence: a 10,054-unit ROM→VRAM transfer matches to the cycle.)
- Game Pak sources always increment; addresses masked/aligned per channel;
  unreadable sources (BIOS/unmapped) yield the DMA open-bus latch.
- SAD/DAD/CNT_L read back as 0. (Evidence: piugba read-modify-writes
  DMA2CNT as a 32-bit word; with open-bus reads it wrote the wrong count
  and its sound DMA ran 32k cycles short.)

### IRQ latency
- The IRQ line is timestamped when (IME && IE & IF) rises: PPU events use
  the exact dot, timers the exact overflow cycle, I/O writes the access
  time.
- The CPU takes the IRQ at the first instruction boundary at least
  `IRQ_LATENCY_CYCLES` = 9 cycles after the rise. (Evidence: VBlank risen
  1 cycle before a boundary → not taken, then taken 12 cycles after;
  risen 9 cycles before → taken; IME written 8 cycles before → not taken.)
- The HBlank IRQ line rises `HBLANK_IRQ_LAG` = 8 cycles after the HBlank
  flag (dot 1006); the flag and HBlank DMA stay at 1006.
- After a DMA, the CPU executes `DMA_IRQ_HOLD_INSNS` = 2 instructions
  before it can take an IRQ.
- HALT engages immediately after the HALTCNT write. On wake-up the CPU
  idles `HALT_EXIT_CYCLES` = 3, executes `HALT_WAKE_INSNS` = 2
  instructions, then enters the handler (the cycle latency is waived).

### PPU stepping
- The PPU is advanced in chunks that end on its events, so a long step
  (a 40k-cycle DMA) still produces every HBlank/VBlank in order. Before
  this fix it advanced one scanline per call and lagged by tens of lines
  after big DMAs.

## Known gaps (where lockstep still diverges)
- A few HBlank/VBlank IRQs are taken one instruction earlier or later than
  the 9-cycle rule predicts (chip-advance line 64, celeste frame 3,
  heartwrench, piugba). The sampling point probably depends on where in
  the instruction the CPU samples the line; `--irq-report` collects the
  data needed to fit it.
- Prefetch buffer: ~300 of ~20,000 ROM branches in bulletgba still differ
  by 1 cycle.
- PPU/CPU VRAM contention: 32-bit CPU accesses to VRAM on visible lines
  stall 1-6 extra cycles in the reference (~27% of accesses, ~0.8 cycles
  on average, mode 0, uniform across the line; 16-bit accesses never
  stall). Not modelled: it depends on the PPU's actual fetch schedule.
  `--vram-stats` produces the histogram.
- Timer reload-while-running edge cases (timer-reload test: 4/7 subtests).
- Multiply-long carry flag (mgba-multiply-long), the remaining 8/1256 DMA
  subtests, and I/O read-back of some registers (mgba-io-read).
