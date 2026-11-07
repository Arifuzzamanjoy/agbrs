#!/usr/bin/env python3
"""Print a compact per-testcase table from a gba-eval summary.json."""
import json, sys
s = json.load(open(sys.argv[1]))
def walk(o, p=""):
    if isinstance(o, dict):
        for k, v in o.items(): yield from walk(v, f"{p}.{k}" if p else k)
    else: yield p, o
for k, v in walk(s):
    if isinstance(v, (int, float)) and not isinstance(v, bool): print(f"{k:70s} {v:.4f}" if isinstance(v,float) else f"{k:70s} {v}")
