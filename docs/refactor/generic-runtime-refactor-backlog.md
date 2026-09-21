# Generic runtime refactor backlog

## Objective

Simplify the project so that it ships as a single generic CLI/SDK distribution, with instance-specific values resolved at runtime from a live metadata catalog rather than embedded into generated code at build time.

This plan keeps the current generator as an optional bootstrap tool for packaging or custom site scaffolding, but makes the default product instance-agnostic and easier to debug, distribute, and maintain.

## Design principles

1. Site identity is runtime configuration, not a build-time code generation target.
2. Index and field availability are runtime metadata, not compile-time constants.
3. Templates are scaffolding only; real logic lives in normal source modules.
4. Primary sites (for example goat, boat, and other supported instances) get a convenience preset layer without adding site-specific code paths.
5. Existing generated-per-site workflows remain available as compatibility mode during migration.

## End state

- One generic Rust CLI binary.
- One generic Python SDK package.
- One generic JavaScript SDK package.
- One generic R package.
- Instance selection via runtime configuration (`--site`, `--api-base`, or env vars).
- Metadata fetched and cached at runtime.
- No per-instance field catalog embedded in generated artifacts by default.

## Convenience preset model

Add a single user-facing convenience layer for common instances.

### Proposed runtime selector

Support all of the following as equivalent entry points:

- `--site goat`
- `--site boat`
- `--api-base https://.../api`
- environment variable: `GENOMEHUBS_SITE=goat`
- environment variable: `GENOMEHUBS_API_BASE=https://.../api`

### Precedence order

1. explicit CLI flags
2. environment variables
3. site preset default
4. hard-coded fallback only for local development

This keeps experts in control while making the common case fast and simple.

### Preset registry

Store preset definitions in a normal config file, not in the generator templates.

Example structure:

```yaml
# config/site-presets.yaml
sites:
  goat:
    api_base: https://goat.genomehubs.org/api
    ui_base: https://goat.genomehubs.org
    default_index: taxon
    description: GenomeHubs GoaT
  boat:
    api_base: https://boat.genomehubs.org/api
    ui_base: https://boat.genomehubs.org
    default_index: taxon
    description: GenomeHubs BoaT
```

This registry is loaded at runtime and can be extended without touching the generic CLI code.

## Delivery plan

## Phase 0 — baseline and migration guardrails

### Goal

Map the current system and freeze the compatibility surface before refactoring.

### Tasks

1. Inventory every site-specific assumption currently embedded in generator output.
2. Identify which generated files are real logic vs packaging scaffolding.
3. Define a compatibility mode for the current `new` and `update` flow.
4. Define a target runtime metadata API contract.
5. Capture the current behavior in tests for multiple known instance URLs.

### Acceptance criteria

- There is a clear list of generated-site features that can be removed from the default path.
- There is a documented compatibility path for existing generated repos.
- All runtime metadata dependencies are named and versioned.

### Files likely involved

- [src/main.rs](src/main.rs)
- [src/core/config.rs](src/core/config.rs)
- [src/commands/new.rs](src/commands/new.rs)
- [src/commands/update.rs](src/commands/update.rs)

---

## Phase 1 — introduce runtime metadata and preset resolution

### Goal

Move metadata lookup and site selection into the runtime layer so that instance differences do not require rebuilds.

### Tasks

1. Add a `SitePreset` registry model and load it from config.
2. Add a `RuntimeConfig` model with:
   - `api_base`
   - `ui_base`
   - `site_name`
   - `metadata_cache_dir`
   - `refresh_metadata_on_startup`
3. Add CLI parsing for:
   - `--site`
   - `--api-base`
   - `--refresh-metadata`
   - `--metadata-cache-dir`
4. Add environment variable support:
   - `GENOMEHUBS_SITE`
   - `GENOMEHUBS_API_BASE`
   - optional `GENOMEHUBS_UI_BASE`
5. Implement precedence resolution for explicit config > env > preset > defaults.
6. Add runtime schema fetch logic and cache validation.

### Acceptance criteria

