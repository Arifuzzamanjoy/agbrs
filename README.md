# GBA Emulator (Rust / WebAssembly)

An original Game Boy Advance (GBA) emulator written in Rust from scratch, compiled to WebAssembly (`wasm32-unknown-unknown`) for the [Mechanize GBA Eval](https://github.com/mechanize-work/gba-eval) benchmark.

---

## 📋 Evaluation & Handoff Deliverables

All deliverables requested for the engineering trial are documented and organized in this repository:

| Deliverable | Location | Description |
|---|---|---|
| **Executive Handoff** | [HANDOFF.md](HANDOFF.md) | Architectural overview, benchmark results (0.3789), and limitations |
| **Compiled Artifact** | [dist/gba_emu.wasm](dist/gba_emu.wasm) | Pre-built, verified release WebAssembly binary (85 KB) |
| **Reproduction Guide** | [docs/REPRODUCIBILITY.md](docs/REPRODUCIBILITY.md) | Step-by-step instructions to rebuild, run unit tests, and grade against Mesen2 |
| **Decisions & Failures** | [docs/DECISIONS_AND_FAILURES.md](docs/DECISIONS_AND_FAILURES.md) | Log of intermediate design pivots, failed attempts, and bug diagnoses |
| **Workflow Metadata** | [docs/WORKFLOW_METADATA.md](docs/WORKFLOW_METADATA.md) | Toolchain versions (Rust 1.99.1), human time spent (13.5 hrs), and model usage |
| **AI Agent Configs** | [agent/antigravity/](agent/antigravity/) | Prompts, subagent configurations, and filtered engineering conversations |
| **Benchmark Output** | [results/summary.json](results/summary.json) | Full per-testcase grading output across all 27 testcases |

---

## Features & Subsystems

- **CPU (ARM7TDMI)**: Full implementation of ARM (32-bit) and Thumb (16-bit) instruction sets, barrel shifter, condition evaluation, banked registers, and 3-stage execution pipeline.
- **Memory Bus**: Complete memory mapping for BIOS, EWRAM, IWRAM, I/O registers, Palette RAM, VRAM, OAM, Game Pak ROM, and SRAM with 16-bit bus alignment and open-bus reads.
- **PPU (Graphics)**: 240x160 scanline renderer supporting Modes 0–5 (tiled and bitmap graphics), affine background layers, OAM sprite rendering (affine and regular), windowing, and alpha blending.
- **DMA Controller**: 4 DMA channels with configurable priorities, repeat modes, and audio FIFO streaming triggers.
- **Timers & Interrupts**: 4 hardware timers with cascading prescalers and IRQ generation.
- **APU (Audio)**: PSG channels (square waves, noise, wave RAM) and DirectSound FIFO channels A & B sampled at 32,768 Hz stereo.

---

## Quickstart: Building & Testing

### 1. Build WebAssembly Binary
```bash
rustup target add wasm32-unknown-unknown
cargo build --release --lib --target wasm32-unknown-unknown
```
Output: `target/wasm32-unknown-unknown/release/gba_emu.wasm` (or use pre-built `dist/gba_emu.wasm`).

### 2. Run Tests
```bash
cargo test
```

### 3. Grade with GBA Eval
```bash
./quickstart/grade.sh dist/gba_emu.wasm my-run
```
See [docs/REPRODUCIBILITY.md](docs/REPRODUCIBILITY.md) for full grading details.

---

## Standard C-ABI Exports

The WebAssembly core exports the standard 10 C-ABI functions:
- `emu_init() -> i32`
- `emu_rom_buffer() -> *mut u8`
- `emu_load_rom(len: i32) -> i32`
- `emu_reset() -> i32`
- `emu_set_keys(keys: u32)`
- `emu_run_frame()`
- `emu_framebuffer() -> *const u32`
- `emu_audio_buffer() -> *const i16`
- `emu_audio_samples() -> i32`
- `emu_audio_rate() -> i32`
