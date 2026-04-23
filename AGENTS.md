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
- **SDK fixture tests require matching URL-state assertions** — whenever a new
  entry is added to `FIXTURE_TO_BUILDER` in any of the three fixture test files
  (`tests/python/test_sdk_fixtures.py`, `tests/javascript/test_sdk_fixtures.mjs`,
  `tests/r/test_sdk_fixtures.R`), a matching entry **must** be added to
  `FIXTURE_EXPECTED_URL_PARTS` in the same file. The entry must assert:
  1. `result=<index>` — confirms the correct index was set.
  2. One substring per non-default builder method called — each method that
     changes query state must have at least one observable effect in the URL.
  Use raw (percent-encoded) substrings as they appear in the URL (e.g.
  `genome_size%3Amin` for `add_field("genome_size", modifiers=["min"])`).
  All three files must stay structurally identical.

---

## Extending cli-generator (Adding Parameters, Validators, Languages)

When adding new functionality that spans multiple languages (Python, R, JavaScript),
follow the **Rust-first pattern** to keep code DRY:

### Quick reference: Rust → Python → R/JS → Docs

1. **Rust core** (`src/core/`) — Add logic + unit tests
2. **Python FFI** (`src/lib.rs` + `python/cli_generator.pyi`) — PyO3 binding
3. **R/JS templates** (`templates/r/query.R`, `templates/js/query.js`) — Call Rust binding
4. **Cross-language tests** (`tests/python/test_sdk_parity.py`) — Verify all languages work
5. **Documentation** (GETTING_STARTED.md, docstrings) — Add examples

### What NOT to do

❌ Add logic to Python template that duplicates Rust
❌ Implement same validator in R and JS separately
❌ Create language-specific implementations of identical functionality

✅ Add it once in Rust; expose via PyO3 + templates; test all languages together

### Full guide

See [docs/planning/extension-guide.md](../docs/planning/extension-guide.md) for:

- 5 worked examples (new parameter, new language, validator, snippet language, custom structure)
- Task checklist before submitting
- Common pitfalls and anti-patterns

---

## Verification before committing

Use `scripts/verify_code.sh` to run all checks in one step:

```bash
bash scripts/verify_code.sh
```

This runs: `cargo fmt`, `cargo clippy`, `cargo test --workspace`, `black`,
`isort`, `pyright`, and `pytest`. Use `--verbose` to see full output on failure.

If `maturin develop` has not been run in this session, run it first so Python
tests can import the compiled extension:

```bash
maturin develop --features extension-module
```

### End-to-end dev-site test

Unit and integration tests do not compile the generated extension. After any
change to templates, the embedded module system, or the WASM subcrate, run:

```bash
bash scripts/dev_site.sh [--no-rebuild-wasm] [--python] [SITE]
```

This script: cleans the previous output, regenerates the site CLI, runs a
Rust `--url` smoke-test, runs a JS `toUrl()` smoke-test, and optionally builds
the Python extension and runs a Python smoke-test.

WASM is rebuilt by default. Pass `--no-rebuild-wasm` to skip it when only
templates or non-WASM Rust was changed.

### Artifact validation (release prep)

Before releasing or during MVP preparation, validate that downloaded CLI and SDK
artifacts work correctly across platforms:

```bash
# Quick validation (smoke tests, ~30 seconds)
bash scripts/validate_artifacts.sh /path/to/artifacts

# Deep validation (real API calls, ~2-3 minutes)
bash scripts/validate_artifacts.sh --deep /path/to/artifacts
```

Validators auto-detect CLI, Python, R, and JavaScript SDKs regardless of artifact
folding structure. For organizing messy CI downloads:

```bash
bash scripts/organize_artifacts.sh /path/to/downloads
bash scripts/validate_artifacts.sh ./artifacts
```

See [VALIDATION.md](scripts/VALIDATION.md) for comprehensive documentation.

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
  bash scripts/dev_site.sh --python goat
  ```

- **`create_js_package()` in `new.rs` has its own Tera context** — the
  `query.js` template is rendered by a separate `tera::Tera::one_off()` call
  in `create_js_package()`, not by the main codegen context in `codegen.rs`.
  Any new Tera variable used in `templates/js/query.js` must be added to the
  `context` in `create_js_package()` as well as in `codegen.rs`. Missing it
  causes a `warn: failed to render query.js template` and `query.js` is not
  written to the workdir.

- **R's `extendr-wrappers.R.tera` must be updated for every new Rust fn** —
  `extendr-wrappers.R` is normally regenerated by `rextendr::document()`, but
  the template in `templates/r/extendr-wrappers.R.tera` is what cli-generator
  writes into the generated project. A function registered in `extendr_module!`
  in `lib.rs.tera` but missing from `extendr-wrappers.R.tera` compiles
  successfully yet produces `could not find function "fn_name"` at R runtime.

- **R's `cli_meta.rs` is generated inline, not from a Tera template** — the
  `create_r_package()` function in `new.rs` writes `cli_meta.rs` via a
  `format!()` string. New constants (e.g. `UI_BASE_URL`) must be added
  directly to that format string, unlike the main Rust path which uses
  `templates/rust/cli_meta.rs.tera`.

- **WASM is rebuilt automatically by `dev_site.sh`** — `--rebuild-wasm` is
  no longer required; WASM is now the default. Use `--no-rebuild-wasm` only
  when you are certain no `#[wasm_bindgen]` exports changed. If a new export
  is added but the pkg is stale, generated JS will throw
  `TypeError: wasmModule.<fn> is not a function` at runtime — invisible to
  `cargo test`, `pyright`, or clippy.

- **Embedded module path confusion** — Functions added to
  `crates/genomehubs-query/src/lib.rs` are **not** available as
  `crate::embedded::genomehubs_query::` in generated projects. The subcrate
  source files are copied piecemeal into `src/embedded/core/` by
  `copy_embedded_modules()` in `src/commands/new.rs`. Only files explicitly
  listed there end up in generated projects. New modules from the subcrate
  (e.g. `parse.rs`) must be:
  1. Added to the copy list in `copy_embedded_modules()`.
  2. Declared in the generated `core_mod_rs_content` string.
  3. Referenced as `crate::embedded::core::<module>::` in `lib.rs.tera`.
     The path `crate::embedded::genomehubs_query::` does **not** exist in
     generated projects.

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