- `cli --site goat` resolves to a valid runtime configuration without generation.
- `GENOMEHUBS_SITE=boat` works identically to `--site boat`.
- `--api-base` overrides the preset cleanly.
- Metadata is fetched and cached without embedding site field definitions into the CLI.

### Example user flow

```bash
cli search --site goat --index taxon --field genome_size
cli search --api-base https://goat.genomehubs.org/api --field genome_size
GENOMEHUBS_SITE=boat cli search --index taxon
```

---

## Phase 2 — replace generated site field maps with runtime schema lookup

### Goal

Remove hard-coded field/index assumptions from the default implementation.

### Tasks

1. Add a runtime `SchemaCatalog` loaded from `metadata/fields` and related endpoints.
2. Move validation from generated tables to runtime checks against this catalog.
3. Support dynamic index lookup and field existence checks.
4. Add graceful behaviour for missing fields or indexes.
5. Introduce a `--refresh-metadata` command path to update stale caches.

### Acceptance criteria

- The same CLI binary works against multiple instance URLs without rebuilds.
- Invalid field/index combinations fail with runtime metadata feedback rather than static compile-time assumptions.
- The CLI can recover from metadata drift and refresh the schema cleanly.

---

## Phase 3 — genericize the Python, R, and JavaScript SDKs

### Goal

Make the public SDK surface generic and metadata-backed, not per-site generated.

### Tasks

1. Adjust `QueryBuilder` creation to accept `api_base` and optional `site`/preset values.
2. Load metadata lazily when needed for validation or field suggestions.
3. Replace generated field-group logic with request-time validation and dynamic suggestions.
4. Keep `describe()` and `snippet()` working from runtime metadata.
5. Provide optional preloading and cache controls for offline or reproducible workflows.

### Acceptance criteria

- Python/R/JS SDKs can target multiple host instances without linked generated packages.
- The common-user API remains simple: `QueryBuilder(..., api_base=...)` or `QueryBuilder(..., site="goat")`.
- The same QueryBuilder can be pointed at different deployed instances with no rebuild.

---

## Phase 4 — simplify templates and move logic into real source files

### Goal

Make templates easier to edit, debug, and reason about by reducing them to scaffolding.

### Tasks

1. Move real runtime logic out of Tera templates into Rust modules.
2. Keep templates focused on:
   - package metadata
   - imports
   - CLI entry point wiring
   - generated boilerplate only
3. Simplify query-building code in:
   - [templates/rust/main.rs.tera](templates/rust/main.rs.tera)
   - [templates/rust/client.rs.tera](templates/rust/client.rs.tera)
   - [templates/python/query.py.tera](templates/python/query.py.tera)
   - [templates/rust/fields.rs.tera](templates/rust/fields.rs.tera)
4. Remove unnecessary template variables and generated field tables from the default path.

### Acceptance criteria

- Templates contain little or no actual query logic.
- Core logic can be tested directly in `src/core` or runtime modules.
- The implementation is easier to debug without re-rendering the whole package.

---

## Phase 5 — preserve compatibility for generated repositories

### Goal

Avoid breaking existing generated repos while the default product shifts to runtime metadata.

### Tasks

1. Keep `new` and `update` as compatibility/bootstrap commands.
2. Mark them as “site packaging helpers” instead of the primary product path.
3. Provide a deprecation warning in docs and CLI output.
4. Preserve generated repo support for at least one release cycle.
5. Add upgrade docs showing how to move from generated-static to runtime-generic usage.

### Acceptance criteria

- Existing generated CLI repos continue to work.
- The primary path forward is clearly documented as generic runtime mode.
- Users understand the difference between compatibility mode and the default product.

---

## Phase 6 — docs, examples, and release messaging

### Goal

Update the project narrative so the simplified model is explicit and easy to adopt.

### Tasks

1. Rewrite README and getting-started guidance to describe the generic runtime model as default.
2. Add examples for `--site goat`, `--site boat`, and env-based configuration.
3. Add a migration note for older generated projects.
4. Publish a short architecture note explaining why runtime schema is preferred over build-time generated field maps.

### Acceptance criteria

- Documentation reflects the simplified runtime design.
- New users can start with a preset and no custom generator workflow.
- The project story no longer implies that every site requires its own distribution.

