# Phase 6g: Quarto Documentation Updates

**Depends on:** Phase 6c (ReportBuilder), Phase 6d (CLI subcommands), Phase 6e (Python gaps), Phase 6f (R/JS gaps)
**Blocks:** Phase 6h (test parity — the Quarto parity tests check that all canonical methods are documented)
**Scope:** `templates/docs/reference/query-builder.qmd.tera`, `templates/docs/reference/cli.qmd.tera`, `templates/docs/quickstart.qmd.tera`

---

## Motivation

The Quarto reference documentation has three gaps:

1. **Language coverage:** Language tabs only show Python, R, JavaScript. Direct API calls (raw HTTP with `curl` or any HTTP client) are not documented. Adding an `API` tab alongside the three SDK tabs gives a language-agnostic view of the v3 interface — valuable for users who want to use `curl`, a custom client, or understand exactly what the SDK is doing.

2. **Method coverage:** Several new methods from phases 6b–6f are not in the template: `to_v2_url()`, `from_v2_url()`, `report()`, `search_all()` (R was missing), `count_batch()`, `search_batch()`, `ReportBuilder` class.

3. **Accuracy:** `to_url()` is now deprecated; `search_all()` description mentions `/searchPaginated` which does not exist in v3; `count()` description doesn't reflect the dedicated `/count` endpoint.

---

## Work Items

### 1. Add `API` tab to all language toggle blocks

**File:** `templates/docs/reference/query-builder.qmd.tera`

The Quarto panel-tabset group is `group="language"`. Adding a fourth tab `## API` to each tabset block shows the equivalent raw HTTP call. The tab order will be: Python → R → JavaScript → API.

**Pattern for a POST endpoint:**

````markdown
## API

```bash
curl -s -X POST {{ api_base }}/v3/search \
  -H "Content-Type: application/json" \
  -d '{
    "query_yaml": "...",
    "params_yaml": "..."
  }'
```
````

**Pattern for a GET endpoint (record, lookup, summary):**

````markdown
## API

```bash
curl -s "{{ api_base }}/v3/record?recordId=GCA_000001405.15&result=assembly"
```
````

The API tab shows:

- The endpoint URL (using the baked-in `{{ api_base }}` Tera variable)
- The HTTP method
- The JSON body shape for POST endpoints
- Where `query_yaml` comes from — reference to `to_query_yaml()` output

**Sections that need API tabs added:**

| Section                | Endpoint            | Method                                 |
| ---------------------- | ------------------- | -------------------------------------- |
| Constructor (overview) | —                   | — (no API tab; introduces the concept) |
| `count()`              | `/v3/count`         | POST                                   |
| `search()`             | `/v3/search`        | POST                                   |
| `search_all()`         | `/v3/search` (loop) | POST                                   |
| `search_batch()`       | `/v3/searchBatch`   | POST                                   |
| `count_batch()`        | `/v3/countBatch`    | POST                                   |
| `record()`             | `/v3/record`        | GET                                    |
| `lookup()`             | `/v3/lookup`        | GET                                    |
| `summary()`            | `/v3/summary`       | GET                                    |
| `report()`             | `/v3/report`        | POST                                   |

Setter methods (`set_taxa`, `add_attribute`, etc.) do not need API tabs — they are SDK constructs with no direct API equivalent. The API tab is meaningful only for methods that make network calls.

---

### 2. Fix and update method signatures

**File:** `templates/docs/reference/query-builder.qmd.tera`

| Current                                                   | Fix                                          |
| --------------------------------------------------------- | -------------------------------------------- |
| `to_url(api_base, api_version, endpoint="search") -> str` | Mark as deprecated; add `to_v2_url()` entry  |
| `search_all()` description mentions `/searchPaginated`    | Update to cursor-based v3 pagination         |
| `count()` description references a workaround             | Update: now uses dedicated `/count` endpoint |

**New method sections to add:**

