# Agent Log: API Aggregation Refactoring Strategy

**Date:** 2026-04-20
**Scope:** Planning (no implementation code written)
**Status:** COMPLETE — Comprehensive 5-phase refactoring strategy finalized

---

## Summary

Conducted comprehensive audit of three codebases (genomehubs-api, genomehubs-ui, blobtoolkit) and designed a **5-phase refactoring strategy** to make cli-generator the foundational data layer for the genomehubs ecosystem. Strategy addresses circular dependency concern (FieldFetcher) and establishes clear migration path from synchronous v2 API to async v3 with SDK-driven architecture.

---

## Problem Statement

**Current State:**

- API aggregation logic (setAggs, getBounds, histogramAgg) is imperative, scattered across multiple Node.js files
- Query validation and building duplicated between API and UI
- No async job handling for long-running reports (e.g., 100k-taxon trees)
- UI does significant data reshaping after API responses (fragile contract)
- CLI generation depends on live API via FieldFetcher (circular dependency for new instances)

**Opportunity:**

- cli-generator already has query building + snippet generation foundation
- Rust aggregation engine can replace imperative ES DSL, provide type safety
- SDK can be source of truth for validation, query building, response shaping
- Async job queue + Redis caching can enable scaling to large reports

---

## Deliverables

### 1. Comprehensive Audit Reports

- **API codebase** ([genomehubs-api](../../genomehubs/genomehubs/src/genomehubs-api)): Architecture diagram, aggregation flow, current data transformation responsibilities, limitations
- **UI codebase** ([genomehubs-ui](../../genomehubs/genomehubs/src/genomehubs-ui)): Data flow, selector/component logic, reshaping operations, inefficiencies
- **blobtoolkit** ([blobtk](../../blobtoolkit/blobtk)): Validation capabilities, field metadata patterns, ES integration readiness

### 2. Strategic Alignment Document

- 11 key decisions finalized with user (SDK role, aggregation design, code location, error handling, caching, etc.)
- Addressed circular dependency concern (FieldFetcher): documented pre-MVP workaround + post-MVP decoupling strategy
- Clarified test data strategy (both mock + real ES), tree report pattern, error response formats

### 3. 5-Phase Implementation Roadmap

| Phase                  | Goal                                                | Duration | MVP Checkpoint            |
| ---------------------- | --------------------------------------------------- | -------- | ------------------------- |
| **1**                  | Rust aggregation engine (DSL + bounds + processors) | 6-8w     | ✓ All 11 report types     |
| **2**                  | SDK FFI + response shapers (Node.js/Python)         | 4-6w     | ✓ histogram, scatter, arc |
| **3**                  | v3 endpoints calling SDK                            | 4-6w     | ✓ MVP endpoints live      |
| **4**                  | UI migration + feature flag                         | 6-8w     | Gradual rollout           |
| **5**                  | Async job queue (PostgreSQL + Bull)                 | 4-6w     | Parallel with Phase 4     |
| **Phase 6 (Post-MVP)** | Decouple FieldFetcher for offline generation        | 6-8w     | N/A                       |

**MVP Checkpoint (Week 10):** Phases 1+2+3 complete for histogram, scatter, arc. Full parity target: All 11 report types.

### 4. Architecture & Design Decisions

**Aggregation Engine (Rust):**

- Type-safe DSL in [cli-generator/src/core/aggregation_v3/](src/core/aggregation_v3/)
- Report-type-specific builders (histogram, scatter, arc, tree, etc.) — no plugins, fully integrated
- Bounds calculation (scale detection, numeric ranges, categorical terms)
- Response processors (post-process ES aggs, apply scales, categorize)
- JSON Schema generation (via `schemars` crate) for validation
- Direct port of existing API logic → ensures parity

**SDK Report Processors (Node.js/Python):**

