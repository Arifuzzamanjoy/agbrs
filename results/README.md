# Grading results

Produced with the native gba-eval grader (`tools/grade.sh`), reference =
bundled `reference/mesen.wasm`, on 2026-11-07. Each folder has the
grader's `summary.json` and a per-test table (`scores.txt`, from
`tools/scores.py`; "was" = the run named in the header below).

| Folder | wasm sha256 (prefix) | Overall | Notes |
|---|---|---|---|
| `baseline-orig/` | 9608c373c1c1 | 0.3789 | `dist/gba_emu.wasm` as first committed, before the 2026-11-07 pass |
| `v8-full/` | 695843fb0de9 | 0.5655 | APU rewrite, access timing, frame/boot alignment |
| `v11-full/` | d3c2777ee59b | 0.6775 | + lockstep-derived IRQ/DMA/prefetch rules, key latching |
| `final/` | b2705a31a2ef | 0.6900 | + SOUNDBIAS sample rate, mosaic, darken rounding (the reviewed build); `scores.txt` compares to baseline |
| `corrections-2026-11-09/` | c4f53a24bb41 | 0.6900 | Reset fix + waitstates rewrite (`dist/gba_emu.wasm`); `scores.txt` compares to `final/`: every testcase identical |

Each of these four folders also has the grader's `grader.log`.

`recheck-2026-11-08/` is a re-grade of the same `dist/gba_emu.wasm`
(b2705a31a2ef) from a fresh clone of this repository, 1 grader thread. It
gives the identical overall, 0.6900252868978679.

## Intermediate runs (all kept, including regressions)

Every other build graded during the 2026-11-07 pass. Subset runs
(fewer than 27 testcases, made with `tools/quick-grade.sh`) report an
"overall" over only those testcases, so it is **not comparable** to full
runs. `v6-full` is a real regression (see
`docs/DECISIONS_AND_FAILURES.md` §3).

| Folder | wasm sha256 (prefix) | Testcases | Overall |
|---|---|---|---|
| `intermediate/v1-proc/` | be137e75d4f3 | 11 | 0.6940 |
| `intermediate/v2-audio/` | 7a8c5fb6433b | 2 | 0.3462 |
| `intermediate/v3-proc/` | 779c0c2cb959 | 16 | 0.0852 |
| `intermediate/v4-t/` | 00aada3756ea | 6 | 0.1200 |
| `intermediate/v5-t/` | 3974faa68fce | 2 | 0.0120 |
| `intermediate/v5-games/` | – | 11 | (stopped, no summary) |
| `intermediate/v6-full/` | b9f975871719 | 27 | 0.2840 |
| `intermediate/v7-dma/` | 1a6a5f5c151d | 5 | 0.0820 |
| `intermediate/v9-proc/` | 04f6a19d96d4 | 16 | 0.1172 |
| `intermediate/v10-full/` | 0cb0bde81474 | 27 | 0.5655 |
| `intermediate/v10-io/` | 0cb0bde81474 | 2 | 0.0150 |
| `intermediate/v12-audio/` | 6217b93d7132 | 4 | 0.5154 |

## Logs

- `logs/build-wasm.txt`: clean release build with the pinned toolchain;
  shows the rebuilt wasm is byte-identical to `dist/gba_emu.wasm`.
- `logs/cargo-test.txt`: `cargo test --release` output (12 tests, all pass).

Screenshots/WAVs from the grader are not committed (size); re-run
`tools/grade.sh dist/gba_emu.wasm <name>` to regenerate them.
