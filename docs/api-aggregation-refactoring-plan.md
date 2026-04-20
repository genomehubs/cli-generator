# API Aggregation Refactoring & SDK-Driven Architecture

**Status:** PLANNING COMPLETE (2026-04-20)
**Target Start:** Week 1 (MVP Checkpoint: Week 10)
**Total Duration:** 19-22 weeks (Phases 1-5)

---

## Overview

This document describes a comprehensive **5-phase strategy** to refactor the genomehubs ecosystem, with cli-generator as the foundational data layer. We will replace scattered, imperative Elasticsearch aggregation logic with a type-safe Rust DSL, expose it to JavaScript/Python via FFI, refactor the API to call SDK functions, and add async job handling for long-running reports.

**Key goals:**

- Single source of truth (SDK) for query validation, aggregation building, and response shaping
- Type-safe aggregation engine (Rust) replacing imperative setAggs/getBounds logic
- Async job queue for reports >50k docs complexity
- Backward-compatible transition from v2 → v3 API (feature flag, v2 compat shim)
- Address circular FieldFetcher dependency (pre-MVP workaround, Phase 6 decoupling)

---

## Problem Analysis

### Current Architecture Pain Points

#### API (genomehubs-api)

**Aggregation logic scattered across multiple files:**

- [setAggs.js](../../genomehubs/genomehubs/src/genomehubs-api/src/api/v2/reports/setAggs.js) — Imperative nested Elasticsearch aggregation DSL building
- [getBounds.js](../../genomehubs/genomehubs/src/genomehubs-api/src/api/v2/reports/getBounds.js) — Scale detection, numeric/categorical bounds calculation
- [histogramAgg.js](../../genomehubs/genomehubs/src/genomehubs-api/src/api/v2/queries/histogramAgg.js) — Interval calculation, scale function inverse (log, sqrt)
- [processHits.js](../../genomehubs/genomehubs/src/genomehubs-api/src/api/v2/functions/processHits.js) — Post-processing, bucket labeling, field extraction

**Issues:**

- Logic is imperative, hard to reason about and test
- No type safety; errors caught at ES query time, not build time
- Bounds calculation and binning logic mixed with ES query building
- 11 report types hard-coded in switch statement (poor extensibility)
- No async job handling; all reports computed synchronously (single-threaded bottleneck)
- Response format complex, requires further reshaping in UI

#### UI (genomehubs-ui)

**Data reshaping logic in selectors/components:**

