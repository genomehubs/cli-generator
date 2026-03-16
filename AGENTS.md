# Agent contribution guide

This file applies to all AI agents and automated tools contributing to this
repository. It complements `.github/copilot-instructions.md` (VS Code Copilot
specific) with conventions that are model- and tool-agnostic.

---

## Agent-log requirement

Every session in which an agent makes **significant changes** — broadly, anything
that would warrant a meaningful commit message — must produce an agent-log entry.

**Purpose:** Keeps LLM contributions transparent, auditable, and reproducible.
Other developers (human or AI) can understand what was changed, why, and what
judgements were made.

### File location and naming

```
agent-logs/YYYY-MM-DD_NNN_short-description.md
```

| Segment             | Meaning                                              |
| ------------------- | ---------------------------------------------------- |
| `YYYY-MM-DD`        | ISO 8601 date of the session                         |
| `NNN`               | Three-digit daily sequence number, starting at `001` |
| `short-description` | Kebab-case summary of the task, 3–6 words            |

Examples:

```
agent-logs/2026-03-16_001_initial-template-setup.md
agent-logs/2026-03-16_002_add-export-subcommand.md
agent-logs/2026-03-17_001_fix-hypothesis-ci-profile.md
```

### Schema

See [agent-logs/README.md](agent-logs/README.md) for the full schema and a
worked example.

---

## When to proceed vs. when to ask

**Proceed autonomously when:**

- The task is clearly scoped by the issue or conversation context.
- The change is localised (one or two modules, existing patterns to follow).
- Verification steps (tests, linting) can confirm correctness.

**Ask the user before proceeding when:**

- The task requires deleting files or branches (destructive and hard to undo).
- The intended behaviour is ambiguous and different interpretations lead to
  meaningfully different implementations.
- The change affects shared infrastructure (CI, pyproject.toml, Cargo.toml)
  in a way that could break other contributors.
- You are about to `git push`, open a PR, or take any action visible outside
  the local repo.

---

## Repository conventions

See [CONTRIBUTING.md](CONTRIBUTING.md) for the full coding standards. Key
points for agents:

- **Logic in `src/core/`** — `lib.rs` and `main.rs` only wire, never implement.
- **Never edit `src/generated/`** — managed by the CLI generator.
- **Commit `proptest-regressions/` and `.hypothesis/examples/`** — failure
  databases; do not gitignore them.
- **Conventional commit messages** — `feat:`, `fix:`, `refactor:`, `test:`,
  `docs:`, `chore:`.
- **No dead code** — if code is no longer called, delete it.

---

## Verification before committing

Always run the following before marking a task complete:

```bash
cargo fmt --all && cargo clippy --all-targets -- -D warnings && cargo test
black --check --line-length 120 python/ tests/python/
isort --check-only --profile black --line-length 120 python/ tests/python/
pyright python/ tests/python/
pytest tests/python/ -v
```

If `maturin develop` has not been run in this session, run it first so Python
tests can import the compiled extension:

```bash
maturin develop --features extension-module
```

---

## Generated code

The `src/generated/` directory and `config/` directory support the CLI generator
tool. When the CLI generator updates a project it:

1. Reads the YAML config in `config/`.
2. Regenerates `src/generated/` and `src/cli_meta.rs` only.
3. Updates `[package.metadata.cli-gen]` in `Cargo.toml` with the new version and
   config hash.

No other files are touched. Agents should respect the same boundary.
