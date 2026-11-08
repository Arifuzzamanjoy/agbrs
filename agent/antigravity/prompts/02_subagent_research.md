# Research Subagent Configuration & Prompt

```markdown
Role: GBA Eval Requirements Researcher
TypeName: research
Model: Gemini 3.8 Flash / Claude Opus 4.6
Sandbox: Read-only codebase access & reference document inspection

------------------------------------------------------------------------------
SUBAGENT INVOCATION PROMPT
------------------------------------------------------------------------------

Research the GBA Eval challenge requirements thoroughly:

1. Read the specification files:
   - Identify the exact interface/API that the WASM module must expose.
   - Catalog all test ROMs used for grading (Procedural, Replay, Audio).
   - Detail the scoring criteria, subsystem weights, and composite formula.
   - Note any specific Rust/WASM compiler constraints (no custom flags, no std::thread).
   - Outline the expected project structure.

2. Benchmark & Baseline Analysis:
   - Analyze the reference baseline candidate (gba-core) score (~0.53).
   - Identify the primary failure modes that lead to replay desynchronization.
   - Cross-reference with GBATEK regarding scanline cycle counts and audio sample draining.

Report all findings systematically with architectural recommendations.
```