---

## Priority order

### P0 (must ship first)

- runtime preset registry
- `--site` and `GENOMEHUBS_SITE` support
- `--api-base` override and env support
- runtime metadata catalog
- compatibility mode for existing generated repos

### P1

- generic Python/R/JS SDK parity
- dynamic validation and field discovery
- metadata refresh flow

### P2

- template simplification
- docs refresh
- long-term deprecation of static site generation

---

## Suggested implementation sequence

1. Add preset registry and selector parsing.
2. Add runtime metadata fetch/cache.
3. Switch generic CLI to runtime schema mode.
4. Update SDK builders to use runtime metadata.
5. Simplify templates and reduce Tera logic.
6. Defer static generator mode to compatibility only.

---

## File-by-file implementation checklist

This checklist is designed to be used as the first implementation pass. It is intentionally scoped to the files that will change in the generic runtime refactor.

### 1. Runtime configuration and preset resolution

- src/core/config.rs
  - Add `SitePreset` and `RuntimeConfig` types.
  - Move preset definitions into a runtime config registry rather than generation-only YAML assumptions.
  - Keep the existing site YAML support for compatibility, but treat it as bootstrap metadata only.
  - Add `resolved_api_base()`, `resolved_ui_base()`, and `resolved_site_name()` helpers.
  - Add environment-variable and CLI-precedence resolution helpers.

- src/core/cli_runtime.rs (new)
  - Create a dedicated runtime config loader for `--site`, `--api-base`, and env vars.
  - Centralize preset resolution and precedence logic in one place.
  - Keep CLI parsing independent from code generation.

- config/site-presets.yaml (new)
  - Add the default preset registry for goat, boat, and other primary public instances.
  - Include `api_base`, `ui_base`, `default_index`, and `description` for each site.
  - Keep this file human-edited and easy to extend.

### 2. CLI entry points and compatibility behavior

- src/main.rs
  - Add generic CLI flags: `--site`, `--api-base`, `--refresh-metadata`, `--metadata-cache-dir`.
  - Keep compatibility flags for the old generated-site workflow but ensure they are secondary to the generic path.
  - Add env-var support and clear precedence rules.

- src/commands/new.rs
  - Treat `new` as a bootstrap/package helper, not the default product path.
  - Keep generated output working for compatibility.
  - Add a deprecation warning or docs note when used as primary flow.

- src/commands/update.rs
  - Do the same: compatibility-first, not default-path-first.
  - Ensure update logic still works for legacy generated repos.

- src/commands/preview.rs
  - Add preview output for runtime config and preset resolution.
  - Preview should clearly show resolved `api_base` and metadata source.

### 3. Runtime metadata catalog and sync layer

- src/core/runtime/schema.rs (new)
  - Define the canonical metadata model for indexes, fields, validation rules, and optional site metadata.
  - Keep it independent from the generator’s YAML config model.

- src/core/runtime/client.rs (new)
  - Add a generic API client using `api_base` from runtime config.
  - Support metadata fetches for `fields`, `indices`, `summary`, and other schema endpoints.

- src/core/runtime/cache.rs (new)
  - Add TTL cache, refresh policy, and stale-check logic.
  - Public API: `get_schema()`, `refresh_schema()`, `invalidate_cache()`.

- src/core/runtime/validation.rs (new)
  - Replace generated field tables for runtime validation.
  - Validate the user query against live metadata rather than build-time assumptions.

### 4. Generic CLI command implementation

- src/core/command/search.rs (new or refactored)
  - Remove site-specific assumptions from the command model.
  - Resolve index and field names via runtime metadata at execution time.

- src/core/command/count.rs (new or refactored)
  - Generic count/search semantics without generated site-specific wrappers.

- src/core/command/lookup.rs (new or refactored)
  - Generic lookup logic using runtime metadata and loaded site config.

- src/core/command/describe.rs (new or refactored)
  - `describe()` should operate from runtime schema instead of generator output.

- src/core/command/metadata.rs (new or refactored)
  - Add a discoverable command to print dynamic metadata and simplify debug workflows.

### 5. SDK runtime layer

