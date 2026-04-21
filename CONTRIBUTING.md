# Contributing

Thank you for contributing. This document covers code style, testing requirements,
and the PR process. These rules apply equally to humans and AI agents.

---

## Guiding principle

Code is read far more often than it is written. Optimise for the next reader —
human or LLM — being able to reason over the logic without needing to hold
large context in their head.

---

## Code style

### Formatting

Formatting is non-negotiable and entirely automated. Do not waste review time on
style debates.

| Language       | Tool                          | Invocation                                                      |
| -------------- | ----------------------------- | --------------------------------------------------------------- |
| Rust           | `rustfmt` via `rust-analyzer` | `cargo fmt --all`                                               |
| Python         | `black`                       | `black --line-length 120 python/ tests/python/`                 |
| Python imports | `isort`                       | `isort --profile black --line-length 120 python/ tests/python/` |
| JavaScript     | `prettier`                    | Applied in generated projects (via template pre-commit hooks)   |
| R              | `styleR` / `lintr`            | Applied in generated projects (via template CI)                 |
| TOML           | `even-better-toml` (VS Code)  | on save                                                         |

Line length: **120** for both Rust and Python.

Install pre-commit hooks to enforce this automatically before every commit:

```bash
pre-commit install
```

### Naming and cross-language consistency

Names should communicate intent such that a reader with no prior context
understands what a value holds or what a function does.

**Do:**

```rust
fn gc_content(sequence: &str) -> f64 { ... }
let high_quality_reads = reads.iter().filter(|r| r.mean_quality >= min_quality).count();
```

**Do not:**

```rust
fn calc(s: &str) -> f64 { ... }
let n = reads.iter().filter(|r| r.mean_quality >= min_quality).count();
let temp = result.unwrap();
```

**Convention across all languages:**

- Rust: `snake_case` for functions and variables, `CamelCase` for types
- Python: `snake_case` for functions and variables, `PascalCase` for classes
- JavaScript: `camelCase` for functions and variables, `PascalCase` for classes
- R: `snake_case` for functions and variables, `PascalCase` for classes (S6)

**SDK method parity:** When adding a new QueryBuilder method, use the same base name across all three SDKs:

- Rust/Python/R: `set_taxa()`, `add_field()`, `to_url()`, `validate()`, etc.
- JavaScript: `setTaxa()`, `addField()`, `toUrl()`, `validate()` (camelCase)

See [SDK Parse Parity Plan](/memories/repo/sdk-parse-parity-plan-draft.md) for canonical method list.

---

## Function design

### Keep functions small and single-purpose

A function should do one thing. If you can name what that thing is without the
word "and", you're on the right track. Aim for functions under ~30 lines.

**Refactor this:**

```rust
fn process_input(raw: &str) -> Result<Output, Error> {
    // 60 lines of: parsing, validation, transformation, and formatting
}
```

**Into this:**

```rust
fn process_input(raw: &str) -> Result<Output, Error> {
    let parsed = parse_raw_input(raw)?;
    let validated = validate_parsed_input(parsed)?;
    let transformed = apply_transformation(validated);
    Ok(format_output(transformed))
}
```

The names in the second version are documentation. The reader can skim the
top-level flow and drill into sub-functions only when needed.

### Prefer early returns over nesting

Instead of:

```rust
fn handle(value: Option<i64>) -> String {
    if let Some(v) = value {
        if v > 0 {
            format!("positive: {v}")
        } else {
            "zero or negative".to_string()
        }
    } else {
        "missing".to_string()
    }
}
```

Prefer:

```rust
fn handle(value: Option<i64>) -> String {
    let Some(v) = value else { return "missing".to_string() };
    if v <= 0 { return "zero or negative".to_string() }
    format!("positive: {v}")
}
```

Max 2 levels of nesting as a soft rule. If you're reaching for a third,
consider extracting a helper.

### No speculative code

Do not add functionality, abstractions, or configuration options that are not
required by the current task. Future requirements are better served by a
readable codebase that is easy to extend than by one pre-loaded with
half-used abstractions.

---

## Documentation

### Rust

Every public function, struct, and module must have a doc comment (`///`).
The first line is a one-sentence description. Use code examples where helpful.

