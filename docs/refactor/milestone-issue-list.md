# Milestone-based implementation issue list

This document turns the refactor plan into a ready-to-implement backlog. It is intended to be used as the execution plan for the generic runtime refactor and the site preset convenience layer.

## Milestone 0 — Baseline and compatibility guardrails

### Issue 0.1: Freeze current behavior and identify stale assumptions

**Goal:** Establish a clear baseline before moving functionality to runtime metadata.

**Scope:**

- inventory generated-site assumptions across the Rust CLI, Python SDK, JS SDK, and R SDK
- identify which values are truly site-specific vs generic
- capture the current generated-path behavior with tests

**Files likely involved:**

- [src/main.rs](src/main.rs)
- [src/core/config.rs](src/core/config.rs)
- [src/commands/new.rs](src/commands/new.rs)
- [src/commands/update.rs](src/commands/update.rs)
- [templates/rust/main.rs.tera](templates/rust/main.rs.tera)

**Acceptance criteria:**

- a documented map exists for what must remain runtime-driven
- a list exists of generated-site assumptions that will be intentionally removed from the default path
- baseline tests cover multiple API base values

---

### Issue 0.2: Introduce compatibility mode for generated repos

**Goal:** Keep the legacy `new` / `update` workflow working while the default path shifts to runtime config.

**Scope:**

- mark generated-site mode as compatibility mode
- preserve output generation for old repos
- ensure docs clearly distinguish compatibility mode from the main product path

**Files likely involved:**

- [src/commands/new.rs](src/commands/new.rs)
- [src/commands/update.rs](src/commands/update.rs)
- [src/commands/preview.rs](src/commands/preview.rs)

**Acceptance criteria:**

- current generator flows still work
- output warns that the runtime-generic path is preferred
- compatibility mode is documented and easy to avoid

---

### Issue 0.3: Define runtime metadata contract

**Goal:** Define the schema that the runtime will load instead of embedding it into generated packages.

**Scope:**

- index metadata
- field metadata
- validation metadata
- site preset definitions
- cache freshness and refresh semantics

**Files likely involved:**

- [src/core/config.rs](src/core/config.rs)
- new runtime metadata files under `src/core/runtime/`

**Acceptance criteria:**

- a canonical metadata contract exists for runtime lookup
- contract is stable across CLI and SDK wrappers
- refresh rules are documented

---

## Milestone 1 — Runtime preset registry and configuration layer

### Issue 1.1: Add runtime preset registry

**Goal:** Support user-friendly site shortcuts (`goat`, `boat`, and future instances) without site-specific code generation.

**Scope:**

- add a `SitePreset` registry model
- store common public site metadata in config
- allow future extension without code changes

**Files likely involved:**

- `config/site-presets.yaml` (new)
- [src/core/config.rs](src/core/config.rs)
- new runtime config module(s)

**Acceptance criteria:**

- `goat` and `boat` presets resolve to valid runtime config
- additional presets can be added via config only
- no code changes are required for adding a new instance preset

---

### Issue 1.2: Add CLI and env precedence for runtime config

**Goal:** Make runtime site selection ergonomic and predictable.

**Scope:**

- `--site`
- `--api-base`
- environment variables `GENOMEHUBS_SITE`, `GENOMEHUBS_API_BASE`
- precedence order: CLI > env > preset > default

**Files likely involved:**

- [src/main.rs](src/main.rs)
- new runtime config loader

**Acceptance criteria:**

- explicit flags override env and preset values
- env vars override presets
- default values are only used when no user choice is supplied

---

### Issue 1.3: Add preset selection helper for common public sites

**Goal:** Make the default user path as simple as possible.

**Scope:**

- add preset metadata for goat, boat, and any other known public instances
- ensure `ui_base`, `api_base`, and default index are available
- ensure these values are runtime values, not generated code

**Files likely involved:**

- `config/site-presets.yaml` (new)
- docs for CLI examples

**Acceptance criteria:**

- users can run a command with `--site goat` or `GENOMEHUBS_SITE=goat`
- presets point at fully valid endpoint roots
- runtime config includes enough metadata to bootstrap the client

---

## Milestone 2 — Runtime metadata fetch and cache layer

### Issue 2.1: Add generic metadata fetcher

**Goal:** Pull schema information from the live API at runtime.

**Scope:**

- fetch indices and field metadata
- normalize data into a stable schema model
- account for API version differences and optional endpoints

**Files likely involved:**

- new file: `src/core/runtime/client.rs`
- new file: `src/core/runtime/schema.rs`

**Acceptance criteria:**

- metadata can be fetched from a configured API base
- response normalization is consistent across supported instances
- failures are surfaced cleanly with actionable messages

