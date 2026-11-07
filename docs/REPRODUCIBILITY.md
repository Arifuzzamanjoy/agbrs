# Independent Reproduction & Grading Guide

This document provides step-by-step instructions to independently compile, verify, and grade the WebAssembly GBA emulator (`gba_emu`) against the Mechanize GBA Eval benchmark.

---

## 1. Prerequisites & Toolchain

The emulator core is written in pure Rust (edition 2021) with zero third-party crate dependencies to ensure deterministic compilation and zero host-level vulnerabilities.

- **Rust**: 1.80+ (Tested on `rustc 1.99.1`)
- **Cargo**: Corresponding version (Tested on `cargo 1.99.1`)
- **Wasm Target**: `wasm32-unknown-unknown`
- **Harness Environment**: Linux x86_64, Git LFS (for reference caches), Docker / Wasmtime

### Install Target
```bash
rustup target add wasm32-unknown-unknown
```

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

Expected output:
```
running 1 test
test test_smoke ... ok
test result: ok. 1 passed; 0 failed; 0 ignored
```

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

