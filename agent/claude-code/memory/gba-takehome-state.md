---
name: gba-takehome-state
description: "GBA emulator take-home (agbrs): grading setup, lockstep tool, timing findings, open gaps"
metadata:
  node_type: memory
  type: project
  originSessionId: 02b30b4e-600f-4585-ac58-ba7f06db074a
  modified: 2026-11-07T13:50:29.069Z
---

Repo /work/agbrs is a Mechanize gba-eval take-home (Rust→wasm GBA emulator). Eval harness cloned at /work/gba-eval (LFS pulled, grader + oracle + gba-lockstep built in its target/).

Score history (2026-11-07): baseline 0.3789 → v8 0.5655 (APU rewrite, access timing, boot/frame alignment). Later fixes: cycle-based IRQ latency, DMA timing, prefetch rules, key latching at frame end (goodboy 840→48 diverging frames of 900).

**Why:** user asked to check task compliance, raise benchmark score with validated methods, and keep handoff honest.

**How to apply:**
- Grade with tools/grade.sh <wasm> <name> <threads>; >2 threads OOMs on this 8 GB box. Subsets: tools/quick-grade.sh.
- tools/lockstep (build with CARGO_TARGET_DIR=../gba-eval/target) is the main debugging instrument; rules + evidence in docs/TIMING_MODEL.md.
- Open leads: chip-advance line-64 HBlank IRQ taken ~11 cycles earlier than the 9-cycle rule; VRAM contention on 32-bit writes; prefetch/DMA ±1 edges.
- Handoff docs from before this pass contain unverifiable claims (fake commit 5fd7684, missing results/summary.json, reconstructed transcripts, waitstates.rs "mirroring Mesen2") — flagged to user, not silently changed. See [[gba-handoff-honesty]].

2026-11-07 handoff audit (session 01dfc9aa): added transcripts/claude-code/ (exported by tools/export_transcripts.py, emails redacted), results/intermediate/ (all v1–v12 runs + grader logs), results/logs/, rust-toolchain.toml (1.99.1), agent/claude-code/README.md, Disclosures + Omissions sections in HANDOFF.md. Two author TODOs left: prior-experience statement (HANDOFF.md) and human time for the Claude Code pass (docs/WORKFLOW_METADATA.md). Push remotes: origin=Arifuzzamanjoy/agbrs, [redacted-account]=[redacted-account]/agbrs.
