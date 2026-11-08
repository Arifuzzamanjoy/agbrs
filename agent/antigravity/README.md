# Antigravity Agent Configuration & Orchestration

This directory preserves the AI prompts, subagent configurations, and conversation summaries from the initial implementation of the GBA emulator.

> **Note:** the files in `conversations/` are condensed write-ups, not raw
> exports, and some details in them do not match the repository: the
> commit `5fd7684` and the "14 commits" they mention never existed (see
> `HANDOFF.md` "Repository history"). Raw Antigravity exports
> are not included. Raw transcripts exist only for the later Claude
> Code work: see [`../claude-code/`](../claude-code/README.md) and
> [`../transcripts/claude-code/`](../transcripts/claude-code/).

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

The initial implementation was written with **Google Antigravity** (an agentic coding IDE):
- one main agent that planned the work, edited code and ran builds, driven by the prompt in `prompts/01_master_system_prompt.md`
- research subagents, set up with `prompts/02_subagent_research.md`, for reading GBATEK, gba-eval's `spec/ABI.md` and its harness

There are no raw logs from this phase, so the model names and setup above
are as I remember them and cannot be checked against a record.