```rust
/// Returns the GC content of a DNA/RNA sequence as a fraction in `[0.0, 1.0]`.
///
/// Both upper- and lower-case bases are recognised. Returns `0.0` for an
/// empty sequence. Callers are responsible for ensuring the sequence contains
/// only valid nucleotide characters.
pub fn gc_content(sequence: &str) -> f64 {
    if sequence.is_empty() {
        return 0.0;
    }
    let gc_count = sequence.chars().filter(|b| matches!(b, 'G' | 'C' | 'g' | 'c')).count();
    gc_count as f64 / sequence.len() as f64
}
```

### Python

Every public function and class must have a docstring. Use Google-style:

```python
def gc_content(sequence: str) -> float:
    """Return the GC content of a DNA/RNA sequence as a fraction in [0.0, 1.0].

    Args:
        sequence: DNA or RNA sequence string (A, T/U, G, C; any case).

    Returns:
        Fraction of bases that are G or C, in the range [0.0, 1.0].
    """
    if not sequence:
        return 0.0
    return sum(1 for b in sequence if b in "GCgc") / len(sequence)
```

---

## Testing

### Requirements

- Every new function requires at least one unit test.
- Every function with a mathematical contract (commutativity, idempotency,
  round-trip, ordering) requires a property-based test.
- Every new SDK method must be added to **all three SDKs (Python, R, JavaScript)** in the same PR.
  The parity test (`tests/python/test_sdk_parity.py`) enforces this automatically.
- Tests must pass on CI before merging (all platforms: Linux, macOS).

### Test layout

| Language            | Location                                        | Framework            |
| ------------------- | ----------------------------------------------- | -------------------- |
| Rust unit           | `#[cfg(test)]` in the same file                 | `cargo test`         |
| Rust integration    | `tests/*.rs`                                    | `cargo test`         |
| Rust property       | `proptest!` blocks in `#[cfg(test)]`            | proptest             |
| Python unit         | `tests/python/test_*.py`                        | pytest               |
| Python property     | `@given` tests in `tests/python/test_*.py`      | Hypothesis           |
| SDK parity          | `tests/python/test_sdk_parity.py`               | pytest               |
| SDK fixtures        | `tests/python/test_sdk_fixtures.py`             | pytest               |
| Generated SDK tests | Generated project: `tests/python/`, `js/`, `r/` | pytest/Jest/testthat |

Commit `proptest-regressions/` and `.hypothesis/examples/` — they ensure CI
replays every known-failing case.

### Type checking

Python code must pass `pyright` in strict mode:

```bash
pyright python/ tests/python/
```

All function signatures must have full type annotations.

### SDK parity testing

When you add a new QueryBuilder method, it must work identically across all three SDKs:

1. **Add to Rust core** (`src/core/`): Implement the logic once.
2. **Expose via PyO3, WASM, and extendr**: Wire the Rust function in `src/lib.rs`, templates, and crates.
3. **Update all three templates**: `templates/python/`, `templates/js/`, `templates/r/`.
4. **Run the parity test**:

   ```bash
   pytest tests/python/test_sdk_parity.py -v
   ```

   This test introspects the generated QueryBuilder methods in all three languages and asserts they are identical.

5. **Test with `test_sdk_fixtures.sh`** to validate round-trip behavior across languages:
   ```bash
   bash scripts/test_sdk_fixtures.sh --site goat --python --r --javascript
   ```

For detailed guidance on extending cli-generator across languages, see
[Extension Guide](docs/planning/extension-guide.md) and rule #10 in
[Copilot Instructions](.github/copilot-instructions.md).

---

## Architecture: Hand-written vs. generated code

### Generator-managed directories (read-only)

**`src/generated/`** — Output from the CLI generator. Never edit by hand.

- `cli_meta.rs` — CLI name/description constants (generated from YAML)
- `field_*.rs` — Field metadata and validation rules (generated from API schema)
- If generated output is wrong, fix the generator templates, not the output.

**Template-managed files** — Rendered into generated projects.

- `templates/rust/`, `templates/python/`, `templates/js/`, `templates/r/` — Not edited directly; edit templates and regenerate.
- `src/commands/new.rs` → copies templates + embedded modules into `/tmp/generated/<site>/`.

### Hand-written code (your domain)

**`src/core/`** — Pure Rust library logic. Never modified by the generator.

- Add new functions, types, and validators here.
- This is where logic for new parameters, validators, and introspection lives.

**`src/lib.rs`** — PyO3 module FFI boundary. Wires `src/core/` functions to Python.

