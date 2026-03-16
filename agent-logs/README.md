# Agent logs

This directory contains committed summaries of AI-assisted coding sessions.
All entries are human-readable markdown files and are part of the permanent
project history.

---

## Purpose

Keeping LLM contributions visible and auditable means:

- New contributors (human or AI) can understand why decisions were made.
- Regressions can be traced back to specific sessions.
- The project maintains a clear record of which parts of the codebase were
  written or significantly influenced by AI.

---

## File naming convention

```
YYYY-MM-DD_NNN_short-description.md
```

| Segment             | Meaning                                      |
| ------------------- | -------------------------------------------- |
| `YYYY-MM-DD`        | ISO 8601 date of the session                 |
| `NNN`               | Three-digit daily sequence starting at `001` |
| `short-description` | Kebab-case task summary, 3–6 words           |

---

## Log schema

Every log file must contain the following frontmatter and sections. Omit
sections that are not applicable rather than leaving them blank.

```markdown
---
date: YYYY-MM-DD
agent: <agent name, e.g. "GitHub Copilot", "Claude", "GPT-4o">
model: <model version, e.g. "claude-sonnet-4-5", "gpt-4o-2024-11">
task: <one-line description of what was requested>
files_changed:
  - path/to/file.rs
  - path/to/other.py
---

## Task summary

One paragraph describing the request and what was done.

## Key decisions

- **Decision:** short justification.
- **Alternative considered:** why it was not chosen.

## Interaction log

Abbreviated — capture the meaningful turns, not a verbatim transcript.

| Turn | Role  | Summary                          |
| ---- | ----- | -------------------------------- |
| 1    | User  | Initial task description         |
| 2    | Agent | Clarifying questions asked       |
| 3    | User  | Clarifications provided          |
| 4    | Agent | Implementation approach proposed |
| 5    | Agent | Files created/edited             |

## Changes made

High-level summary of what changed and why. Reference specific files.

## Notes / warnings

Anything a future contributor should know: limitations, known tradeoffs,
follow-up tasks left intentionally out of scope.
```

---

## When to create a log

Create a log entry for any session where the agent:

- Implements a new feature or module.
- Makes a non-trivial bug fix or refactor.
- Changes project structure, CI, or tooling configuration significantly.

Skip log entries for trivial sessions (single-line fixes, typo corrections,
formatting-only changes).
