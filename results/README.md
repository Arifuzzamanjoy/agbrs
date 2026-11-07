# Grading results

Produced with the native gba-eval grader (`tools/grade.sh`), reference =
bundled `reference/mesen.wasm`, on 2026-11-07. Each folder has the
grader's `summary.json` and a per-test table (`scores.txt`, from
`tools/scores.py`; "was" = the run named in the header below).

| Folder | wasm sha256 (prefix) | Overall | Notes |
|---|---|---|---|
| `baseline-orig/` | 9608c373c1c1 | 0.3789 | `dist/gba_emu.wasm` as received (commit e549c6a) |
| `v8-full/` | 695843fb0de9 | 0.5655 | APU rewrite, access timing, frame/boot alignment |
| `v11-full/` | (see summary) | 0.6775 | + lockstep-derived IRQ/DMA/prefetch rules, key latching |
| `final/` | b2705a31a2ef | 0.6900 | + SOUNDBIAS sample rate, mosaic, darken rounding (`dist/gba_emu.wasm`); `scores.txt` compares to baseline |

Screenshots/WAVs from the grader are not committed (size); re-run
`tools/grade.sh dist/gba_emu.wasm <name>` to regenerate them.
