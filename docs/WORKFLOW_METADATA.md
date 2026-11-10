# Engineering Workflow & Environment Metadata

Toolchain, AI tools and models, and human time for this take-home.

---

## 1. Toolchain & Runtime Versions

| Component | Version / Identifier | Purpose |
|---|---|---|
| **Operating System** | Linux (Ubuntu / x86_64, kernel 6.8.0-147-generic at the 2026-11-07 pass) | Host execution environment |
| **Rust Toolchain** | `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, pinned in `rust-toolchain.toml` | Language compiler |
| **Cargo Package Manager** | `cargo 1.99.1 (5f94df478 2026-08-27)` | Dependency & build manager |
| **Compilation Target** | `wasm32-unknown-unknown` | WebAssembly output format |
| **WebAssembly Engine** | Wasmtime (inside the gba-eval grader) | Runs the wasm during grading |
| **Reference Emulator** | Mesen2 (bundled with gba-eval) | Ground-truth reference oracle |

---

## 2. AI Engineering Framework & Model Configuration

| Component | Configuration | Notes |
|---|---|---|
| **Agentic Framework** | Google Antigravity | Agentic coding IDE with research subagents |
| **Lead Architect Model** | Claude Opus 4.6 / Gemini 3.8 Flash | System architecture, CPU decoding, MMIO routing |
| **Research Subagent** | Antigravity Research Subagent | Specification verification, GBATEK cross-referencing, ABI analysis |
| **Reviewer Model** | Claude Opus 4.6 | Evaluation review, failure gap analysis, handoff inspection |
| **2026-11-07 pass** | Claude Code 2.1.317, `claude-opus-5-5`, single agent, `auto` permission mode | Timing/APU/PPU fixes, lockstep tool, grading, handoff. Details and token usage: `agent/claude-code/README.md` |

The Antigravity rows above are as I remember them; no raw logs from that phase are in the repo to confirm model names or usage.

---

## 3. Human Time & Engineering Effort Breakdown

The total active human effort for the initial implementation was approximately **13.5 hours**. This is my own estimate after the fact, not tracked time:

| Phase | Hours | Activities |
|---|---|---|
| **Phase 1: Research & Setup** | 1.5 hrs | Spec review (gba-eval `spec/ABI.md`, GBATEK), Cargo scaffold, Wasm C-ABI verification |
| **Phase 2: Core Subsystems** | 4.0 hrs | ARM7TDMI (ARM/Thumb), registers, condition flags, MMIO bus mapping |
| **Phase 3: Hardware Peripherals** | 3.5 hrs | PPU scanline renderer (Modes 0-5), DMA controller, Timers, APU |
| **Phase 4: Debugging & Grader Iteration** | 3.0 hrs | Oracle differential tracing, fixing LDM/STM and shift prefetch, test execution |
| **Phase 5: Documentation & Packaging** | 1.5 hrs | Handoff preparation, reproducibility guide, transcript sanitization |
| **Total Human Time** | **13.5 hrs** | |
| 2026-11-07 Claude Code pass | Not tracked | Wall clock 11:40–16:04 UTC (4 h 23 min) plus the final audit session. I followed the session in the Claude Code app and reviewed the code changes in my IDE as the agent made them, rather than keeping a separate time log. |

---

## 4. Originality

The code was written with AI agents, guided by the ARM7TDMI reference manual, GBATEK and the `gba-eval` specification. No existing GBA emulator repository was cloned or reused. One exception was found in review: the reviewed version's `src/bus/waitstates.rs` closely matched Mesen2's structure and has to be treated as adapted. It was replaced on 2026-11-09 (see `CORRECTIONS.md`). The Claude Code sessions also read parts of gba-eval's own reference wrapper and harness (see `HANDOFF.md`, Disclosures). Omissions and external dependencies are listed in `HANDOFF.md`.
