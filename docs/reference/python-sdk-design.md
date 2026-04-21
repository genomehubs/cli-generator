# Python SDK design note

How to go from the current generator output to a usable Python SDK for
querying GoaT (and other genomehubs instances) directly from Python code.

Written: 2026-03-16

---

## Current state

The rust-py-template already has the full PyO3 scaffolding in place:

- `src/lib.rs` — `#[pymodule]` and `#[pyfunction]` FFI boundary (gated behind
  the `extension-module` feature so the binary still builds without libpython).
- `python/goat_cli/__init__.py` — re-exports compiled Rust symbols.
- `python/goat_cli/goat_cli.pyi` — pyright-compatible type stubs.
- `maturin` — builds the `.so`/`.pyd` extension from `pyproject.toml`.

The generated `src/generated/client.rs` already has three standalone Rust
functions — `search`, `count`, `lookup` — that do the real work. The only
thing currently exposed to Python is the placeholder `gc_content` function.

In other words: **the plumbing exists; only the wiring is missing.**

---

## What needs to happen

### Layer 1 — Thin `#[pyfunction]` wrappers in `lib.rs`

`lib.rs` is a hand-written file and will not be overwritten by
`cli-generator update`. For each index × operation:

```rust
#[pyfunction]
fn taxon_search(query: &str, fields: Vec<String>, size: usize) -> PyResult<String> {
    use goat_cli::generated::{client, indexes::Index};
    let field_refs: Vec<&str> = fields.iter().map(String::as_str).collect();
    client::search(Index::Taxon, query, &field_refs, size, "text/tab-separated-values")
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
}
```

The only non-trivial part is the error conversion (`anyhow::Error` →
`PyRuntimeError`). Write this `From` impl once in `lib.rs`:

```rust
#[cfg(feature = "extension-module")]
impl From<anyhow::Error> for pyo3::PyErr {
    fn from(e: anyhow::Error) -> Self {
        pyo3::exceptions::PyRuntimeError::new_err(e.to_string())
    }
}
```

With that in place, `?` works directly inside every `#[pyfunction]` body —
no per-function `.map_err(...)` call needed. This `From` impl does **not**
exist yet and needs to be added to `lib.rs`. It is not provided by PyO3 or
anyhow; there is a `pyo3-anyhow` crate that does the same thing, but it is
unmaintained, so writing the four lines above is preferable.

Register each function in the `#[pymodule]` init and add typed signatures to
`goat_cli.pyi`.

### Layer 2 — Return types

Three natural options, in ascending order of usefulness to Python users:

| Option                        | Return type            | Effort     | Notes                                                       |
| ----------------------------- | ---------------------- | ---------- | ----------------------------------------------------------- |
| **Raw string**                | `str`                  | Minimal    | TSV/JSON as-is; user parses                                 |
| **List of dicts**             | `list[dict[str, Any]]` | Low–medium | Parse TSV/JSON in Rust, return as Python objects via PyO3   |
| **Polars / pandas DataFrame** | `DataFrame`            | High       | Requires additional dep; best left to a thin Python wrapper |

**Prototype path**: return raw strings (one line of Rust per function); add a
Python-side convenience wrapper in `__init__.py` that parses TSV into a list of
dicts or a DataFrame using pure Python. This keeps the Rust layer simple and
lets Python users opt in to heavier deps.

Both `pandas` and `polars` accept a `StringIO` directly, so the wrappers are
one-liners on top of the raw string call:

```python
# python/goat_cli/__init__.py (hand-written prototype layer)
import io
import goat_cli._goat_cli as _ext  # compiled Rust extension

# list-of-dicts (no extra deps)
def taxon_search_records(
    query: str, fields: list[str], size: int = 50
) -> list[dict[str, str]]:
    import csv
    tsv = _ext.taxon_search(query, fields, size)
    return list(csv.DictReader(io.StringIO(tsv), delimiter="\t"))

# pandas DataFrame
def taxon_search_df(
    query: str, fields: list[str], size: int = 50
) -> "pandas.DataFrame":
    import pandas as pd
    tsv = _ext.taxon_search(query, fields, size)
    return pd.read_csv(io.StringIO(tsv), sep="\t")

# polars DataFrame (faster; stricter type inference)
def taxon_search_polars(
    query: str, fields: list[str], size: int = 50
) -> "polars.DataFrame":
    import polars as pl
    tsv = _ext.taxon_search(query, fields, size)
    return pl.read_csv(io.StringIO(tsv), separator="\t")
```

Example usage:

```python
import goat_cli

df = goat_cli.taxon_search_df(
    query="tax_name(Insecta)",
    fields=goat_cli.expand_flags("taxon", ["genome-size", "busco"]),
    size=200,
)
df[df["genome_size"] > 1e9]
```

