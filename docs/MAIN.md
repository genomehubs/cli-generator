# CLI Generator — Documentation Index

**Project Status**: Active development — v3 API parity + report endpoints + full SDK coverage.

**Last Updated**: 2026-05-01
**Current Focus**: v3 API parity (Phase 0–3) and report endpoint implementation (Phases 4–7)
**Next Milestone**: Full /search + /record + /lookup parity; report infrastructure foundation

---

## 🎯 Active Work

### Current Phase: v3 API Parity + Report Endpoints

- **Lead Document**: [v3-api-parity-plan.md](planning/v3-api-parity-plan.md) — **authoritative plan** for all remaining API, report, and SDK work
- **Status**: Phase 0 (envelope fixes) ready to start; Phase 1 (/search + shared ES infra) next
- **Outstanding**: Phases 0–9 as detailed in the plan

### In-Progress Audits

- **Parameter Coverage**: [parameter-coverage-audit.md](testing/parameter-coverage-audit.md) — 58% coverage (11/19 major groups)
- **SDK Parity Testing**: [sdk-parity-testing.md](testing/sdk-parity-testing.md) — Cross-language consistency verified
- **Coverage Threshold**: Relaxed to 65% (focus on high-value coverage)

---

## 📖 Documentation by Category

### Testing (Primary Working Reference)

| Document                                                           | Purpose                                            | Status         |
| ------------------------------------------------------------------ | -------------------------------------------------- | -------------- |
| [fixtures-complete-guide.md](testing/fixtures-complete-guide.md)   | Fixture discovery, caching, and pytest integration | ✅ Complete    |
| [sdk-parity-testing.md](testing/sdk-parity-testing.md)             | Cross-SDK method & parameter consistency           | ✅ Complete    |
| [parameter-coverage-audit.md](testing/parameter-coverage-audit.md) | Current API parameter test coverage                | 🔄 In-Progress |
| [test-scripts-reference.md](testing/test-scripts-reference.md)     | Quick command reference for test scripts           | 📚 Reference   |

### Planning & Roadmap

| Document                                                        | Purpose                                                           | Status                |
| --------------------------------------------------------------- | ----------------------------------------------------------------- | --------------------- |
| [**v3-api-parity-plan.md**](planning/v3-api-parity-plan.md)     | **Authoritative plan** — v3 API, reports, SDK parity (Phases 0–9) | 🎯 **Active**         |
| [integration-runbook.md](planning/integration-runbook.md)       | Step-by-step guide for onboarding new repos                       | ✅ Complete           |
| [extension-guide.md](planning/extension-guide.md)               | Add parameters, validators, languages to generator                | ✅ Complete           |
| [sdk-parse-parity-plan.md](planning/sdk-parse-parity-plan.md)   | SDK method naming, parse functions, WASM/extendr parity           | 🔄 Partially complete |
| [api-refactoring-phases.md](planning/api-refactoring-phases.md) | Earlier 5-phase SDK-driven architecture plan                      | 🗄️ Superseded         |
| [release-strategy.md](planning/release-strategy.md)             | Multi-language SDK release + package manager plan                 | ✅ Complete           |
| [multi-language-roadmap.md](planning/multi-language-roadmap.md) | R, JS, Go snippet generation + multi-repo strategy                | 📋 Planned            |
| [post-mvp-capabilities.md](planning/post-mvp-capabilities.md)   | Deferred features with priority/effort/rationale                  | 📚 Reference          |

### Design & Reference

| Document                                                       | Purpose                                    | Status       |
| -------------------------------------------------------------- | ------------------------------------------ | ------------ |
| [python-sdk-design.md](reference/python-sdk-design.md)         | PyO3 FFI architecture + type translation   | ✅ Complete  |
| [query-builder-design.md](reference/query-builder-design.md)   | Query builder core design & invariants     | ✅ Complete  |
| [coverage-measurement.md](reference/coverage-measurement.md)   | Rust & Python coverage tool quick start    | 📚 Reference |
| [goat-cli-gap-analysis.md](reference/goat-cli-gap-analysis.md) | Feature parity: hardcoded vs generated CLI | ✅ Complete  |

### Archived (Completed Phases)

See [HISTORY.md](HISTORY.md) for:

