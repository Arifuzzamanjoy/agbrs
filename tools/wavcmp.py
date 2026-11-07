#!/usr/bin/env python3
"""Compare reference vs candidate WAV: levels, DC, dominant pitch over time.
Usage: wavcmp.py ref.wav cand.wav [window_secs...]"""
import sys, wave
import numpy as np
def load(f):
    w = wave.open(f); r = w.getframerate(); ch = w.getnchannels()
    a = np.frombuffer(w.readframes(w.getnframes()), dtype=np.int16).reshape(-1, ch).astype(float)
    return a, r
ref, r = load(sys.argv[1]); cand, _ = load(sys.argv[2])
print(f"rate {r}  ref {len(ref)/r:.2f}s  cand {len(cand)/r:.2f}s")
for name, a in (("ref", ref), ("cand", cand)):
    print(f"{name:5s} L mean {a[:,0].mean():8.1f} rms {a[:,0].std():8.1f}  R mean {a[:,1].mean():8.1f} rms {a[:,1].std():8.1f}  min {a.min():.0f} max {a.max():.0f}")
N = 2048
times = [float(x) for x in sys.argv[3:]] or list(np.linspace(0, len(ref)/r - N/r - 0.01, 8))
for t in times:
    row = []
    for a in (ref, cand):
        seg = a[int(t*r):int(t*r)+N, 0]
        if len(seg) < N: row.append("   n/a"); continue
        sp = np.abs(np.fft.rfft((seg - seg.mean()) * np.hanning(N)))
        k = sp[3:].argmax() + 3
        row.append(f"peak {k*r/N:7.1f}Hz rms {seg.std():7.1f}")
    print(f"t={t:6.2f}s  ref[{row[0]}]  cand[{row[1]}]")
