# Artifact Validation Scripts

These scripts verify that downloaded CLI and SDK artifacts work correctly before use or release.

## Quick Start

### Option 1: Direct validation (recommended)

After downloading artifacts from GitHub Actions, validate directly:

```bash
# Point validators at download folder (works with any artifact naming)
bash scripts/validate_artifacts.sh /path/to/downloads
```

The orchestrator script **automatically detects** artifacts regardless of naming or folder structure.

### Option 2: Organize manually first

If you prefer explicit organization:

```bash
# Organize messy artifacts into standard structure
bash scripts/organize_artifacts.sh /path/to/downloads
# Creates: ./artifacts/ with proper folder layout

# Now validate
bash scripts/validate_artifacts.sh ./artifacts
```

---

**Result:** All tests should pass (✓). If any fail (✗), the artifact is broken and should not be released.

## Individual Scripts

### `validate_artifacts.sh` — Orchestrator

Runs all available tests on the artifacts directory. **Automatically finds artifacts** regardless of folder structure or naming:

- CLI binary (looks for any executable named `*-cli`)
- Python wheel (searches recursively for any `.whl`)
- R package (searches for `DESCRIPTION` file marker)
- JavaScript SDK (searches recursively for `query.js`)

This allows you to point the validator directly at messy CI downloads without manual reorganization.

**Usage:**

```bash
# Direct on download folder (auto-detects all artifacts)
bash scripts/validate_artifacts.sh /path/to/downloads

# Or on organized artifacts
bash scripts/validate_artifacts.sh ./artifacts

# Run comprehensive tests (real API calls, 2-3 min)
bash scripts/validate_artifacts.sh --deep /path/to/artifacts
```

**Smart Detection:** If you have scattered artifacts in different locations, the `organize_artifacts.sh` helper can organize them into a standard structure first.

### `organize_artifacts.sh` — Artifact Organization Helper

Scans a messy download folder and organizes artifacts into a clean structure compatible with validators.

CI builds produce inconsistent artifact naming and folder structures:

- CLI might extract to `goat-cli` or `goat-cli-macos-aarch64` or nested in a folder
- Python wheel might be named `goat_sdk-*.whl` or `goat_cli-*.whl`
- R and JavaScript packages often tar to a single `goat/` folder (not `r/goat/` or `js/goat/`)

This script **auto-detects** these variations and reorganizes them:

```bash
bash scripts/organize_artifacts.sh /path/to/messy/downloads
# Creates: ./artifacts/
#   ├── goat-cli                     (executable)
#   ├── goat_sdk-*.whl               (Python)
#   ├── r/
#   │   └── goat/                    (R package)
#   └── js/
#       └── goat/                    (JavaScript SDK)
```

Then validate the organized artifacts:

```bash
bash scripts/validate_artifacts.sh ./artifacts
```

**Optional:** Use this helper if you prefer explicit organization. The main orchestrator (`validate_artifacts.sh`) can also detect artifacts directly without reorganization.

Tests the CLI binary with smoke tests from GETTING_STARTED:

- Help works (`--help`)
- Subcommands work (`taxon search --help`)
- URL generation works (`--url` flag)
- Field groups list works (`--list-field-groups`)

**Usage:**

```bash
bash scripts/validate_cli.sh /path/to/goat-cli
```

### `validate_python_sdk.sh` — Python SDK Testing

Tests the Python wheel by:

- Installing to a temporary venv
- Importing `QueryBuilder`
- Building queries with methods (`set_taxa`, `add_field`)
- Generating URLs
- Running validation

**Usage:**

```bash
bash scripts/validate_python_sdk.sh /path/to/goat_sdk-*.whl
```

**Requirements:** Python 3.10+ with `pip`

### `validate_r_sdk.sh` — R SDK Testing

Tests the R package by:

- Loading the package library
- Instantiating `QueryBuilder$new()`
- Calling builder methods (`$set_taxa()`, `$add_field()`)
- Generating URLs (`$to_url()`)

**Usage:**

```bash
bash scripts/validate_r_sdk.sh /path/to/r/goat
```

**Requirements:** R (≥ 4.1), plus R packages: `R6`, `httr`, `jsonlite`, `yaml`, `devtools`

### `validate_javascript_sdk.sh` — JavaScript SDK Testing

Tests the JavaScript module by:

- Importing `QueryBuilder`
- Instantiating builders
- Calling methods (`setTaxa`, `addField`)
- Generating URLs

**Usage:**

```bash
bash scripts/validate_javascript_sdk.sh /path/to/js/goat
```

**Requirements:** Node.js 18+

## What Gets Tested

Each language's validator tests **the same core operations** shown in [GETTING_STARTED.md](../GETTING_STARTED.md), using a minimal smoke-test example:

| Operation      | Python                                        | R                                              | JavaScript                       |
| -------------- | --------------------------------------------- | ---------------------------------------------- | -------------------------------- |
| Create builder | `QueryBuilder("taxon")`                       | `QueryBuilder$new("taxon")`                    | `new QueryBuilder("taxon")`      |
| Set taxa       | `.set_taxa(["Mammalia"], filter_type="tree")` | `$set_taxa(c("Mammalia"), filter_type="tree")` | `.setTaxa(["Mammalia"], "tree")` |
| Add field      | `.add_field("genome_size")`                   | `$add_field("genome_size")`                    | `.addField("genome_size")`       |
| Generate URL   | `.to_url()`                                   | `$to_url()`                                    | `.toUrl()`                       |

**Python and R validators also test:**

- `.validate()` / `$validate()` — validation without network calls

Each script runs **~5 quick smoke tests** covering import, instantiation, chaining, URL generation, and validation.

**See also:** The [full SDK examples](../GETTING_STARTED.md#3-python-sdk) in GETTING_STARTED demonstrate richer queries including `.add_attribute()`, `.describe()`, `.snippet()`, and `.search()`. The validation scripts use a simpler example to keep runtime < 30 seconds.

---

## Language-Specific Notes

### Python SDK

The Python validator tests the compiled extension module (`goat_sdk`) which provides the same functionality as the Python examples in [GETTING_STARTED.md #3](../GETTING_STARTED.md#3-python-sdk).

### R SDK

The R validator loads the package using `devtools::load_all()` and tests the `QueryBuilder` R6 class. This matches the workflow in [GETTING_STARTED.md #4](../GETTING_STARTED.md#4-r-sdk).

### JavaScript SDK

The JavaScript validator tests the Node.js module with dynamic `import()` and verifies the WASM binding works. This matches the REPL workflows in [GETTING_STARTED.md #5](../GETTING_STARTED.md#5-javascript-sdk).

---

## CI Integration

Add to your CI before publishing artifacts:

```yaml
- name: Validate artifacts
  run: |
    unzip goat-cli-*.zip -d ./artifacts
    bash scripts/validate_artifacts.sh ./artifacts
```

## Test Coverage Summary

Each script runs **~5 quick smoke tests** covering:

| Test            | CLI | Python | R   | JS  |
| --------------- | --- | ------ | --- | --- |
| Help/Import     | ✓   | ✓      | ✓   | ✓   |
| Instantiation   | ✓   | ✓      | ✓   | ✓   |
| Builder methods | ✓   | ✓      | ✓   | ✓   |
| URL generation  | ✓   | ✓      | ✓   | ✓   |
| Validation      | ✓   | ✓      | —   | —   |
| Field groups    | ✓   | —      | —   | —   |

Tests take **~10–30 seconds** total (mostly Python venv setup).

## Deep Validation (Comprehensive Testing)

For thorough validation before release, use the `--deep` flag:

```bash
bash scripts/validate_artifacts.sh --deep /path/to/artifacts
```

This runs **comprehensive tests** (2–3 minutes) including:

- Real API calls to goat.genomehubs.org
- Query building with multiple attributes and operators
- Response parsing and transformations
- Schema validation and describe/snippet generation

**With --deep, tests include:**

| Test                                        | CLI | Python | R   | JS  |
| ------------------------------------------- | --- | ------ | --- | --- |
| Quick tests (above)                         | ✓   | ✓      | ✓   | ✓   |
| `.validate()` / `$validate()`               | —   | ✓      | ✓   | —   |
| `.count()` / `$count()`                     | —   | ✓      | ✓   | ✓   |
| `.search()` / `$search()`                   | —   | ✓      | ✓   | ✓   |
| `.add_attribute()` chaining                 | —   | ✓      | ✓   | ✓   |
| Multiple operators (gt, ge, le, eq, exists) | —   | ✓      | ✓   | ✓   |
| `.parse_response_status()`                  | —   | ✓      | —   | —   |
| `.describe()` schema generation             | —   | ✓      | ✓   | ✓   |
| `.snippet()` code generation                | —   | ✓      | ✓   | ✓   |

Use `--deep` for final validation before publishing artifacts.

## Exit Codes

- `0` — All tests passed
- `1` — One or more tests failed

The main orchestrator will skip missing artifacts gracefully (e.g., if JS SDK not present).

## Troubleshooting

**"CLI not found or not executable"**

- Check the path: `ls -la /path/to/goat-cli`
- Make it executable: `chmod +x goat-cli`

**"Failed to install wheel"**

- Ensure `pip` is available: `python3 -m pip --version`
- Check wheel filename: `ls -la *.whl`

**"Compilation error or R packages missing" (R SDK)**

- Install missing R packages:
  ```r
  install.packages(c("R6", "httr", "jsonlite", "yaml"))
  ```
- Or install the full R package:
  ```bash
  cd r/goat
  R -e "install.packages(c('devtools', 'R6', 'httr', 'jsonlite', 'yaml'))"
  R -e "devtools::install()"
  ```

**"Node.js not found"**

- JavaScript tests skip silently if Node.js is missing
- Install from https://nodejs.org/ if you want JS testing

**"URL doesn't contain API base"**

- Network failure or API URL misconfigured in goat config
- Check that `goat.yaml` has correct `api_url`