- When adding to `src/core/`, expose it via `#[pyfunction]` and `#[pymodule]`.

**`crates/genomehubs-query/src/`** — WASM + extendr subcrate.

- Shared parsing and introspection logic exposed to all three SDKs.
- When adding a parse function, add it here and export via WASM + extendr.

**`python/cli_generator/`** — Python package re-exporting the Rust extension.

- `__init__.py` — imports from Rust extension
- `query.py` — QueryBuilder wrapper (high-level SDK API)
- Keep signatures in sync with `templates/python/query.py.tera`

### Rust-first pattern for multi-language features

When adding functionality that spans Python, R, and JavaScript:

1. **Add logic in Rust** (`src/core/`) ← All intelligence lives here
2. **Expose via PyO3** (`src/lib.rs` + `python/cli_generator.pyi`)
3. **Expose via WASM + extendr** (`crates/genomehubs-query/src/`)
4. **Update templates** (`templates/python/`, `templates/js/`, `templates/r/`)
   - These are **wiring only** — they call the Rust function, do not re-implement logic
5. **Test parity** (`pytest tests/python/test_sdk_parity.py`)

**Why:** Maintaining the same logic across 3 languages is a maintenance nightmare. Do it once in Rust; expose it; wire it in templates. See [AGENTS.md](AGENTS.md) for the extension checklist and [Extension Guide](docs/planning/extension-guide.md) for worked examples.

---

## Helper scripts

This repository includes scripts to automate code verification, SDK testing, and end-to-end validation.

### `scripts/verify_code.sh` — Complete code quality checks

Runs formatting, linting, type checking, and tests on Rust and Python code.

**Usage:**

```bash
# Check all code (concise pass/fail output)
bash scripts/verify_code.sh

# Show detailed output (diffs and error messages)
bash scripts/verify_code.sh --verbose

# Verify a different project (e.g., generated CLI)
PROJECT_ROOT=/path/to/project bash scripts/verify_code.sh
```

**Checks performed:**

| Language | Check   | Tool                      |
| -------- | ------- | ------------------------- |
| Rust     | Format  | `cargo fmt --all`         |
| Rust     | Lint    | `cargo clippy`            |
| Rust     | Tests   | `cargo test --lib`        |
| Python   | Format  | `black --line-length 120` |
| Python   | Imports | `isort --profile black`   |
| Python   | Types   | `pyright` (strict mode)   |
| Python   | Tests   | `pytest` all modules      |

Exit code 0 = all checks pass; 1 = one or more checks failed.

### `scripts/test_sdk_fixtures.sh` — SDK fixture validation

Generates SDKs and validates fixture round-trips across Python, R, and JavaScript.
Tests that generated SDKs can fetch, parse, and round-trip API responses.

**Usage:**

```bash
# Test Python SDK fixtures
bash scripts/test_sdk_fixtures.sh --site goat --python

# Test R SDK fixtures (requires R + devtools)
bash scripts/test_sdk_fixtures.sh --site goat --r

# Test all language SDKs
bash scripts/test_sdk_fixtures.sh --site goat --python --r --javascript

# Show available options
bash scripts/test_sdk_fixtures.sh --help
```

**What it does:**

1. Generates SDKs for the specified site
2. Validates fixture JSON files against API schema
3. Tests round-trip: SDK method → URL → cached fixture → response parsing
4. Verifies response shape consistency across languages

**Fixtures:** Cached API responses committed to `docs/api-schemas/` ensure
tests run without network access and with identical canned data across CI runs.

### `scripts/dev_site.sh` — Full end-to-end test

Generates a site, builds CLI + all SDKs, and runs smoke tests.
Use this after major changes to templates or generation logic.

**Usage:**

```bash
# Quick smoke test (CLI + fixture validation)
bash scripts/dev_site.sh goat

# Rebuild WASM and test JavaScript SDK
bash scripts/dev_site.sh --rebuild-wasm goat

# Run Python SDK tests in generated project
bash scripts/dev_site.sh --python boar
```

**What it does:**

1. Cleans previous output (`/tmp/generated/<site>`)
2. Generates the CLI and SDKs
3. Builds Rust CLI (`cargo build --release`)
4. Tests CLI with `--url` flag (no network)
5. Builds WASM (if `--rebuild-wasm`)
6. Smoke-tests all SDKs: constructors, methods, serialization
7. Optionally runs full Python test suite