- HistogramBuilder, ScatterBuilder, etc. — validate + generate ES aggs
- Response shaping (unpack nested aggs → clean v3 format)
- 6 touch-points for PyO3 FFI (from [AGENTS.md](../../AGENTS.md#checklist-for-adding-a-new-pyo3-function))
- Local file path dependency during dev; defer npm publish until near MVP

**v3 API (Node.js):**

- New routes calling SDK (not direct ES queries)
- HTTP 200 + inline error messages (matches v2 contract)
- MVP scope: histogram, scatter, arc; v2 compat shim for others
- Redis caching (daily flush, preload common reports)

**UI Migration (React/Redux):**

- Feature flag: `apiVersion: "v2" | "v3"`
- Parallel selectors for safe A/B testing
- Gradual rollout: MVP reports first, then other types
- v2 compat shim at API layer (transparent to UI)

**Async Jobs (PostgreSQL + Bull):**

- Job table + Bull queue
- Polling-based progress feedback (not WebSocket)
- JSON file storage on disk (not database)
- For reports >50k docs complexity threshold
- Preload cache support for common queries

### 5. Critical Constraints & Post-MVP Roadmap

**Pre-MVP Constraint: FieldFetcher Circular Dependency**

- CLI generation (`new`, `preview`, `update`) depends on live API
- New genomehubs instances can't run `cli-generator new` until API is live
- **Workaround (Pre-MVP):** Accept as precondition, provide docker-compose bootstrap kit
- **Phase 6 (Post-MVP):** Decouple FieldFetcher by extracting field metadata logic into reusable library ([cli-generator/src/core/field_metadata.rs](src/core/field_metadata.rs))

**Test Strategy:**

- Phase 1: Unit tests (bounds calc, scales) + property tests (roundtrip) + integration tests (mock ES)
- Phase 2: Both mock ES responses (unit) and real ES instances (integration)
- Phase 3: Integration tests comparing v2 vs. v3 output (±2% tolerance)
- Phase 4: Selector tests + visual regression (v2 vs. v3 rendering)
- Phase 5: Job queue tests + worker crash recovery + E2E UI progress feedback

---

## Key Design Decisions

| Decision                                           | Rationale                                                                         |
| -------------------------------------------------- | --------------------------------------------------------------------------------- |
| **Rust-first aggregation DSL**                     | Type-safe, auto-generates JSON Schema, supports Python bindings (post-MVP)        |
| **Report-type-specific handlers**                  | Each report (histogram, scatter, etc.) has own builder + processor; not unified   |
| **Direct port of API logic**                       | Maintain parity ±2% vs. redesign from scratch                                     |
| **Local file path (dev) → npm publish (post-MVP)** | Simpler dependency management during dev; defer package versioning until mature   |
| **Accept FieldFetcher dependency (pre-MVP)**       | Documented Bootstrap workflow; decoupling deferred to Phase 6 for lower risk      |
| **Redis cache + daily flush**                      | Matches index rebuild cycle; supports preload of common reports                   |
| **HTTP 200 + inline errors**                       | Consistent with existing v2 contract; empty bounds OK if no results               |
| **Polling-based job progress**                     | Simpler than WebSocket; sufficient for use case (reports take seconds-to-minutes) |

---

## Risks & Mitigations

| Risk                                                  | Mitigation                                                                                                      |
| ----------------------------------------------------- | --------------------------------------------------------------------------------------------------------------- |
| Rust DSL doesn't match v2 output                      | Extensive Phase 1 tests; 2% tolerance for rounding/precision                                                    |
| FFI breaks generated projects                         | Follow all 6 touch-points from [AGENTS.md](../../AGENTS.md); test with `bash scripts/dev_site.sh --python goat` |
| UI rendering breaks during migration                  | Feature flag allows fallback to v2; A/B testing during rollout                                                  |
| Async job queue adds complexity                       | Start simple (polling + disk); no WebSocket yet; test crash recovery                                            |
| Database schema migration issues                      | Reversible migrations; test on staging first                                                                    |
| **Circular FieldFetcher dependency blocks bootstrap** | Pre-MVP: provide docker-compose kit; Phase 6: decouple for offline-first generation                             |

---

## Timeline

```
Week 1-8:    Phase 1 (Aggregation Engine)
       ↓ (parallel)
Week 3-9:    Phase 2 (SDK FFI)
       ↓
Week 5-11:   Phase 3 (API v3)
       ↓
Week 10:     🎯 MVP CHECKPOINT: Phases 1+2+3 complete (histogram, scatter, arc)
       ↓ (parallel)
Week 7-15:   Phase 4 (UI Migration)
Week 11-17:  Phase 5 (Async Jobs)

Post-MVP:
       ↓
Phase 6:     Decouple FieldFetcher (~6-8w)
```

**Total MVP timeline:** 19-22 weeks (including 2-3w contingency for integration testing).

---

## Exploration Process

### Subagent Calls

1. **Explore genomehubs-api** — Documented architecture, aggregation flow, 11 report types, response formats
2. **Explore genomehubs-ui** — Data flow, selector reshaping logic, inefficiencies, validation gaps
3. **Explore blobtoolkit** — Validation patterns, field metadata, ES integration readiness

### User Alignment Sessions

1. **Strategic decisions** — 5 questions: SDK role, async strategy, aggregation design, code location, backward compat
2. **Design details** — 4 questions: DSL representation, report unification, Python bindings, job notification
3. **Implementation clarifications** — 5 decisions: no plugins, tree same pattern as others, error handling inline, Redis caching, local file paths
4. **Circular dependency** — 4 questions: identified FieldFetcher dependency, documented pre-MVP workaround, Phase 6 decoupling plan

---

## Files Created (This Session)

- [/memories/session/plan.md](../../memories/session/plan.md) — Detailed strategic decisions + architectural constraint + post-MVP roadmap
- [/memories/session/implementation-plan.md](../../memories/session/implementation-plan.md) — Phase-by-phase implementation details + file mappings + verification checklist

---

## Next Steps (For Implementation)

1. **Phase 1 Kickoff**
   - Create [src/core/aggregation_v3/](src/core/aggregation_v3/) directory structure
   - Implement `dsl.rs` with ReportType enum + request structs (Histogram, Scatter, Arc, Tree, Map, Oxford, Arc, xPerRank, Sources, Files, Types, Table)
   - Port bounds calculation logic from [getBounds.js](../../genomehubs/genomehubs/src/genomehubs-api/src/api/v2/reports/getBounds.js)
   - Begin report-type-specific aggregation builders

2. **Parallel: Bootstrap Documentation**
   - Create docker-compose.yml starter kit for new genomehubs instances (addresses FieldFetcher dependency)
   - Document `new` command workflow: "start API first, then run cli-generator new"

3. **CI/CD Readiness**
   - Ensure `cargo test --lib aggregation_v3` runs in CI (Phase 1 verification)
   - Ensure `maturin develop --features extension-module` tested in CI (Phase 2 verification)

---

## Session Artifacts

**In-memory (session notes):**

- API audit findings (11 report types, aggregation scattered, bounds calc logic)
- UI audit findings (deep nesting, duplicate query logic, reshaping inefficiencies)
- blobtoolkit audit findings (validation patterns, field metadata, ES-ready architecture)

**Documented (memories):**

- Strategic decisions (11 finalized items)
- Architectural constraint (circular FieldFetcher dependency + workarounds)
- 5-phase roadmap with timelines and file mappings
- Verification checklist (per phase)
- Risk mitigation table

---

## Related Issues / PRs

- Active PR: [Feature/multi language support](https://github.com/genomehubs/cli-generator/pull/1) — This phase 2 work will eventually depend on aggregation engine (Phase 1)
- No existing issues for aggregation refactoring; recommend opening issue once Phase 1 planning is approved

---

## Conclusion

This planning session established a **comprehensive, low-risk refactoring strategy** that:

- Makes cli-generator the foundational data layer for genomehubs ecosystem
- Replaces scattered, imperative aggregation logic with type-safe Rust DSL
- Enables SDK-driven API architecture (v3) with async job queue support
- Provides clear migration path from v2 to v3 (feature flag, v2 compat shim)
- Addresses circular dependency concern with documented workarounds
- Prioritizes MVP over perfection (histogram, scatter, arc first; full parity by end)

Ready for Phase 1 implementation kickoff.