---

### Issue 2.2: Add metadata cache with refresh policy

**Goal:** Prevent every command from hitting the network unnecessarily while keeping schema fresh.

**Scope:**

- short-lived cache
- TTL/refresh support
- explicit refresh flag
- offline or stale-cache fallback behavior

**Files likely involved:**

- new file: `src/core/runtime/cache.rs`
- runtime config layer

**Acceptance criteria:**

- cached metadata is reused for repeated queries
- `--refresh-metadata` updates the cache
- expired or invalid metadata is rejected cleanly

---

### Issue 2.3: Replace static validation assumptions with runtime validation

**Goal:** Stop validating queries against generated static field tables.

**Scope:**

- validate fields on query construction or execution against runtime metadata
- error messages should include which metadata source was used
- handle missing index or field gracefully

**Files likely involved:**

- new file: `src/core/runtime/validation.rs`
- query-building code paths in the generic CLI and SDKs

**Acceptance criteria:**

- invalid fields fail based on live metadata
- valid fields work across different public instances with no rebuild
- CLI and SDKs share the same validation semantics

---

## Milestone 3 — Generic CLI refactor

### Issue 3.1: Make the default CLI path generic and runtime-configured

**Goal:** Remove the need for site-generated field tables in the default CLI.

**Scope:**

- switch the primary CLI flow to runtime config + metadata lookup
- keep generated-site CLI generation as compatibility mode only
- ensure `--site` and `--api-base` are first-class inputs

**Files likely involved:**

- [src/main.rs](src/main.rs)
- command modules under `src/core/command/`

**Acceptance criteria:**

- one generic CLI binary targets multiple public instances without rebuild
- command output remains stable across site changes
- runtime config is used for the default command path

---

### Issue 3.2: Add metadata introspection commands

**Goal:** Make debugging and user discovery easy.

**Scope:**

- `describe` / `metadata` / `list-indexes` / `list-fields` style subcommands
- show how the runtime schema is resolved and refreshed

**Files likely involved:**

- `src/core/command/metadata.rs` (new)
- CLI wiring in [src/main.rs](src/main.rs)

**Acceptance criteria:**

- users can inspect available indexes and fields at runtime
- the same commands work against goat and boat without rebuilds
- output is useful for diagnostics and troubleshooting

---

### Issue 3.3: Deprecate the old generated-site CLI experience

**Goal:** Make the migration explicit but safe.

**Scope:**

- warnings in generated-site commands
- docs and release notes
- user messaging to prefer generic runtime usage

**Files likely involved:**

- [src/main.rs](src/main.rs)
- [README.md](README.md)
- [GETTING_STARTED.md](GETTING_STARTED.md)

**Acceptance criteria:**

- users see a clear migration signal without hard breakage
- the project narrative clearly prefers the generic runtime path

---

## Milestone 4 — Generic SDK runtime layer

### Issue 4.1: Refactor the Python `QueryBuilder` to runtime metadata

**Goal:** Make Python SDK query building generic and site-agnostic.

**Scope:**

- support `site` and `api_base` runtime configuration
- lazy metadata lookup for validation and field suggestions
- runtime refresh support for stale schema

**Files likely involved:**

- `python/cli_generator/query.py`
- `python/cli_generator/__init__.py`
- new config helper module under `python/cli_generator/`

**Acceptance criteria:**

- Python builder works against multiple API bases without rebuild
- field validation is metadata-driven
- a user can point the same builder to goat or boat via runtime config

---

### Issue 4.2: Refactor the JavaScript SDK to runtime metadata

**Goal:** Bring JS in line with the generic runtime design.

**Scope:**

- JS builder accepts runtime site/API settings
- metadata lookup and validation happen at runtime
- convenience preset support for common sites

**Files likely involved:**

- `templates/js/query.js`
- JS package template files

**Acceptance criteria:**

- same builder can target different site URLs without regenerated packages
- preset selection works for goat and boat
- validation uses runtime metadata

---

### Issue 4.3: Refactor the R SDK to runtime metadata

**Goal:** Keep the R package aligned with the runtime generic model.

**Scope:**

- runtime `api_base` or `site` selection
- metadata-backed validation and introspection
- no site-specific field tables in the default runtime path

**Files likely involved:**

- `templates/r/query.R`
- R package template files

**Acceptance criteria:**

- R query builder behaves the same as Python/JS against multiple endpoints
- no rebuild is required to change target site
- field discovery works dynamically

---

### Issue 4.4: Add parity tests for runtime config across SDKs

**Goal:** Ensure cross-language parity while simplifying the architecture.

**Scope:**