- [processReport](../../genomehubs/genomehubs/src/genomehubs-ui/src/client/views/selectors/report.js#L905) — Transforms aggregation structure into visualization-ready format
- [processScatter](../../genomehubs/genomehubs/src/genomehubs-ui/src/client/views/selectors/report.js#L325) — Bucket → chart data transformation, jitter application, scale transforms

**Issues:**

- Duplicate query validation logic (also exists in API)
- Deep nesting without validation (try/catch with silent fallbacks)
- Unbounded selector caching
- Scale transforms (log, sqrt, ordinal) happen in UI (could be in SDK)
- Jitter applied incorrectly in some cases
- Fragile contract with API (expects specific aggregation nesting)

#### blobtoolkit

**Validation patterns that could be reused:**

- Field metadata handling (type system, constraints)
- Multi-format parsing (BUSCO, BAM, TSV)
- Taxonomy matching and normalization

**Opportunity:** Extract common validation/field logic into shared library (post-MVP consideration).

### Current Constraints

#### FieldFetcher Circular Dependency

**The issue:**

- CLI generation (`new`, `preview`, `update` commands) depends on FieldFetcher
- FieldFetcher calls the live API to fetch field metadata (`GET /api/v2/report/types`)
- Field metadata stored in static YAML configs but requires ES + existing resultFields logic to enrich
- **New genomehubs instances can't run `cli-generator new` until API is already running** → circular dependency

**Pre-MVP solution:** Accept as precondition, provide bootstrap workflow

1. Provide `docker-compose.yml` starter kit (minimal ES + genomehubs-api)
2. Users start API first
3. Then run `cli-generator new`

**Post-MVP solution (Phase 6):** Decouple FieldFetcher

1. Extract field metadata logic into standalone library ([cli-generator/src/core/field_metadata.rs](src/core/field_metadata.rs))
2. Offline-first mode: `cli-generator new --offline` (no API dependency)
3. Lazy enrichment: `cli-generator preview --enrich` (fetches live metadata after API is up)

---

## Strategic Decisions

| #   | Decision                                   | Rationale                                                                                                        |
| --- | ------------------------------------------ | ---------------------------------------------------------------------------------------------------------------- |
| 1   | **SDK as source of truth**                 | API calls SDK internally for query validation, aggregation building, response shaping. Single source of truth.   |
| 2   | **Async report jobs**                      | PostgreSQL + Bull queue. Progress feedback via polling (not WebSocket). For reports >50k docs.                   |
| 3   | **Declarative aggregation engine**         | Rust enums + structs, auto-generated JSON Schema. Replaces imperative ES DSL.                                    |
| 4   | **No aggregation plugins**                 | New report types fully integrated into aggregation_v3 module (not plugin system).                                |
| 5   | **Tree follows same pattern**              | Tree report uses same aggregation/bounds/processor pattern as histogram, scatter, etc.                           |
| 6   | **Error handling: inline (200)**           | Return HTTP 200 + error message in response body (matches v2 contract). Empty bounds OK if no results.           |
| 7   | **Redis caching (daily flush)**            | Cache valid across day (matches index rebuild cycle). Flushed at rebuild. Preload common/example reports.        |
| 8   | **Local file paths (dev)**                 | Use `"cli-generator": "file:../cli-generator"` in package.json during dev. Defer npm publish until near MVP.     |
| 9   | **Code location: cli-generator/src/core/** | Aggregation engine, SDK processors, and shared validation in cli-generator (reusable across repos).              |
| 10  | **Backward compat: relaxed**               | Plan breaking API. Encourage SDK adoption. Keep /api/v2 in separate container post-MVP.                          |
| 11  | **MVP scope: full parity**                 | Target all 11 report types (histogram, scatter, tree, map, oxford, arc, xPerRank, sources, files, types, table). |

---

## 5-Phase Implementation Roadmap

### Phase 1: Rust Aggregation Engine (6-8 weeks)

**Goal:** Create type-safe, declarative aggregation DSL in Rust. Replace imperative nested ES queries from API.

**Deliverables:**

Create [cli-generator/src/core/aggregation_v3/](src/core/aggregation_v3/) with:

- **dsl.rs** — Core types
  - `ReportType` enum (Histogram, Scatter, Arc, Tree, Map, Oxford, xPerRank, Sources, Files, Types, Table)
  - `HistogramRequest { field, scale, min_count, category_field }`, `ScatterRequest { x_field, y_field, ... }`, etc.
  - `AggregationQuery` wrapper containing report type + config

- **bounds.rs** — Scale detection and bounds calculation
  - `determine_scale(field_metadata) → Scale` (infer linear/log2/ordinal)
  - `calculate_numeric_bounds(field, es_agg_response)` → min/max/min_display/max_display
  - `fetch_categorical_bounds(field, opts)` → top-N terms

- **histogram.rs, scatter.rs, arc.rs, tree.rs, ...** — Report-type builders
  - `histogram::build_agg(config) → serde_json::Value` (ES DSL JSON)
  - Direct ports from existing [setAggs.js](../../genomehubs/genomehubs/src/genomehubs-api/src/api/v2/reports/setAggs.js) + [histogramAgg.js](../../genomehubs/genomehubs/src/genomehubs-api/src/api/v2/queries/histogramAgg.js)

- **processor.rs** — Post-process ES responses
  - `process_histogram_response(es_response, config) → HistogramReport`
  - Extract buckets, apply scales, categorize

- **lib.rs** — Public API
  - `pub fn build_es_agg(query: &AggregationQuery) → serde_json::Value`
  - `pub fn process_response(es_response: &Value, query: &AggregationQuery) → ProcessedReport`

- **tests/** — Comprehensive test suite
  - Unit tests (bounds calc, scale functions)
  - Property tests (scale roundtrip correctness via proptest)
  - Integration tests (mock ES responses → verify processor output)

**Success Criteria:**

- All 11 report types have aggregation builders
- Bounds calc output matches v2 API ±2%
- JSON Schema generation succeeds (via `schemars` crate)
- ≥90% test coverage
- No clippy warnings

---

### Phase 2: SDK FFI + Report Processors (4-6 weeks)

**Goal:** Expose Rust aggregation to Node.js/Python. Add response shaping layer.

**Deliverables:**

**Rust FFI** — [cli-generator/src/lib.rs](src/lib.rs)

```rust
#[pyfunction]
pub fn build_report_aggregation(query_json: &str) -> PyResult<String> { ... }

#[pyfunction]
pub fn process_report_response(es_response: &str, config: &str) -> PyResult<String> { ... }
```

**SDK Report Builders** — [cli-generator/python/cli_generator/report_v3.py](python/cli_generator/report_v3.py)

```python
class HistogramBuilder:
    def __init__(self, field, scale="linear", category_field=None, ...):
        ...
    def validate(self) -> bool:  # Check against JSON Schema
        ...
    def to_dict(self) -> dict:  # { esAgg, processingConfig }
        ...
```

**Response Shaping** — [cli-generator/python/cli_generator/response_shaper.py](python/cli_generator/response_shaper.py)

```python
def shape_histogram(es_response: dict, config: dict) -> dict:
    # Unpack nested aggs → clean v3 format
    ...
```

**6 Touch-Points (from [AGENTS.md](../../AGENTS.md#checklist-for-adding-a-new-pyo3-function)):**

1. [src/lib.rs](src/lib.rs) — `#[pyfunction]` + `#[pymodule]` registration
2. [templates/rust/lib.rs.tera](templates/rust/lib.rs.tera) — Mirror in generated projects
3. [src/commands/new.rs → copy_embedded_modules()](src/commands/new.rs) — Copy aggregation_v3 files
4. [src/commands/new.rs → required_deps](src/commands/new.rs) — Add new Cargo dependencies
5. [src/commands/new.rs → patch_python_init()](src/commands/new.rs) — Export in generated **init**.py
6. [templates/python/query.py.tera](templates/python/query.py.tera) — Sync method signatures

**Tests** — [tests/python/test_report_v3.py](tests/python/test_report_v3.py)

- Schema validation (valid + invalid inputs)
- Response shaping (mock ES → verify v3 format)
- Rust FFI callable from Python/JS

**Success Criteria:**

- `maturin develop --features extension-module` builds
- `pytest tests/python/test_report_v3.py -v` passes
- HistogramBuilder output matches Phase 1 aggregation DSL
- Callable from Node.js and Python

---

### Phase 3: API v3 Endpoints (4-6 weeks)

**Goal:** Create new v3 routes calling SDK. Replace query building with SDK calls.

**Deliverables:**

**New v3 Routes** — [genomehubs-api/src/api/v3/routes/report.js](../../genomehubs/genomehubs/src/genomehubs-api/src/api/v3/routes/report.js)

```javascript
GET /api/v3/report?report=histogram&x=field&category=status&scale=log2

Handler:
1. Parse query params → validate against JSON Schema (from SDK)
2. SDK: { esAgg, config } = await sdk.buildReportAgg(query)
3. ES: esResponse = await getResults({ aggs: esAgg })
4. SDK: shaped = await sdk.shapeReportResponse(esResponse, config)
5. Return { status, report: shaped }
```

**Query Validation** — [genomehubs-api/src/api/v3/queryParams.js](../../genomehubs/genomehubs/src/genomehubs-api/src/api/v3/queryParams.js)

- Parse URL params → AggregationQuery struct
- Validate against JSON Schema (early error detection)

**Route Dispatcher** — [genomehubs-api/src/api/v3/index.js](../../genomehubs/genomehubs/src/genomehubs-api/src/api/v3/index.js)

- Mount v3 routes for MVP types (histogram, scatter, arc)
- Others use v2 compat shim

**Modifications to v2 API:**

- Remove calls to setAggs, getBounds (now in SDK)
- Simplify processHits (SDK handles more)
- Keep getResults, connection.js (still needed for ES)

**Integration Tests** — [genomehubs-api/tests/api/v3/](../../genomehubs/genomehubs/src/genomehubs-api/tests/api/v3/)

- Compare v2 vs. v3 output on real ES (goat.yaml, boat.yaml)
- Verify aggregations, bounds, categories match ±2%
- Test error cases, fallback to compat shim

**Success Criteria:**

- `/api/v3/report` endpoints return correct aggregations
- Response format improves on or matches v2
- Tests pass with real Elasticsearch
- v2 compat shim works for non-MVP types

---

### Phase 4: UI Migration (6-8 weeks)

**Goal:** Update UI to consume v3 responses. Gradual rollout via feature flag.

**Deliverables:**

**Feature Flag** — [genomehubs-ui/src/client/views/store.jsx](../../genomehubs/genomehubs/src/genomehubs-ui/src/client/views/store.jsx)

- Add `apiVersion: "v2" | "v3"` to Redux state

**New v3 Selector** — [genomehubs-ui/src/client/views/selectors/report.js](../../genomehubs/genomehubs/src/genomehubs-ui/src/client/views/selectors/report.js)

- `getReportByIdV3(state, reportId)` — processes v3 response format
- Runs parallel to existing `getReportByReportId()` (v2)
- Components route based on flag

**Simplified Components** — [genomehubs-ui/src/client/views/components/Report\*.jsx](../../genomehubs/genomehubs/src/genomehubs-ui/src/client/views/components/)

- Move scale transforms to SDK
- UI only renders; no more `applyJitter()`, scale functions
- Use pre-jittered data from API response

**Rollout Timeline:**

1. Week 7-8: Feature flag defaults to "v2" (hidden)
2. Week 9-10: Set to "v3" for histogram, scatter, arc (real-world testing)
3. Week 11-12: Global default to "v3" (fallback to v2 if broken)
4. Week 13+: All 11 report types, remove v2 compat

**Success Criteria:**

- v3 reports render identically to v2 (visual regression)
- Feature flag toggle works
- No unbounded selector cache growth
- All 11 report types available

---

### Phase 5: Async Job Queue (4-6 weeks)

**Goal:** Add background workers for long-running reports. Progress feedback via polling.

**Deliverables:**

**PostgreSQL Schema** — [genomehubs-api/src/db/migrations/001_create_report_jobs.sql](../../genomehubs/genomehubs/src/genomehubs-api/src/db/migrations/001_create_report_jobs.sql)

```sql
CREATE TABLE report_jobs (
  id UUID PRIMARY KEY,
  query_id VARCHAR(40),
  report_type VARCHAR(50),
  status VARCHAR(20),      -- queued, processing, ready, failed
  progress INT DEFAULT 0,  -- 0-100
  result_path TEXT,
  error TEXT,
  created_at, updated_at, completed_at TIMESTAMP
);
```

**Job Queue** — [genomehubs-api/src/api/v3/jobs/reportQueue.js](../../genomehubs/genomehubs/src/genomehubs-api/src/api/v3/jobs/reportQueue.js)

- Bull queue (Redis or in-memory)
- `enqueueReport(jobData) → jobId`

**Worker** — [genomehubs-api/src/api/v3/jobs/reportWorker.js](../../genomehubs/genomehubs/src/genomehubs-api/src/api/v3/jobs/reportWorker.js)

- Consume jobs from queue
- SDK: build agg, execute ES query
- Update progress every 10% or 5s
- Save result to disk, update job record

**API Integration** — [genomehubs-api/src/api/v3/routes/report.js](../../genomehubs/genomehubs/src/genomehubs-api/src/api/v3/routes/report.js)

- Estimate complexity: `reportComplexity(config) → size`
- If >threshold: enqueue (return jobId) else sync
- `GET /api/v3/jobs/{jobId}` — status + progress
- `GET /api/v3/jobs/{jobId}/result` — download once ready

**UI Integration:**

- Detect `{ status: "queued", jobId }`
- Show progress bar
- Poll every 2 seconds
- Fetch + render once ready

**Success Criteria:**

- Large report (100k-taxon tree) queues + processes
- Progress feedback every 2-5 seconds
- Result matches sync rendering
- Worker crash recovery works
- Tests: queuing, processing, completion, errors

---

### Phase 6: Decouple FieldFetcher (Post-MVP, 6-8 weeks)

**Goal:** Enable offline-first CLI generation. Remove API dependency from `new` command.

**Deliverables:**

**Field Metadata Library** — [cli-generator/src/core/field_metadata.rs](src/core/field_metadata.rs)

- Parse YAML configs without ES/API
- Build field registry from static data
- JSON Schema generation (matches aggregation DSL)

**Offline-First Mode** — [cli-generator/src/commands/new.rs](src/commands/new.rs)

- `--offline` flag: skip FieldFetcher, generate scaffold
- Works without running API

**Lazy Enrichment** (optional) — [cli-generator/src/commands/preview.rs](src/commands/preview.rs)

- `preview --enrich` after API is live
- Fetches live field metadata for interactive discovery

**Result:** SDK/CLI fully decoupled from API. API becomes optional for advanced features.

---

## Verification & Testing

| Phase | Verification                                   | Success Criteria                                     |
| ----- | ---------------------------------------------- | ---------------------------------------------------- |
| **1** | `cargo test --lib aggregation_v3`              | All tests pass, ≥90% coverage, no clippy warnings    |
| **2** | `maturin develop` + `pytest test_report_v3.py` | Builds, imports work, schema valid                   |
| **3** | Integration tests: v3 vs. v2 output            | Aggregations match ±2%, tests pass                   |
| **4** | Selector tests + visual regression             | v3 renders identical, feature flag works             |
| **5** | Job queue + E2E UI test                        | Job queuing, progress, result render; crash recovery |

---

## Timeline & Milestones

```
Week 1-8:    Phase 1 (Aggregation Engine)
       ↓ (can overlap)
Week 3-9:    Phase 2 (SDK FFI + Report Builders)
       ↓ (phase 2 must complete first)
Week 5-11:   Phase 3 (API v3 Endpoints)
       ↓
Week 10:     🎯 MVP CHECKPOINT
             • Phase 1 complete (all 11 report types)
             • Phase 2 complete (FFI + SDK processors)
             • Phase 3 complete (v3 endpoints for hist/scatter/arc)
             • MVP reports live, other report types use v2 compat
       ↓ (phase 3 must be live first)
Week 7-15:   Phase 4 (UI Migration)
       ↓ (can overlap with phase 4)
Week 11-17:  Phase 5 (Async Job Queue)

Post-MVP (when cores stabilize):
       ↓
Phase 6:     Decouple FieldFetcher (~6-8w)
```

**Total MVP duration:** 19-22 weeks (including 2-3w contingency for integration testing and unexpected issues).

---

## Risk Mitigation

| Risk                                                      | Impact              | Mitigation                                                                                   |
| --------------------------------------------------------- | ------------------- | -------------------------------------------------------------------------------------------- |
| Rust DSL doesn't match API output                         | Phase 1 blocked     | Extensive unit + integration tests; 2% tolerance acceptable                                  |
| FFI breaks generated projects                             | Phase 2 integration | Follow all 6 touch-points from AGENTS.md; test with `bash scripts/dev_site.sh --python goat` |
| UI rendering breaks after migration                       | Phase 4 blocked     | Feature flag allows fallback; gradual rollout strategy                                       |
| Async job queue adds operational complexity               | Phase 5 stability   | Start simple (polling + disk); test crash recovery; no WebSocket yet                         |
| Database schema migration issues                          | Phase 5 rollout     | Use reversible migrations; test on staging first; can roll back                              |
| **FieldFetcher circular dependency blocks new instances** | Pre-MVP blocker     | Accept as precondition; provide docker-compose bootstrap kit; Phase 6 decoupling post-MVP    |
| Scope creep (all 11 report types)                         | MVP delayed         | Prioritize: histogram → scatter → arc first; others use v2 compat during transition          |

---

## Critical Dependencies & Integration Points

### 1. Rust-to-JS-to-Python Chain (Phase 2 Critical)

Must follow all 6 touch-points from [AGENTS.md](../../AGENTS.md#checklist-for-adding-a-new-pyo3-function):

- Missing any touch-point → ImportError or NameError in generated projects
- Template → verify `scripts/dev_site.sh --python goat` succeeds

### 2. API-to-SDK Dependency (Phase 3 Critical)

- API calls `await sdk.buildReportAgg()` after Phase 2 complete
- Use local file path during dev: `"cli-generator": "file:../cli-generator"` in genomehubs-api/package.json
- Defer npm publish until near MVP (avoid version management headache early)

### 3. UI-to-API Contract (Phase 4 Critical)

- UI selectors expect v3 response format (cleaner than v2)
- Feature flag allows v2/v3 switching (safe A/B test)
- v2 compat shim at API level (transparent to UI code)

### 4. FieldFetcher Bootstrap (Pre-MVP)

- New instances require running API before `cli-generator new`
- Provide docker-compose.yml starter kit
- Document workflow clearly in README
- Phase 6 (post-MVP) enables offline-first generation

---

## Success Metrics

**Phase 1:** Aggregation engine complete, all 11 report types, ≥90% tests, zero clippy warnings
**Phase 2:** FFI bindings work from Node.js and Python, schema validation correct
**Phase 3:** v3 endpoints live, v2 vs. v3 output parity ±2%, no regressions
**MVP Checkpoint:** Phases 1+2+3 complete; histogram, scatter, arc reports live; UI can toggle to v3
**Phase 4:** UI fully migrated, feature flag works, visual regression tests pass
**Phase 5:** Job queue processing reports, progress feedback working, crash recovery tested
**Phase 6 (Post-MVP):** Offline-first CLI generation, FieldFetcher decoupled from API

---

## File Structure (by Phase)

### Phase 1 (New Files)

```
cli-generator/src/core/aggregation_v3/
├── lib.rs
├── dsl.rs
├── bounds.rs
├── histogram.rs
├── scatter.rs
├── arc.rs
├── tree.rs
├── [map.rs, oxford.rs, xperrank.rs, sources.rs, files.rs, types.rs, table.rs]
├── processor.rs
└── tests/
    ├── bounds_tests.rs
    ├── histogram_tests.rs
    └── integration_tests.rs
```

### Phase 2 (New Files)

```
cli-generator/python/cli_generator/
├── report_v3.py
└── response_shaper.py

cli-generator/tests/python/
└── test_report_v3.py
```

### Phase 3 (New Files)

```
genomehubs-api/src/api/v3/
├── index.js
├── routes/
│   └── report.js
├── queryParams.js
└── jobs/
    ├── reportQueue.js
    └── reportWorker.js

genomehubs-api/src/db/migrations/
└── 001_create_report_jobs.sql

genomehubs-api/tests/api/v3/
└── [...integration tests]
```

### Phase 4 (Modifications)

```
genomehubs-ui/src/client/views/
├── selectors/report.js (add getReportByIdV3)
├── reducers/report.js (handle v3 format)
├── store.jsx (add apiVersion flag)
└── components/Report*.jsx (simplify)
```

### Phase 5 (Modifications)

```
genomehubs-api/src/api/v3/
├── routes/report.js (add job enqueueing)
└── […job queue files from Phase 3]

genomehubs-api/src/app.js (start worker)
```

---

## Appendix: Key Files & Functions to Refactor

### From API (to move to aggregation_v3)

| File                                                                                                 | Function                                         | Maps To (Phase 1)            |
| ---------------------------------------------------------------------------------------------------- | ------------------------------------------------ | ---------------------------- |
| [setAggs.js](../../genomehubs/genomehubs/src/genomehubs-api/src/api/v2/reports/setAggs.js)           | attributeTerms, attributeHistogram, scaleBuckets | `scatter.rs`, `histogram.rs` |
| [getBounds.js](../../genomehubs/genomehubs/src/genomehubs-api/src/api/v2/reports/getBounds.js)       | getBounds, setScale, setTerms                    | `bounds.rs`                  |
| [histogramAgg.js](../../genomehubs/genomehubs/src/genomehubs-api/src/api/v2/queries/histogramAgg.js) | histogramAgg, scaledBounds                       | `histogram.rs`               |
| [processHits.js](../../genomehubs/genomehubs/src/genomehubs-api/src/api/v2/functions/processHits.js) | bucket labeling, field extraction                | `processor.rs`               |

### From UI (to simplify or remove)

| File                                                                                                      | Function                               | Status (Phase 4)                            |
| --------------------------------------------------------------------------------------------------------- | -------------------------------------- | ------------------------------------------- |
| [processReport](../../genomehubs/genomehubs/src/genomehubs-ui/src/client/views/selectors/report.js#L905)  | Overall transform                      | Simplified (v3 response already processed)  |
| [processScatter](../../genomehubs/genomehubs/src/genomehubs-ui/src/client/views/selectors/report.js#L325) | Jitter, scale transforms               | Moved to SDK (jitter pre-calculated by API) |
| [applyJitter](../../genomehubs/genomehubs/src/genomehubs-ui/src/client/views/selectors/report.js#L480)    | Add Gaussian noise                     | Moved to SDK                                |
| Query building in fetchReport                                                                             | Parameter validation, URL construction | Simplified (SDK handles more)               |

---

## Related Documentation

- [AGENTS.md](../../AGENTS.md) — Agent contribution guidelines (6 touch-points for PyO3)
- [.github/copilot-instructions.md](.github/copilot-instructions.md) — Workspace coding rules
- [CONTRIBUTING.md](../../CONTRIBUTING.md) — Contributing standards
- [scripts/verify_code.sh](scripts/verify_code.sh) — Full verification checklist

---

## Glossary

| Term             | Definition                                                             |
| ---------------- | ---------------------------------------------------------------------- |
| **DSL**          | Domain-Specific Language (Rust enums + structs for aggregation config) |
| **FFI**          | Foreign Function Interface (Rust → Node.js/Python via PyO3)            |
| **v2 API**       | Current genomehubs-api endpoint format                                 |
| **v3 API**       | New SDK-driven endpoint format (cleaner responses)                     |
| **FieldFetcher** | Introspection of field metadata from live API                          |
| **Compat shim**  | v2 compat layer (API calls v3 internally, returns v2 format)           |
| **MVP**          | Minimum Viable Product (histogram, scatter, arc reports + async jobs)  |

---

**Document Version:** 1.0 (2026-04-20)
**Last Updated:** 2026-04-20
**Next Review:** After Phase 1 completion (Week 8-9)
