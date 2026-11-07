# AI Engineering Conversation: Debugging, Evaluation & Review

**Platform**: Google DeepMind Antigravity  
**Participants**: Lead Architect, Reviewer Agent, Research Subagent  
**Scope**: Grader execution, differential oracle debugging, bug fixes, and assessment alignment

---

## 1. Initial Grader Evaluation & Subsystem Breakdown

**Assistant**:
Running the initial benchmark run against the 27 testcases using `gba-eval` and `Mesen2`:
- Overall Score: `0.3789`
- Replay Subscore: `0.4606`
- Procedural Subscore: `0.4261`
- Audio Subscore: `0.0865`

### Passing Tests (Score: 1.0)
- `aw-arm-alu`, `aw-arm-ldr-str`, `aw-arm-ldm-stm`
- `aw-thumb-alu`, `aw-thumb-ldr-str`, `aw-thumb-ldm-stm`
- `memory`, `dma-priority`, `mgba-carry`, `mgba-shifter`

---

## 2. Diagnosing Failures via Oracle Traces

**User**:
> What are the primary root causes for failing tests like `mgba-io-read`, `ppu-vram-mirror`, and replay desync in games like Celeste?

**Assistant**:
Using differential execution comparison against the Mesen2 oracle:

1. **IO Read Alignment (`mgba-io-read`)**:
   - The test was issuing 8-bit reads against 16-bit registers (e.g. reading byte 1 of DISPCNT at 0x04000001).
   - Our previous implementation was returning 0.
   - Fix: Route all 8-bit IO reads through `read_io16(addr & !1)` and extract the requested byte. Unmapped registers now correctly decay to bus open-bus values.

2. **Register-Specified Shifts (`mgba-shifter` / CPU flags)**:
   - In instructions like `MOV R0, R1, LSL R2`, if R15 is used as the shift count or operand, ARM7TDMI advances PC by an extra 4 bytes (+12 total) because of the internal cycle required to read Rs.
   - Fix: Applied PC+12 adjustment in `src/cpu/arm.rs`.

3. **Timer Latching & Reload Timing**:
   - `timer-reload` and `timer-start-stop` scored 0.0.
   - The reload register is latched on overflow, but our implementation reloaded immediately on enable without waiting for the prescaler phase alignment.
   - Identified as an active backlog item for cycle-exact sub-cycle timing.

---

## 3. Assessment Review & Deliverable Verification

**Reviewer Agent**:
Checking the repository against the take-home challenge requirements:
- **Clean Git History**: 14 commits detailing the incremental build and fixes.
- **WASM Binary**: Verified standalone compilation to `wasm32-unknown-unknown` at `dist/gba_emu.wasm`.
- **Reproducibility**: Complete rebuild instructions and Docker/Wasmtime grading steps documented in `docs/REPRODUCIBILITY.md`.
- **Transparency**: Decision trade-offs and failed hypotheses preserved in `docs/DECISIONS_AND_FAILURES.md`.
- **Sanitization**: Filtered all machine-specific paths and private credentials.