- create parity tests for runtime preset resolution and metadata-backed validation
- verify the same query semantics across Python/R/JS

**Files likely involved:**

- `tests/python/test_sdk_parity.py`
- `tests/r/...` if present
- `tests/javascript/...` if present

**Acceptance criteria:**

- all three SDKs behave consistently for the same runtime config
- parity tests pass for both generic runtime and compatibility mode

---

## Milestone 5 — Template simplification and logic extraction

### Issue 5.1: Move runtime logic out of Tera templates

**Goal:** Make templates easier to edit and debug.

**Scope:**

- extract real query logic from templates into runtime modules
- leave templates as package scaffolding only

**Files likely involved:**

- [templates/rust/main.rs.tera](templates/rust/main.rs.tera)
- [templates/rust/client.rs.tera](templates/rust/client.rs.tera)
- [templates/python/query.py.tera](templates/python/query.py.tera)
- [templates/rust/fields.rs.tera](templates/rust/fields.rs.tera)

**Acceptance criteria:**

- templates no longer contain the main implementation logic
- core logic is in maintainable modules that can be tested directly
- template files are easier to read and troubleshoot

---

### Issue 5.2: Remove generated static field assumptions from default runtime path

**Goal:** Stop using compiled static field groups as the default source of truth.

**Scope:**

- reduce dependence on generated field tables
- keep them only as compatibility fallback or cache source

**Files likely involved:**

- [templates/rust/cli_flags.rs.tera](templates/rust/cli_flags.rs.tera)
- [templates/rust/fields.rs.tera](templates/rust/fields.rs.tera)
- runtime validation layer

**Acceptance criteria:**

- runtime metadata is used in the primary path
- generated metadata remains available only as compatibility fallback or optional optimization

---

### Issue 5.3: Simplify template context and generated package wiring

**Goal:** Cut template complexity and make code generation less brittle.

**Scope:**

- reduce template variables to the minimal package-wiring set
- remove redundant context from generated package setup
- keep the Rich generated shell but simplify logic flow

**Files likely involved:**

- generator code under `src/commands/`
- template rendering files

**Acceptance criteria:**

- template rendering is easier to reason about
- fewer template variables are required for runtime logic
- generated package creation remains straightforward

---

## Milestone 6 — Documentation, migration, and release readiness

### Issue 6.1: Update README and getting-started docs

**Goal:** Make the new architecture clear for new users.

**Scope:**

- describe the runtime generic CLI as the default product
- show `--site goat`, `--site boat`, `--api-base`, and env-var shortcuts
- add a short migration note for old generated repos

**Files likely involved:**

- [README.md](README.md)
- [GETTING_STARTED.md](GETTING_STARTED.md)

**Acceptance criteria:**

- users can understand the new generic runtime flow immediately
- examples reflect the new recommended path
- generated-site mode is clearly called out as compatibility only

---

### Issue 6.2: Release note and migration guide

**Goal:** Make the architecture change explicit to maintainers and users.

**Scope:**

- explain why runtime metadata is preferred
- document the convenience preset strategy
- describe compatibility and deprecation timing

**Files likely involved:**

- docs/refactor/
- release notes or changelog

**Acceptance criteria:**

- migration covers both new users and existing generated repos
- rationale is documented in a way maintainers can reference later

---

### Issue 6.3: Final cleanup and acceptance review

**Goal:** Ensure the refactor is ready to merge and ship.

**Scope:**

- verify all milestone checks are green
- run validation suite
- confirm compatibility mode is documented and stable

**Files likely involved:**

- all touched implementation and test files

**Acceptance criteria:**

- all major milestones are complete
- no critical runtime assumption remains embedded in the default path
- the project is ready for implementation handoff or PR creation

---

## Suggested ordering for the first implementation sprint

1. Milestone 0 — compatibility guardrails and runtime metadata contract
2. Milestone 1 — runtime preset registry and config precedence
3. Milestone 2 — metadata fetch and cache
4. Milestone 3 — generic CLI path
5. Milestone 4 — Python/R/JS runtime SDK alignment
6. Milestone 5 — template simplification
7. Milestone 6 — docs and release cleanup

## Definition of done for the overall refactor

This refactor is complete when:

- one generic CLI works against multiple public instance APIs without rebuilds
- one generic SDK path works across public instances without site-specific generation
- field/index metadata is runtime-resolved for the primary path
- convenience presets exist for goat, boat, and other known public sites
- legacy generated repos remain supported in compatibility mode
- templates are reduced to packaging scaffolding rather than product logic

## Recommended next action

Start with Milestone 0 and Milestone 1 together. Those two milestones establish the runtime preset and metadata foundation that the rest of the refactor depends on.
