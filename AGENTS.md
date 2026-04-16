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

---

## Adding a new PyO3 function to generated projects

When you add a new function to `src/core/` and want it available to users of a
generated SDK (not just cli-generator itself), **six touch-points** must all be
updated together. Missing any one of them produces a build error or `ImportError`
at runtime.

| #   | File                                              | What to do                                                                                                       |
| --- | ------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------- |
| 1   | `src/lib.rs`                                      | Add the `#[pyfunction]` + register it in `#[pymodule]`                                                           |
| 2   | `templates/rust/lib.rs.tera`                      | Mirror the same function (using `crate::embedded::core::` paths) and register it                                 |
| 3   | `src/commands/new.rs` → `copy_embedded_modules()` | Add the new `core/*.rs` file to `modules_to_copy` and declare it in `core_mod_rs_content`                        |
| 4   | `src/commands/new.rs` → `required_deps`           | Add any new Cargo dependencies the new module needs                                                              |
| 5   | `src/commands/new.rs` → `patch_python_init()`     | Add the function name to the generated `__init__.py` import and `__all__`                                        |
| 6   | `templates/python/query.py.tera`                  | Expose the function via a `QueryBuilder` method, keeping signatures in sync with `python/cli_generator/query.py` |

### Common pitfalls

- **`include_str!` path resolution** — `include_str!` resolves paths at compile
  time relative to the source file's location. If a core module uses
  `include_str!("../../templates/…")` those files must be copied to the same
  relative location inside the generated project. Add the copy logic in
  `copy_embedded_modules()`.

- **Template/library signature drift** — `query.py.tera` and
  `python/cli_generator/query.py` must have identical method signatures. When
  adding or changing a parameter, update **both** files. Drift causes `NameError`
  or unexpected `TypeError` in generated projects that is invisible until
  `maturin develop` + runtime test, not caught by clippy or pyright on the
  generator itself.

- **`patch_python_init()` exports must match the extension's registered functions**
  — only import names that are actually registered in `#[pymodule]`. For example,
  `version` is exported by cli-generator itself but is _not_ registered in
  generated projects, so it must not appear in the generated `__init__.py`.

- **End-to-end test after every generated-project change** — `cargo test` only
  tests that _generation_ succeeds (file structure, Cargo.toml contents, etc.).
  It does not compile the generated extension. Always finish with:
  ```bash
  rm -rf /tmp/test-cli && cargo run -- new <site> --config sites/ --output-dir /tmp/test-cli
  cd /tmp/test-cli/<site>-cli && maturin develop --features extension-module
  python3 -c "from <site>_sdk.query import QueryBuilder; qb = QueryBuilder('taxon'); print(qb.describe()); print(qb.snippet()['python'])"
  ```

### Checklist for adding a new language to `snippet()`

Adding a new snippet language (e.g. R, JavaScript) requires:

1. Add `<lang>_snippet.tera` to `templates/snippets/`.
2. Register it in `SnippetGenerator::new()` in `src/core/snippet.rs` via
   `tera.add_raw_template(…, include_str!(…))`.
3. The `copy_embedded_modules()` function in `new.rs` automatically copies all
   `.tera` files from `templates/snippets/` — no change needed there.
4. Add Python tests in `tests/python/test_core.py` covering the new language key.
5. Update `GETTING_STARTED.md` to move the language from "Future" to "Supported"
   in the `snippet()` section.