- `to_v2_url(endpoint="search") -> str` — replaces `to_url()` for the v2 URL-building path
- `from_v2_url(url) -> QueryBuilder` — class method for reconstructing from a v2 URL
- `report(report_type_or_builder, ...) -> dict` — with full parameter table
- `ReportBuilder` class — new section after `QueryBuilder` reference
- `search_all()` R tab (was missing — only Python and JavaScript had examples)

---

### 3. `ReportBuilder` section

Add a new top-level section `## ReportBuilder` below the `QueryBuilder` section.

Structure:

```markdown
## ReportBuilder

`ReportBuilder` constructs the report configuration for `/v3/report` queries.
It is designed to be paired with a `QueryBuilder`:

    rb = ReportBuilder("histogram").set_x("genome_size").set_rank("species")
    data = qb.report(rb)

### Constructor

### Axis setters

### Common config setters

### Type-specific config setters

### `to_report_yaml() -> str`

### `validate(field_metadata=None) -> list[str]`

### `run(query_builder) -> dict`
```

Each setter section has Python / R / JavaScript / API tabs. The API tab shows the relevant `report_yaml` YAML snippet.

---

### 4. CLI reference — add new subcommands

**File:** `templates/docs/reference/cli.qmd.tera`

Add reference sections for the new subcommands added in phase 6d:

- `<index> record <ID>` — options: `--format`
- `<index> summary <ID>` — options: `--fields`, `--summary-types`
- `<index> report <TYPE>` — full option table (x, y, cat, rank, and all query flags)
- `<index> count` — updated to document the full set of attribute flags (now matching `search`)
- `search-batch <FILE>` (top-level)
- `count-batch <FILE>` (top-level)

---

### 5. Quickstart — add v3 and API examples

**File:** `templates/docs/quickstart.qmd.tera`

The quickstart currently shows only the `search` path. Add:

1. A "Using the API directly" section showing a `curl` POST to `/v3/search` — useful for users who want to understand the underlying API or use a language not covered by the SDKs
2. A `report()` example (histogram)
3. Update the `count()` example to reflect v3 (currently shows v2 URL pattern)

---

### 6. Deprecation callout for `to_url()`

Every documentation page that shows `to_url()` should have a callout block:

```markdown
::: {.callout-warning}

## Deprecated

`to_url()` is deprecated. Use `to_v2_url()` to build a v2 GET URL,
or use `to_query_yaml()` + `to_params_yaml()` to construct a v3 POST body.
:::
```

---

## Test parity (consumed by phase 6h)

The existing `test_no_extra_methods_in_python` test checks that documented methods match implemented methods. After this phase, the Quarto parity tests (`TestDocumentationParity`) will check that:

- All canonical methods appear in the Quarto reference
- `ReportBuilder` methods are documented
- Deprecated `to_url()` is documented with a deprecation marker
- The API tab is present for all network-call methods

---

## Ordering

1. Fix existing inaccuracies in `query-builder.qmd.tera` (`to_url()` deprecation, `search_all()` description, `count()` description)
2. Add `to_v2_url()` and `from_v2_url()` method sections
3. Add `report()` method section (all language tabs + API tab)
4. Add `ReportBuilder` class section
5. Add API tab to all existing network-call sections
6. Add R tab to `search_all()` section
7. Update `cli.qmd.tera` with new subcommand sections
8. Update `quickstart.qmd.tera` with v3 and API examples
9. Regenerate test workdir: `bash scripts/dev_site.sh --no-rebuild-wasm goat`
10. Verify Quarto builds: `cd workdir/goat-test-cli && quarto render docs/`

---

## Notes on Quarto panel-tabset syntax

The `group="language"` attribute on panel-tabsets means that selecting "Python" in one section automatically switches all other sections to "Python". Adding a fourth tab `## API` will be shown when the user selects it in any section. The tab label should be `## API` (plain, no "HTTP" prefix) to keep it concise. Using `## API` rather than `## curl` makes it clear it documents the API contract, not a specific HTTP client.
