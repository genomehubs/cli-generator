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
| TOML           | `even-better-toml` (VS Code)  | on save                                                         |

Line length: **120** for both Rust and Python.

Install pre-commit hooks to enforce this automatically before every commit:

```bash
pre-commit install
```

### Naming

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

Rust: use `snake_case` for functions and variables, `CamelCase` for types.
Python: use `snake_case` for functions and variables, `PascalCase` for classes.

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
- Tests must pass on CI before merging.

### Test layout

| Language         | Location                                   | Framework    |
| ---------------- | ------------------------------------------ | ------------ |
| Rust unit        | `#[cfg(test)]` in the same file            | `cargo test` |
| Rust integration | `tests/*.rs`                               | `cargo test` |
| Rust property    | `proptest!` blocks in `#[cfg(test)]`       | proptest     |
| Python unit      | `tests/python/test_*.py`                   | pytest       |
| Python property  | `@given` tests in `tests/python/test_*.py` | Hypothesis   |

Commit `proptest-regressions/` and `.hypothesis/examples/` — they ensure CI
replays every known-failing case.

### Type checking

Python code must pass `pyright` in strict mode:

```bash
pyright python/ tests/python/
```

All function signatures must have full type annotations.

---

## Generated code

Code in `src/generated/` is produced by the CLI generator tool. Do not edit it
by hand — changes will be overwritten on the next generator run. If the generated
output is wrong, fix the generator, not the generated file.

Hand-written code in `src/core/` and `src/lib.rs` is never modified by the
generator.

---

## Commit messages

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

Before requesting review:

- [ ] `cargo fmt --all` and `cargo clippy -- -D warnings` pass clean.
- [ ] `black`, `isort`, and `pyright` pass clean.
- [ ] All new functions have doc comments / docstrings.
- [ ] All new functions have tests; CI passes.
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
