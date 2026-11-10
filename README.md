# GBA Emulator (Rust / WebAssembly)

An original Game Boy Advance (GBA) emulator written in Rust from scratch, compiled to WebAssembly (`wasm32-unknown-unknown`) for the [Mechanize GBA Eval](https://github.com/mechanize-work/gba-eval) benchmark.

---

## Handoff deliverables

Where each requested deliverable is; anything missing or redacted is listed in `HANDOFF.md` ("Omissions").

> **Corrections after review (2026-11-09):** see [CORRECTIONS.md](CORRECTIONS.md).
> The reviewed version is tagged `reviewed-2026-11-08`; all corrections are
> separate later commits.

| Deliverable | Location | Description |
|---|---|---|
| **Handoff** | [HANDOFF.md](HANDOFF.md) | Architectural overview, benchmark results (baseline 0.3789 → see the 2026-11-07 pass), and limitations |
| **Compiled Artifact** | [dist/gba_emu.wasm](dist/gba_emu.wasm) | Pre-built release WebAssembly binary (129 KB, sha256 `c4f53a24…`, corrected build of 2026-11-09; reviewed build was `b2705a31…`; rebuilds byte-identically, see `results/logs/build-wasm.txt`) |
| **Reproduction Guide** | [docs/REPRODUCIBILITY.md](docs/REPRODUCIBILITY.md) | Step-by-step instructions to rebuild, run unit tests, and grade against Mesen2 |
| **Decisions & Failures** | [docs/DECISIONS_AND_FAILURES.md](docs/DECISIONS_AND_FAILURES.md) | Log of intermediate design pivots, failed attempts, and bug diagnoses |
| **Workflow Metadata** | [docs/WORKFLOW_METADATA.md](docs/WORKFLOW_METADATA.md) | Toolchain versions (Rust 1.99.1), estimated human time, and model usage |
| **AI Agent Configs** | [agent/claude-code/](agent/claude-code/README.md), [agent/antigravity/](agent/antigravity/) | Claude Code setup, model usage and sessions (2026-11-07 pass); Antigravity prompts and condensed conversation summaries (initial implementation) |
| **AI Transcripts** | [agent/transcripts/claude-code/](agent/transcripts/claude-code/) | Claude Code session exports (`.jsonl`) with readable `.md` renderings; personal details redacted |
| **Prompt playbook** | [agent/claude-code/PROMPT_PLAYBOOK.md](agent/claude-code/PROMPT_PLAYBOOK.md) | The prompts restated as a reusable workflow (paraphrased; exact wording in the transcripts) |
| **Benchmark Output** | [results/](results/) | `summary.json`, `grader.log` and per-test `scores.txt` for the baseline and final runs, plus every intermediate/regressed run; build and test logs in `results/logs/` |
| **Timing model** | [docs/TIMING_MODEL.md](docs/TIMING_MODEL.md) | Cycle-timing rules derived by instruction-level lockstep against the reference, with evidence and known gaps |
| **Dev tools** | [tools/](tools/), [examples/run.rs](examples/run.rs) | Native grading scripts, lockstep tracer vs Mesen2, frame/audio diff tools, native debug runner |

---

## Features & Subsystems

Current score 0.69 on gba-eval; known gaps are listed in `HANDOFF.md`.

- **CPU (ARM7TDMI)**: ARM (32-bit) and Thumb (16-bit) instruction sets, barrel shifter, condition evaluation, banked registers, and 3-stage execution pipeline.
- **Memory Bus**: Memory map for BIOS, EWRAM, IWRAM, I/O registers, Palette RAM, VRAM, OAM, Game Pak ROM, and SRAM with 16-bit bus alignment and open-bus reads.
- **PPU (Graphics)**: 240x160 scanline renderer supporting Modes 0–5 (tiled and bitmap graphics), affine background layers, OAM sprite rendering (affine and regular), windowing, and alpha blending.
- **DMA Controller**: 4 DMA channels with configurable priorities, repeat modes, and audio FIFO streaming triggers.
- **Timers & Interrupts**: 4 hardware timers with cascading prescalers and IRQ generation.
- **APU (Audio)**: PSG channels (square waves, noise, wave RAM) and DirectSound FIFO channels A & B; stereo output at 32,768 Hz (65,536 Hz when the game selects it via SOUNDBIAS).

---

## Quickstart: Building & Testing

### 1. Build WebAssembly Binary
```bash
cargo build --release --lib --target wasm32-unknown-unknown
```
Output: `target/wasm32-unknown-unknown/release/gba_emu.wasm` (or use pre-built `dist/gba_emu.wasm`).

### 2. Run Tests
```bash
cargo test
```

### 3. Grade with GBA Eval
```bash
# from a gba-eval checkout (Docker path)
./quickstart/grade.sh /path/to/agbrs/dist/gba_emu.wasm my-run
# or natively from this repo, with gba-eval cloned next to it
tools/grade.sh dist/gba_emu.wasm my-run 1
```
Native grading needs bash, `git lfs pull` inside `../gba-eval`, a C toolchain
(`build-essential` on Ubuntu) and about 8 GB of RAM.
See [docs/REPRODUCIBILITY.md](docs/REPRODUCIBILITY.md) for full grading details.

---

## C-ABI Exports

The 10 functions required by gba-eval's `spec/ABI.md`:
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

Plus one optional export the grader reads, `emu_boot_frames() -> i32`
(returns 6 so frame N lines up with the reference's frame N; see
`src/lib.rs`), and three read-only debug exports used by the native tools:
`emu_debug_pc`, `emu_debug_dispcnt`, `emu_debug_vcount`.
