# Antigravity Agent Configuration & Orchestration

This directory preserves the AI prompts, subagent configurations, and conversation transcripts used during the engineering of the GBA emulator.

---

## Directory Structure

```
agent/antigravity/
├── README.md
├── prompts/
│   ├── 01_master_system_prompt.md  # The primary system prompt given to the AI architect
│   └── 02_subagent_research.md     # The prompt defining the background research subagent
└── conversations/
    ├── 01_architecture_and_implementation.md # Core implementation & build session
    └── 02_debugging_and_review.md            # Grader review, bug diagnoses, and verification
```

---

## Agent Architecture

We utilized **Google DeepMind Antigravity**, an autonomous agentic pair-programming framework equipped with:
- **Lead Architect Agent**: Manages the development roadmap, writes code, edits files, and executes builds.
- **Background Research Subagents**: Spawned asynchronously to read external specifications, cross-reference documentation (e.g. GBATEK, ABI.md), and analyze evaluation harnesses without interrupting active code compilation.
- **Deterministic Sandboxing**: Executes strictly within the project boundary with no external network access or side effects.

