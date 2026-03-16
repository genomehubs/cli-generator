# Workspace instructions for GitHub Copilot

This file is loaded automatically by GitHub Copilot in VS Code. It governs how
Copilot should behave when contributing to **cli-generator**.

---

## Project structure

```
src/core/         Pure library logic. No PyO3 or clap dependencies.
src/lib.rs        PyO3 FFI boundary. Calls into core. No logic here.
src/main.rs       clap CLI entry point. Calls into core. No logic here.
src/cli_meta.rs   CLI name/description constants. Generator-controlled.
src/generated/    Auto-generated code. Never edit by hand.
python/cli_generator/  Python package re-exporting the Rust extension.
tests/python/     pytest + Hypothesis tests.
config/           YAML config snapshots (CLI generator input).
agent-logs/       Committed LLM session summaries.
```

---

## Coding rules

Apply these rules to every edit:

1. **Functions do one thing.** If the body is growing past ~30 lines or contains
   the word "and" in a natural description, extract a named helper.

2. **Prefer early returns over nesting.** Max 2 levels of nesting. Use
   `let-else`, `?`, and guard clauses in Rust; `if not condition: return` in
   Python.

3. **Names communicate intent.** No single-letter variables, no `tmp`, `val`,
   `result`, or `data` unless the scope is a single expression. Names should
   make the code self-documenting.

4. **Add doc comments to all public items.** `///` for Rust, docstrings for
   Python. The first line is a one-sentence description. Skip private helpers
   unless the logic is non-obvious.

5. **No speculative code.** Do not add features, configuration, or abstractions
   that are not needed by the current task. Do not add fallbacks for scenarios
   that cannot happen.

6. **Logic lives in `src/core/`.** `lib.rs` and `main.rs` only wire core
   functions to their respective interfaces. When in doubt: if it would be
   tested with a plain unit test, it belongs in `core`.

7. **Every new function needs a test.** Unit test for the happy path; proptest
   or `@given` test for any invariant property.

8. **Type-annotate all Python functions.** Code must pass `pyright` in strict
   mode. Full signatures required — no implicit `Any`.

9. **Do not edit `src/generated/`.** This directory is managed by the CLI
   generator tool. Changes will be overwritten.

---

## Verify changes

After making changes, run the appropriate checks:

```bash
# Rust
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test

# Python
black --line-length 120 python/ tests/python/
isort --profile black --line-length 120 python/ tests/python/
pyright python/ tests/python/
pytest tests/python/ -v

# After changing Rust source, rebuild the Python extension:
maturin develop --features extension-module
```

---

## Agent-log requirement

For any session where you make significant changes (new features, non-trivial
bug fixes, refactoring), create an agent-log entry:

```
agent-logs/YYYY-MM-DD_NNN_short-description.md
```

See [AGENTS.md](../AGENTS.md) for the required schema and an example.

---

## File hygiene

- Prefer editing existing files over creating new ones.
- Do not leave commented-out code in committed files.
- Do not add `TODO` comments unless you also open a tracking issue.
- Keep `src/generated/` for generated code and `src/core/` for hand-written
  logic — never mix the two.