Both DataFrame libraries handle TSV headers, column types, and missing values
automatically from the string. Polars will infer stricter numeric types
(numeric columns won't silently become `object` dtype) and is faster for large
result sets. Neither library is a hard dependency of the package — the imports
are deferred inside each function so the base install stays lightweight.

The wrapper functions are trivially generatable: `python_init.py.tera` emits
one function per index × operation × return-type variant using the same
`indexes` context variable already available to every other template.

**Production target**: the hand-written prototype should inform but not become
the final implementation. Every site generated by cli-generator needs the same
SDK surface; hand-written `lib.rs` wrappers would diverge immediately as sites
add or remove indexes and field groups. The generator templates (see below)
are the correct long-term home — the prototype is only for validating the
design before committing to the template work.

### Layer 3 — Field-group flags from Python

The generated `cli_flags.rs` structs know which fields belong to each flag
(e.g. `busco` → `["busco_completeness", "busco_lineage", "busco_string"]`).
Exposing a `expand_flags(index, flag_names)` function lets Python code use the
same flag semantics as the CLI without repeating the field lists:

```python
fields = goat_cli.expand_flags("taxon", ["busco", "genome_size"])
# → ["busco_completeness", "busco_lineage", "busco_string", "genome_size", ...]
results = goat_cli.taxon_search("tax_name(Homo sapiens)", fields, size=50)
```

This can be a `#[pyfunction]` that calls into the generated `cli_flags` module.

---

## Generator changes needed (production requirement)

The hand-written prototype is sufficient for one repo, but the generator must
own this surface for the SDK to work across all sites (GoaT, BoaT, future
instances) and stay in sync as fields and indexes change. Two new templates are
needed:

1. **`python_ffi.rs.tera`** — generates `#[pyfunction]` wrappers and
   `#[pymodule]` registration boilerplate for every index × operation
   combination. Rendered to `src/generated/python_ffi.rs`; `lib.rs` declares
   `pub use generated::python_ffi;` and adds a one-line call to register the
   generated module. `lib.rs` itself stays hand-written and is never
   overwritten — it only needs updating when a new operation type is added to
   the generator (rare).

2. **`python_init.py.tera`** — generates `python/{site_name}/__init__.py`
   with the full public API: re-exports from the compiled extension, plus the
   Python convenience wrappers (TSV→dicts). This file _is_ regenerated on
   `cli-generator update`, so hand-written additions should go in a separate
   `_user.py` module that `__init__.py` optionally imports.

3. **`python_stubs.pyi.tera`** — generates `python/{site_name}/{site_name}.pyi`
   from the same index/flag metadata used for the Rust types. Ensures pyright
   stubs stay in sync automatically on every `update`.

This mirrors exactly how `client.rs`, `cli_flags.rs`, and `output.rs` are
already handled: generated files in `src/generated/`, hand-written wiring in
`src/lib.rs`. The pattern is established; the Python templates follow it.

---

## Packaging

`maturin` already handles the build. Users would install with:

```bash
pip install goat-cli          # if published to PyPI
# or locally:
maturin develop --features extension-module
```

The same wheel runs as both a Python library and (via the Rust binary entry
point) a CLI — one crate, two interfaces.

---

## Effort estimate

| Step                                     | Effort                      | Blocks on              |
| ---------------------------------------- | --------------------------- | ---------------------- |
| Thin wrappers in `lib.rs` + `.pyi` stubs | ~2 hours                    | Nothing — doable today |
| Python convenience layer (TSV→dicts)     | ~1 hour                     | Step above             |
| `expand_flags` function                  | ~1 hour                     | Step above             |
| `lib.rs.tera` generator template         | ~half day                   | Stabilised API surface |
| `goat_cli.pyi.tera` generator template   | ~half day                   | `lib.rs.tera`          |
| Polars/pandas DataFrame return option    | ~1 hour (Python layer only) | Convenience layer      |

The first three steps (raw SDK, Python convenience layer, flag expansion) can
be done entirely in the generated repo without touching the generator at all.
The generator templates are a follow-on quality-of-life improvement.

---

## Open questions

- **GIL and blocking HTTP**: `reqwest::blocking` holds the Python GIL for the
  duration of each request. For a small SDK this is fine. For batch queries
  or large paginated results, releasing the GIL (`Python::allow_threads`) or
  switching to async reqwest + a tokio runtime would be necessary. This aligns
  with the async/progress-bar discussion in the CLI gap analysis.

- **Error model**: should network errors surface as Python exceptions
  (`PyRuntimeError`) or as a `Result`-like return type? Exceptions are
  more Pythonic; a `Result` type would require a custom PyO3 class.

- **Versioning the SDK**: if `goat-cli-options.yaml` changes (new flags, new
  fields), the Python API surface changes too. The existing `validate` /
  `update` workflow handles this for the CLI; the SDK stubs would need the
  same treatment.

- **DataFrame return type annotations in `.pyi` stubs**: the deferred import
  pattern (`import pandas`/`import polars` inside the function body) means
  the packages don't need to be installed for type checking. The stubs must
  use `TYPE_CHECKING` guards so pyright is satisfied without requiring either
  library as a real dependency:

  ```python
  from __future__ import annotations
  from typing import TYPE_CHECKING

  if TYPE_CHECKING:
      import pandas
      import polars

  def taxon_search_df(query: str, fields: list[str], size: int = 50) -> pandas.DataFrame: ...
  def taxon_search_polars(query: str, fields: list[str], size: int = 50) -> polars.DataFrame: ...
  ```

  The `python_stubs.pyi.tera` template should emit these guards at the top
  whenever DataFrame variants are generated.
