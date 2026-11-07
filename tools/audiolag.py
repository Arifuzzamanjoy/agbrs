#!/usr/bin/env python3
"""Waveform cross-correlation lag between reference and candidate audio, per 1 s window.
Usage: audiolag.py ref.wav cand.wav [ref_skip_samples]
Positive lag = candidate is BEHIND the reference by that many samples."""
import sys, wave
import numpy as np
def load(f):
    w = wave.open(f)
    return np.frombuffer(w.readframes(w.getnframes()), dtype=np.int16).reshape(-1, w.getnchannels())[:, 0].astype(float), w.getframerate()
r, rate = load(sys.argv[1]); c, _ = load(sys.argv[2])
r = r[int(sys.argv[3]) if len(sys.argv) > 3 else 0:]
def lag(a, b, maxlag=4000):
    a = a - a.mean(); b = b - b.mean()
    N = 1 << int(np.ceil(np.log2(2 * len(a))))
    x = np.fft.irfft(np.fft.rfft(a, N) * np.conj(np.fft.rfft(b, N)), N)
    lags = np.concatenate([np.arange(0, maxlag), np.arange(-maxlag, 0)])
    vals = np.concatenate([x[:maxlag], x[-maxlag:]])
    return -int(lags[vals.argmax()]), vals.max() / (np.linalg.norm(a) * np.linalg.norm(b) + 1e-9)
for t in range(0, int(min(len(r), len(c)) / rate)):
    s = t * rate
    k, corr = lag(r[s:s + rate], c[s:s + rate])
    print(f"t={t:3d}s lag {k:6d} samples ({k / rate * 1000:6.1f} ms)  corr {corr:.3f}")
