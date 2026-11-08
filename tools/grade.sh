#!/usr/bin/env bash
# Grade a wasm against the gba-eval corpus with a native grader build.
# Usage: tools/grade.sh <wasm> <run-name> [threads]
# Env: GBA_EVAL (path to gba-eval checkout, default ../gba-eval)
# Runs with limited rayon threads because each worker holds two emulator
# instances + reference caches; on an 8 GB host 3+ threads were OOM-killed,
# and 2 can be too when other processes use memory (1 is the safe choice).
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
WASM="$(realpath "$1")"; NAME="$2"; THREADS="${3:-2}"
EVAL="${GBA_EVAL:-$HERE/../../gba-eval}"
cd "$EVAL"
[ -x target/release/grader ] || cargo build -p grader --release
OUT="results/$NAME"; rm -rf "$OUT"; mkdir -p "$OUT"
RAYON_NUM_THREADS="$THREADS" ./target/release/grader "$WASM" corpus/ "$OUT" > "$OUT/grader.log" 2>&1
python3 "$HERE/summarize.py" "$OUT/summary.json"