- Phase 1: Error Testing Plan (no execution)
- Phase 1: Multi-Language Foundation (no execution, superseded by api-refactoring-phases.md)
- Phase 6: Documentation Parity (✅ complete)
- Other phase-based planning (consolidated into MAIN structures)

---

## 🚀 Path to MVP & Integration

**Current State** (2026-05-01):

- ✅ CLI generator core (Rust) — Rust coverage 90%+
- ✅ SDK generation (Python, JS, R templates) — All 3 languages tested
- ✅ Query builder + validation + parse pipeline
- ✅ Pytest fixtures + cross-SDK parity testing
- ✅ CI pipeline (Rust, Python, JS)
- ✅ genomehubs-api v3: /status, /resultFields, /taxonomies, /taxonomicRanks, /indices, /count
- ⚠️ genomehubs-api v3: /search, /record, /lookup, /summary, /msearch, /report — in plan
- ⚠️ SDK: count + search only; record/lookup/report methods not yet implemented

**Active Plan**: [v3-api-parity-plan.md](planning/v3-api-parity-plan.md)

| Phase | Focus                                                          | Detail Doc                                                          | Status  |
| ----- | -------------------------------------------------------------- | ------------------------------------------------------------------- | ------- |
| 0     | Return envelope consistency + `ranks` key fix                  | [phase-0](planning/phases/phase-0-envelope-consistency.md)          | 🔜 Next |
| 1     | Shared ES infra + `/search`                                    | [phase-1](planning/phases/phase-1-search-and-infra.md)              | 🔜      |
| 2     | `/record`, `/lookup`, `/summary`, `/msearch`                   | [phase-2](planning/phases/phase-2-record-lookup-summary-msearch.md) | 🔜      |
| 3     | SDK coverage for new endpoints                                 | [phase-3](planning/phases/phase-3-sdk-coverage.md)                  | 🔜      |
| 4     | Report axis type system (`genomehubs-query`)                   | [phase-4](planning/phases/phase-4-report-axis-types.md)             | 🔜      |
| 5     | Report infrastructure (`genomehubs-api`)                       | [phase-5](planning/phases/phase-5-report-infrastructure.md)         | 🔜      |
| 6     | Report types: histogram, scatter, xPerRank, sources, tree, map | [phase-6](planning/phases/phase-6-report-types.md)                  | 🔜      |
| 7     | Arc reports                                                    | [phase-7](planning/phases/phase-7-arc-reports.md)                   | 🔜      |
| 8     | Per-site Swagger customisation                                 | [phase-8](planning/phases/phase-8-swagger-customisation.md)         | 🔜      |
| 9     | URL query string support (late)                                | [phase-9](planning/phases/phase-9-url-query-strings.md)             | 🔜      |

---

## ⚠️ Known Gaps & Deferred Work

| Area                                 | Issue                                 | Rationale                                  | Planned For |
| ------------------------------------ | ------------------------------------- | ------------------------------------------ | ----------- |
| **`/search`, `/record`, `/report`**  | Not yet in v3 API                     | See v3-api-parity-plan.md Phases 1–6       | Phases 1–6  |
| **SDK record/lookup/report methods** | Not yet implemented                   | Depends on Phases 1–2                      | Phase 3     |
| **Oxford plot report type**          | Complex, separate design needed       | Part of wider report family                | Deferred    |
| **`/download` route**                | File streaming infra decisions needed | Disk path / S3 redirect not yet decided    | Deferred    |
| **R fixture CI**                     | Extendr build overhead                | Too heavyweight for CI; local testing only | Deferred    |
| **Parameter coverage**               | 58% (11/19 groups)                    | Add fixture scenarios on-demand            | Continuous  |

---

## 🔗 Quick Navigation

**Want to...**

- **See the current plan?** → [v3-api-parity-plan.md](planning/v3-api-parity-plan.md)
- **Test the generator?** → [test-scripts-reference.md](testing/test-scripts-reference.md)
- **Understand fixture caching?** → [fixtures-complete-guide.md](testing/fixtures-complete-guide.md)
- **See what's not tested?** → [parameter-coverage-audit.md](testing/parameter-coverage-audit.md)
- **Plan post-MVP work?** → [post-mvp-capabilities.md](planning/post-mvp-capabilities.md)
- **Understand SDK parity?** → [sdk-parity-testing.md](testing/sdk-parity-testing.md)
- **Learn about query building?** → [query-builder-design.md](reference/query-builder-design.md)