- python/cli_generator/query.py
  - Make `QueryBuilder` accept `api_base` and optional `site` config.
  - Load metadata lazily for validation and field suggestions.
  - Support `refresh_metadata()` and `describe()` from runtime schema.

- python/cli_generator/sdk_config.py (new)
  - Add a runtime config and preset resolution helper for Python.
  - Apply the same precedence rules as the Rust CLI.

- python/cli_generator/**init**.py
  - Re-export the generic runtime-configured builders with simple defaults.

- templates/python/query.py.tera
  - Shrink this file to a thin package wrapper rather than the source of business logic.
  - Remove generated field-group logic from the default path.

### 6. R and JavaScript wrappers

- templates/r/query.R
  - Rework to resolve runtime API base and metadata instead of per-site generated values.

- templates/js/query.js
  - Move the same runtime pattern into JS, with preset support for the common site names.

- templates/r/lib.rs.tera
  - Keep package wiring only; no site logic should be embedded here.

- templates/js/package.json.tera / js wrapper files
  - Ensure runtime config and preset support are documented and easy to discover.

### 7. Template cleanup and source-of-truth migration

- templates/rust/main.rs.tera
  - Remove large generated logic blocks and leave only bootstrap wiring.

- templates/rust/client.rs.tera
  - Reduce generated client logic to a thin adapter around the real runtime client.

- templates/rust/fields.rs.tera
  - Remove static field tables from the default path or keep them only as an optional cached fallback.

- templates/rust/cli_flags.rs.tera
  - Treat this as a compatibility helper only, not the main user path.

- templates/rust/generated_mod.rs.tera
  - Keep generated exports but make them secondary to runtime-catalog logic.

### 8. Tests that must be added or rewritten

- tests/python/test_runtime_config.py (new)
  - Verify `--site`, env vars, and explicit `--api-base` precedence.

- tests/python/test_runtime_metadata.py (new)
  - Verify runtime metadata load, cache refresh, and missing-field behaviour.

- tests/python/test_sdk_parity.py
  - Ensure Python/R/JS wrappers still behave the same when pointing at generic runtime config.

- tests/r/test_runtime_config.R (new)
  - Validate preset resolution in R.

- tests/javascript/test_runtime_config.mjs (new)
  - Validate preset resolution in JavaScript.

- tests/python/test_compatibility_generators.py
  - Confirm the old generated-site path still works, but only in compatibility mode.

### 9. Documentation updates

- README.md
  - Reframe the project around the generic runtime model.
  - Add examples for `--site goat`, `--site boat`, `--api-base`, and env vars.

- GETTING_STARTED.md
  - Add a “quick start with preset site” section.
  - Add a “runtime metadata + generic client” section.

- docs/refactor/generic-runtime-refactor-backlog.md
  - Keep this as the canonical plan and update it as milestones are completed.

- docs/planning/integration-runbook.md
  - Mark the static generated-site route as compatibility mode, not the default.

### 10. Decommissioning milestones

- Add a deprecation warning in the `new` and `update` command output.
- Add user-facing docs noting that the generic runtime CLI is preferred.
- Add a release note explaining the migration from site-specific generated packages to runtime metadata driven clients.

## Implementation order for the first pass

1. Add preset registry and runtime config resolution.
2. Add metadata cache and fetch layer.
3. Switch the CLI to runtime-configured mode.
4. Update Python SDK builder and validation semantics.
5. Refactor R/JS wrapper usage to the same runtime-configured path.
6. Simplify templates and remove logic from Tera as the final cleanup step.

## Exit criteria for the refactor

The refactor is complete when all of the following are true:

- one generic CLI works against multiple public instances without rebuilds;
- one generic SDK path works across instances without site-specific generation;
- field and index availability come from runtime metadata;
- the convenience preset layer works for goat, boat, and any additional supported instance;
- the `new` and `update` commands are compatibility helpers rather than the core delivery path;
- templates are primarily package scaffolding, not product logic.

## Key architectural decision

The generic runtime model should win by default, even for known public sites. The convenience preset layer is not a special code path; it is just a user-friendly alias for a real runtime configuration.

That keeps the project scalable without creating a new site-specific package for every deployment.
