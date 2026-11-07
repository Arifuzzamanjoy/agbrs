#!/usr/bin/env bash
# Grade a wasm on a subset of gba-eval testcases (fast iteration loop).
# Usage: tools/quick-grade.sh <wasm> <run-name> <testcase-id>...
# Builds a filtered corpus view (symlinks + filtered testcases.json)
# so the real grader and reference caches are used unchanged.
set -euo pipefail
WASM="$(realpath "$1")"; NAME="$2"; shift 2
HERE="$(dirname "$(realpath "$0")")"
EVAL="$(realpath "${GBA_EVAL:-$HERE/../../gba-eval}")"
SUB="$EVAL/results/.subset-$NAME"
rm -rf "$SUB"; mkdir -p "$SUB"
for f in roms replays reference-cache grader.yaml; do ln -s "$EVAL/corpus/$f" "$SUB/$f"; done
python3 - "$EVAL/corpus/testcases.json" "$SUB/testcases.json" "$@" <<'PY'
import json, sys
src, dst, ids = sys.argv[1], sys.argv[2], set(sys.argv[3:])
d = json.load(open(src))
d["testcases"] = [t for t in d["testcases"] if t["id"] in ids]
json.dump(d, open(dst, "w"), indent=1)
PY
OUT="$EVAL/results/$NAME"; rm -rf "$OUT"; mkdir -p "$OUT"
cd "$EVAL"
RAYON_NUM_THREADS="${THREADS:-1}" ./target/release/grader "$WASM" "$SUB" "$OUT" > "$OUT/grader.log" 2>&1 || { tail -5 "$OUT/grader.log"; exit 1; }
python3 "$HERE/scores.py" "$OUT"
