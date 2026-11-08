# Independent Reproduction & Grading Guide

This document provides step-by-step instructions to independently compile, verify, and grade the WebAssembly GBA emulator (`gba_emu`) against the Mechanize GBA Eval benchmark.

---

## 1. Prerequisites & Toolchain

The emulator core is written in pure Rust (edition 2021) with no third-party crate dependencies, so the build needs nothing beyond the pinned toolchain.

- **Rust**: `rustc 1.99.1`, pinned by `rust-toolchain.toml` (rustup installs it and the wasm target automatically)
- **Cargo**: Corresponding version (Tested on `cargo 1.99.1`)
- **Wasm Target**: `wasm32-unknown-unknown`
- **Harness Environment**: Linux x86_64, Git LFS (for reference caches), Docker / Wasmtime

---

## 2. Compiling the WebAssembly Binary

From the repository root:

```bash
cargo build --release --lib --target wasm32-unknown-unknown
```

The compiled release artifact will be output to:
```
target/wasm32-unknown-unknown/release/gba_emu.wasm
```

A pre-built, verified release binary matching the benchmark run is also provided at:
```
dist/gba_emu.wasm
```

### Verification (SHA-256)
You can verify the candidate binary hash:
```bash
sha256sum target/wasm32-unknown-unknown/release/gba_emu.wasm
```

---

## 3. Running Unit & Integration Tests

The repository includes sanity and smoke test harnesses:

```bash
cargo test
```

Expected: 9 tests pass (1 smoke test in `tests/debug_test.rs`, 8 regression
tests in `tests/regressions.rs`, which pin down behaviours fixed in the
2026-11-07 pass: branch-to-same-PC, HALT, VRAM mirror hole, ROM open bus,
BIOS read protection, PSG pitch, timer start/stop latency). A recorded run
is in `results/logs/cargo-test.txt`.

---

## 4. Grading with Mechanize GBA Eval

To evaluate the emulator against the full 27-testcase reference corpus using `Mesen2`:

1. Clone and initialize the `gba-eval` evaluation harness repository:
```bash
git clone https://github.com/mechanize-work/gba-eval.git
cd gba-eval

# Initialize submodules and pull precomputed reference caches
git submodule update --init --recursive
git lfs install && git lfs pull
```

2. Run pre-flight check:
```bash
./quickstart/smoke-test.sh
```

3. Run the automated grading suite on the candidate binary:
```bash
./quickstart/grade.sh /path/to/agbrs/dist/gba_emu.wasm submission-run
```

4. Inspect the resulting metrics and screenshot diffs:
```bash
cat results/submission-run/summary.json
```

The output JSON includes the composite overall score, sub-scores for Replay, Procedural, and Audio, as well as per-testcase breakdowns.


---

## 5. Native grading without Docker (used for the 2026-11-07 timing/audio pass)

The numbers in `HANDOFF.md` for the timing/audio pass were produced with a
native grader build, not Docker. Steps on a fresh Ubuntu host:

```bash
# toolchain
curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --target wasm32-unknown-unknown
sudo apt-get install -y build-essential git-lfs     # grader needs a C linker

# eval harness, next to this repo (tools/ default to ../gba-eval)
git clone https://github.com/mechanize-work/gba-eval.git ../gba-eval
(cd ../gba-eval && git lfs install && git lfs pull && cargo build -p grader --release)

# build + grade
cargo build --release --lib --target wasm32-unknown-unknown
tools/grade.sh target/wasm32-unknown-unknown/release/gba_emu.wasm my-run 1
python3 tools/scores.py ../gba-eval/results/my-run            # per-test table
```

Memory: each grader worker holds two emulator instances plus reference
caches. On an 8 GB host, `RAYON_NUM_THREADS` > 2 was OOM-killed, and 2
can be too when other processes use memory, so 1 is the safe choice; the
`tools/grade.sh` third argument sets the thread count (1 thread ≈ 28 min
for all 27 testcases, 2 threads ≈ 15 min).

### Fast iteration on a subset
```bash
tools/quick-grade.sh <wasm> <run-name> timer-reload ppu-vram-mirror tonc-snd1
```
Builds a filtered corpus view (symlinks + filtered `testcases.json`) so the
real grader and reference caches are used unchanged.

### Debugging tools (not part of the wasm)
| Tool | Purpose |
|---|---|
| `cargo run --release --example run -- <rom> <frames> [--replay f] [--dump dir] [--wav f] [--trace-frame n] [--watch addr] [--peek addr] [--steps n]` | native runner: per-frame state, PPM frames, WAV, instruction trace, watchpoints |
| `tools/lockstep` (`CARGO_TARGET_DIR=../gba-eval/target cargo build --release` inside it) | instruction-level lockstep vs `reference/mesen.wasm`; see `docs/TIMING_MODEL.md` |
| `tools/firstdiff.py`, `tools/ppm2png.py` | first divergent frame between oracle and our PPM dumps; side-by-side PNG with diff mask |
| `tools/wavcmp.py`, `tools/audiolag.py` | level/pitch comparison and windowed waveform cross-correlation (needs numpy) |

Oracle frames/audio: `ORACLE_MESEN_WASM=../gba-eval/reference/mesen.wasm ../gba-eval/target/release/oracle run <rom> <frames> [--replay f] --dump-frames dir --dump-audio f.wav`.
Note: the oracle CLI does not drain audio during its 6 boot frames and the
reference's buffer caps at 2048 pairs, so its WAV is missing ~1086 samples
at the start compared with what the grader sees. Skip 2048 samples (not
6 frames' worth) when aligning against our `--wav` output.
