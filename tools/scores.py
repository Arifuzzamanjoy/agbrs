#!/usr/bin/env python3
"""Per-testcase graded scores from gba-eval summary.json, optionally vs a baseline run.
Usage: scores.py <results-dir> [<baseline-results-dir>]"""
import json, os, sys
def load(d):
    p = os.path.join(d, "summary.json")
    return json.load(open(p)) if os.path.exists(p) else {"per_testcase": {}}
cur = load(sys.argv[1]); base = load(sys.argv[2]) if len(sys.argv) > 2 else {"per_testcase": {}}
f = lambda x: "   -  " if x is None else f"{x:.4f}"
for k, v in sorted(cur["per_testcase"].items(), key=lambda kv: (kv[1]["section"], kv[0])):
    b = base["per_testcase"].get(k, {})
    print(f"{v['section'][:5]:5s} {k:26s} video {f(v.get('video_score'))} (was {f(b.get('video_score'))})"
          f"  audio {f(v.get('audio_score'))} (was {f(b.get('audio_score'))})")
for name, sec in cur.get("sections", {}).items():
    bs = base.get("sections", {}).get(name, {}).get("score")
    print(f"section {name:10s} {f(sec['score'])} (was {f(bs)})")
if "overall" in cur: print(f"overall {cur['overall']:.4f} (was {f(base.get('overall'))})")
