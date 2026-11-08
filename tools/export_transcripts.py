#!/usr/bin/env python3
"""Export Claude Code session transcripts into the repo, sanitized.

Usage: export_transcripts.py [--redact-brief START END] [--redact-account NAME ...]
                             [--rename OLD=NEW ...] <claude-project-dir> <out-dir> [session-id ...]

<claude-project-dir> is ~/.claude/projects/<mangled-cwd>. For every session
JSONL (or only the ids given) this writes:
  <out-dir>/<id>.jsonl   full raw event stream, sanitized (nothing dropped)
  <out-dir>/<id>.md      readable rendering: user / assistant / tool calls / results
and prints token usage per session (also written to <out-dir>/usage.json).

Sanitization (applied to every string value in every event, keys included):
  - e-mail addresses other than a small public allowlist -> [redacted-email]
  - common credential shapes (GitHub/Anthropic/AWS/Slack tokens) -> [redacted-secret]
  - with --redact-brief START END: every copy of the pasted take-home message
    from the hiring company (the text from its first words START to its last
    words END) -> [redacted: take-home brief ...]. Truncated copies
    (session titles, grep output) are matched by their common prefix with
    the full message. The user's own words around the paste are kept.
  - with --redact-account NAME: the account name NAME (case-insensitive,
    also as a prefix such as NAME-cyber) -> [redacted-account]
  - with --rename OLD=NEW: literal, case-insensitive replacement, used to
    show local paths and the working repository's earlier name as the
    published ones
Nothing else is removed. Thinking blocks are kept as stored by the client
(they may be empty/signature-only).
"""
import json, os, re, sys

ALLOW = {"noreply@anthropic.com", "git@github.com"}  # technical addresses, not personal
EMAIL = re.compile(r"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}")
SECRET = re.compile(r"(ghp_|gho_|ghs_|github_pat_|sk-ant-|AKIA|xox[bp]-)[A-Za-z0-9_-]{8,}")
BRIEF_START = BRIEF_END = None  # set by --redact-brief START END
BRIEF_MARK = ("[redacted: take-home brief from the hiring company, pasted by the user; "
              "its requirements are summarized in HANDOFF.md]")
BRIEFS = []  # full copies of the brief, filled by collect_briefs()
MAX_RESULT = 4000  # chars of each tool result shown in the .md (raw .jsonl is complete)


def collect_briefs(path):
    """Full copies of the brief as typed in this session's user prompts."""
    for line in open(path, encoding="utf-8"):
        d = json.loads(line) if line.strip() else {}
        c = (d.get("message") or {}).get("content") if isinstance(d.get("message"), dict) else None
        if d.get("type") == "user" and isinstance(c, str) and BRIEF_START in c and BRIEF_END in c:
            a = c.index(BRIEF_START)
            b = c.index(BRIEF_END, a) + len(BRIEF_END)
            for v in (c[a:b], json.dumps(c[a:b])[1:-1]):  # raw and JSON-escaped forms
                if v not in BRIEFS:
                    BRIEFS.append(v)


def _skip_ws(s, i):
    while i < len(s):
        if s[i].isspace():
            i += 1
        elif s.startswith("\\n", i) or s.startswith("\\t", i):
            i += 2
        else:
            break
    return i


def common_prefix(text, a, brief):
    """Length of text[a:] that matches a prefix of brief, any whitespace run
    (or escaped \\n) matching any other. Session titles re-wrap the paste."""
    i, j = a, 0
    while i < len(text) and j < len(brief):
        wi, wj = _skip_ws(text, i), _skip_ws(brief, j)
        if wi != i or wj != j:
            if (wi == i) != (wj == j):
                break
            i, j = wi, wj
            continue
        if text[i] != brief[j]:
            break
        i, j = i + 1, j + 1
    while i > a and text[i - 1].isspace():
        i -= 1
    return i - a


def redact_brief(text):
    if not BRIEF_START or BRIEF_START not in text:
        return text
    out, i = [], 0
    while True:
        a = text.find(BRIEF_START, i)
        if a < 0:
            out.append(text[i:])
            return "".join(out)
        out.append(text[i:a])
        best = len(BRIEF_START)
        for b in BRIEFS:  # longest common prefix with any known full copy
            best = max(best, common_prefix(text, a, b))
        if text.startswith("…", a + best):
            best += 1
        end = text.find(BRIEF_END, a)
        if end >= 0 and end - a <= max(len(b) for b in BRIEFS or [""]) + 200:
            best = max(best, end + len(BRIEF_END) - a)
        out.append(BRIEF_MARK)
        i = a + best


ACCOUNTS = []  # compiled patterns from --redact-account
RENAMES = []  # (compiled pattern, replacement) from --rename


def sanitize(text):
    for pat, new in RENAMES:
        text = pat.sub(lambda m: new, text)
    for pat in ACCOUNTS:
        text = pat.sub("[redacted-account]", text)
    if BRIEFS:
        text = redact_brief(text)
    text = SECRET.sub("[redacted-secret]", text)
    return EMAIL.sub(lambda m: m.group(0) if m.group(0) in ALLOW else "[redacted-email]", text)


