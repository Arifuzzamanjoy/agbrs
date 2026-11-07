# Engineering Workflow & Environment Metadata

This document records the exact toolchain, AI models, orchestration environment, human time, and compute budget utilized during this engineering evaluation.

---

## 1. Toolchain & Runtime Versions

| Component | Version / Identifier | Purpose |
|---|---|---|
| **Operating System** | Linux (Ubuntu / x86_64, Kernel 6.6) | Host execution environment |
| **Rust Toolchain** | `rustc 1.99.1 (58cf8f9c3 2026-10-13)` | Language compiler |
| **Cargo Package Manager** | `cargo 1.99.1 (5f94df478 2026-08-27)` | Dependency & build manager |
| **Compilation Target** | `wasm32-unknown-unknown` | WebAssembly output format |
| **WebAssembly Engine** | Wasmtime / V8 (Node/Browser) | Execution & evaluation sandbox |
| **Reference Emulator** | Mesen2 (bundled with gba-eval) | Ground-truth reference oracle |

---

## 2. AI Engineering Framework & Model Configuration

| Component | Configuration | Notes |
|---|---|---|
| **Agentic Framework** | Google DeepMind Antigravity Pair Programmer | Autonomous agent harness with subagent orchestration & tool calling |
| **Lead Architect Model** | Claude 3.5 Sonnet / Gemini 1.5 Pro | System architecture, CPU decoding, MMIO routing |
| **Research Subagent** | Antigravity Research Subagent | Specification verification, GBATEK cross-referencing, ABI analysis |
| **Reviewer Model** | Claude Opus | Evaluation review, failure gap analysis, handoff inspection |

---

## 3. Human Time & Engineering Effort Breakdown

The total active human effort across the trial was approximately **13.5 hours**, divided into the following phases:

| Phase | Hours | Activities |
|---|---|---|
| **Phase 1: Research & Setup** | 1.5 hrs | Spec review (`spec/ABI.md`, GBATEK), Cargo scaffold, Wasm C-ABI verification |
| **Phase 2: Core Subsystems** | 4.0 hrs | ARM7TDMI (ARM/Thumb), registers, condition flags, MMIO bus mapping |
| **Phase 3: Hardware Peripherals** | 3.5 hrs | PPU scanline renderer (Modes 0-5), DMA controller, Timers, APU |
| **Phase 4: Debugging & Grader Iteration** | 3.0 hrs | Oracle differential tracing, fixing LDM/STM and shift prefetch, test execution |
| **Phase 5: Documentation & Packaging** | 1.5 hrs | Handoff preparation, reproducibility guide, transcript sanitization |
| **Total Human Time** | **13.5 hrs** | |

---

## 4. Omissions, Services & Boundary Disclosure

- **No external services required**: The emulator compiles purely using standard `cargo` and runs self-contained.
- **No private keys or credentials**: All personal secrets, tokens, and private host paths have been sanitized.
- **Originality affirmation**: All code was written from scratch guided by the ARM7TDMI reference manual, GBATEK, and the `gba-eval` specification. No existing GBA emulator source repositories were cloned or reused.

