#!/usr/bin/env python3
"""First divergent frame between two PPM dumps (oracle vs ours).
Usage: firstdiff.py <ref_dir> <cand_dir> [max_report]"""
import os, sys
ref, cand = sys.argv[1], sys.argv[2]; nrep = int(sys.argv[3]) if len(sys.argv) > 3 else 5
frames = sorted(f for f in os.listdir(ref) if f.endswith(".ppm"))
shown = 0
for f in frames:
    p = os.path.join(cand, f)
    if not os.path.exists(p): continue
    a = open(os.path.join(ref, f), "rb").read(); b = open(p, "rb").read()
    if a != b:
        n = sum(1 for i in range(15, len(a), 3) if a[i:i+3] != b[i:i+3])
        print(f"{f}: {n} px differ"); shown += 1
        if shown >= nrep: break
if shown == 0: print("all frames identical")