def scrub(o):
    if isinstance(o, str):
        return sanitize(o)
    if isinstance(o, list):
        return [scrub(x) for x in o]
    if isinstance(o, dict):
        return {scrub(k): scrub(v) for k, v in o.items()}
    return o


def load_clean(path):
    events = []
    for line in open(path, encoding="utf-8"):
        if line.strip():
            events.append(scrub(json.loads(line)))
    return events


def dump(events, path):
    with open(path, "w", encoding="utf-8") as f:
        for e in events:
            f.write(json.dumps(e, ensure_ascii=False) + "\n")


def blocks(content):
    if isinstance(content, str):
        return [{"type": "text", "text": content}]
    return content or []


def result_text(c):
    out = c.get("content")
    if isinstance(out, list):
        out = "\n".join(x.get("text", f"[{x.get('type')}]") for x in out if isinstance(x, dict))
    out = str(out or "")
    if len(out) > MAX_RESULT:
        out = out[:MAX_RESULT] + f"\n... [{len(out) - MAX_RESULT} more chars in the .jsonl]"
    return out


def render(events, title):
    md = [f"# Claude Code session {title}\n",
          "Rendered from the sanitized `.jsonl` next to this file (the complete record). "
          f"Tool results are truncated to {MAX_RESULT} chars here.\n"]
    for d in events:
        t, ts = d.get("type"), d.get("timestamp", "")[:19]
        msg = d.get("message") or {}
        if t not in ("user", "assistant") or not isinstance(msg, dict):
            continue
        for c in blocks(msg.get("content")):
            k = c.get("type")
            if k == "text" and c.get("text", "").strip():
                who = "User" if t == "user" else "Assistant"
                body = c["text"]
                if t == "user":  # plain text: fence it so tags like <pasted_content> stay visible
                    fence = "~" * max(3, 1 + max((len(m) for m in re.findall(r"~{3,}", body)), default=0))
                    body = f"{fence}text\n{body}\n{fence}"
                md.append(f"\n### {who} · {ts}\n\n{body}\n")
            elif k == "tool_use":
                md.append(f"\n**Tool call `{c.get('name')}`** · {ts}\n\n```json\n"
                          f"{json.dumps(c.get('input'), indent=1)[:6000]}\n```\n")
            elif k == "tool_result":
                md.append(f"\n<details><summary>Tool result{' (error)' if c.get('is_error') else ''}"
                          f"</summary>\n\n```\n{result_text(c)}\n```\n</details>\n")
    return "".join(md)


def usage(events):
    u, models, ts = {}, {}, []
    for d in events:
        if d.get("timestamp"):
            ts.append(d["timestamp"])
        m = d.get("message")
        if isinstance(m, dict) and m.get("usage") and d.get("type") == "assistant":
            models[m.get("model")] = models.get(m.get("model"), 0) + 1
            for k, v in m["usage"].items():
                if isinstance(v, int):
                    u[k] = u.get(k, 0) + v
    return {"first_event": min(ts, default=None), "last_event": max(ts, default=None),
            "assistant_messages_by_model": models, "tokens": u,
            "sidechain_events": sum(1 for d in events if d.get("isSidechain"))}


def main():
    global BRIEF_START, BRIEF_END
    argv, args, redact = sys.argv[1:], [], False
    while argv:
        a = argv.pop(0)
        if a == "--redact-brief":
            BRIEF_START, BRIEF_END, redact = argv.pop(0), argv.pop(0), True
        elif a == "--rename":
            old, new = argv.pop(0).split("=", 1)
            RENAMES.append((re.compile(re.escape(old), re.I), new))
        elif a == "--redact-account":
            ACCOUNTS.append(re.compile(re.escape(argv.pop(0)) + r"[\w-]*", re.I))
        else:
            args.append(a)
    src, out = args[0], args[1]
    ids = args[2:] or sorted(f[:-6] for f in os.listdir(src) if f.endswith(".jsonl"))
    os.makedirs(out, exist_ok=True)
    summary = {}
    if redact:
        for sid in ids:
            collect_briefs(os.path.join(src, sid + ".jsonl"))
    for sid in ids:
        events = load_clean(os.path.join(src, sid + ".jsonl"))
        dump(events, os.path.join(out, sid + ".jsonl"))
        open(os.path.join(out, sid + ".md"), "w", encoding="utf-8").write(render(events, sid))
        # subagent transcripts, if any, live in <id>/subagents/*.jsonl
        sub = os.path.join(src, sid, "subagents")
        if os.path.isdir(sub):
            for f in os.listdir(sub):
                if f.endswith(".jsonl"):
                    dump(load_clean(os.path.join(sub, f)), os.path.join(out, f"{sid}.subagent.{f}"))
        summary[sid] = usage(events)
        print(sid, json.dumps(summary[sid]))
    json.dump(summary, open(os.path.join(out, "usage.json"), "w"), indent=2)


if __name__ == "__main__":
    main()