### `scripts/validate_templates.sh` — Template formatting

Validates Tera template files without rendered context. Extracts embedded Rust,
Python, JavaScript, and R code, then validates formatting with appropriate tools.

**Usage:**

```bash
# Validate all templates
bash scripts/validate_templates.sh
```

**How it works:**

1. Locates all `.tera` files in `templates/` and subdirectories
2. Extracts code by replacing Tera expressions with valid placeholders:
   - Rust/JavaScript: `{{ ... }}` → `0`, `{% ... %}` → comments
   - Python: `{{ ... }}` → `None`, `{% ... %}` → comments
   - R: `{{ ... }}` → `NULL`, `{% ... %}` → comments
3. Validates extracted code with rustfmt/black/isort/prettier/styler

Exit code 0 = all templates pass; 1 = one or more templates have issues.

**Troubleshooting template errors:**

If a template fails validation:

1. Generate the SDK: `bash scripts/dev_site.sh <site>`
2. Edit the generated file (e.g., `/tmp/generated/<site>/<lang>/<module>.rs`)
   - You'll get full syntax highlighting and linting in your editor
3. Verify formatting: `cargo fmt` / `black` / `isort` / `prettier`
4. Once the generated file is correct, apply the changes back to the template
5. Regenerate and verify output matches your edits

---

Use [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<optional scope>): <short description>

<optional body>
```

Common types: `feat`, `fix`, `refactor`, `test`, `docs`, `chore`, `ci`.

Examples:

```
feat(cli): add gc-content subcommand
fix(core): handle empty sequence in gc_content
test(core): add proptest for gc_content unit interval invariant
docs: clarify maturin develop step in GETTING_STARTED
```

---

## Pull request checklist

Use `bash scripts/verify_code.sh` to run all checks automatically:

```bash
# Run all code and test verification checks
bash scripts/verify_code.sh

# Or with verbose output showing diffs/errors
bash scripts/verify_code.sh --verbose
```

For SDK-related changes:

```bash
# Validate SDKs round-trip correctly across languages
bash scripts/test_sdk_fixtures.sh --site goat --python --r --javascript

# Full end-to-end test (regenerate + build + test)
bash scripts/dev_site.sh goat
```

Manual verification:

- [ ] `cargo fmt --all` and `cargo clippy -- -D warnings` pass clean.
- [ ] `black`, `isort`, and `pyright` pass clean.
- [ ] All new functions have doc comments (Rust) / docstrings (Python).
- [ ] All new functions have unit tests; all 371+ tests pass locally.
- [ ] **If you added a QueryBuilder method:** Also added to all three SDK templates (`templates/python/`, `templates/js/`, `templates/r/`); `test_sdk_parity.py` passes.
- [ ] **If you modified Tera templates:** `bash scripts/validate_templates.sh` and `bash scripts/dev_site.sh <site>` pass.
- [ ] No dead code, no commented-out blocks, no speculative features.
- [ ] Agent-log entry created in `agent-logs/` if an AI agent contributed
      significantly (see [AGENTS.md](AGENTS.md)).

---

## Agent contributions

AI agents are welcome for certain tasks, but their use depends on your
experience level with the codebase.

### Experienced contributors (≥ 5 merged commits in this organisation)

Agents are welcome. Use them to accelerate implementation, explore approaches,
or handle boilerplate-heavy tasks. Requirements:

- Review all agent-generated code before committing — you are responsible for
  correctness, not the model.
- Create an agent-log entry in `agent-logs/` for any session involving
  significant changes (see [AGENTS.md](AGENTS.md)).
- The PR description must note that AI assistance was used.

### New contributors (< 5 merged commits in this organisation)

Please avoid submitting AI-generated code. This is not a blanket restriction on
AI tools — it is a request to start by reading and writing code yourself.

The reason: reviewing AI-generated PRs from contributors who haven't yet built
familiarity with the codebase is difficult. Subtle errors are easy to miss, and
the review burden falls entirely on maintainers. A few PRs written by hand
build the shared understanding that makes AI-assisted work productive later.

Agents are still fine for:

- Spinning up a new project from this template (`cargo generate`).
- Answering questions about the codebase.
- Generating draft tests that you then read, understand, and edit.

Once you have a few merged contributions and are comfortable with the patterns
here, AI assistance for implementation is fully encouraged.
