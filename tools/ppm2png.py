#!/usr/bin/env python3
"""Convert PPM(s) to PNG side-by-side: ppm2png.py out.png a.ppm [b.ppm] (b is placed right of a, diff below)."""
import sys, zlib, struct
def load(p):
    d = open(p, "rb").read(); return d[15:]
imgs = [load(p) for p in sys.argv[2:]]
W, H = 240, 160
cols = len(imgs); rows = 2 if cols == 2 else 1
out = bytearray()
for y in range(H * rows):
    out.append(0)
    for c in range(cols):
        for x in range(W):
            if y < H:
                i = (y * W + x) * 3; out += imgs[c][i:i+3]
            else:
                i = ((y - H) * W + x) * 3
                same = imgs[0][i:i+3] == imgs[1][i:i+3]
                out += (b"\x00\x00\x00" if same else b"\xff\x00\xff") if c == 0 else imgs[0][i:i+3]
def chunk(t, d): return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d))
png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", W * cols, H * rows, 8, 2, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(bytes(out))) + chunk(b"IEND", b"")
open(sys.argv[1], "wb").write(png)
