# CLI Generator — Documentation Index

**Project Status**: MVP Phase — Core SDK generation + testing complete; integration work pending.

**Last Updated**: 2026-04-21
**Current Focus**: API aggregation refactoring + integration preparation
**Next Milestone**: MVP release + agent cleanup

---

## 🎯 Active Work

### Current Phase: Integration & Refactoring

- **Lead Document**: [api-refactoring-phases.md](planning/api-refactoring-phases.md) — 5-phase strategy for SDK-driven architecture
- **Status**: Planning complete; Phase 0 (parse parity) near completion
- **Outstanding**: Phase 1–5 refactoring (deferred post-MVP)

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

| Document                                                        | Purpose                                            | Status                             |
| --------------------------------------------------------------- | -------------------------------------------------- | ---------------------------------- |
| [api-refactoring-phases.md](planning/api-refactoring-phases.md) | 5-phase SDK-driven architecture refactoring        | 📋 Planned (blocking: other repos) |
| [release-strategy.md](planning/release-strategy.md)             | Multi-language SDK release + package manager plan  | ✅ Complete                        |
| [multi-language-roadmap.md](planning/multi-language-roadmap.md) | R, JS, Go snippet generation + multi-repo strategy | 📋 Planned                         |
| [post-mvp-capabilities.md](planning/post-mvp-capabilities.md)   | Deferred features with priority/effort/rationale   | 📚 Reference                       |

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

**Current State** (2026-04-21):

- ✅ CLI generator core (Rust) — Rust coverage 90%+
- ✅ SDK generation (Python, JS, R templates) — All 3 languages tested
- ✅ Query builder + validation
- ✅ Pytest fixtures + cross-SDK parity testing
- ✅ CI pipeline (Rust, Python, JS) — R deferred
- ⚠️ API aggregation refactoring — Phase 0 (parse parity) complete; Phase 1+ deferred

**MVP Criteria**:

- Core generation working end-to-end ✅
- Three-language SDK generation with parity testing ✅
- All Rust/Python coverage checks passing ✅
- CI pipeline passing (Python + JS) ✅

**After CI Green**:

1. Clean up workspace (remove stale terminals, demos)
2. Update agent logs with session summary
3. Prepare branch for integration into other repos (boat, other cli projects)
4. Document integration approach for downstream users

---

## ⚠️ Known Gaps & Deferred Work

| Area                                  | Issue                      | Rationale                                               | Planned For |
| ------------------------------------- | -------------------------- | ------------------------------------------------------- | ----------- |
| **R fixture CI**                      | Extendr build overhead     | Too heavyweight for CI; local testing only              | Phase 2+    |
| **API refactoring phases 1–5**        | Requires other repos ready | Blocked on boat-cli & assessment-api integration        | Post-MVP    |
| **Multi-language snippet generation** | Design incomplete          | Needs query representation agreement                    | Post-MVP    |
| **Parameter coverage**                | Only 58% (11/19 groups)    | Add fixture scenarios on-demand as new parameters added | Continuous  |

---

## 🔗 Quick Navigation

**Want to...**

- **Test the generator?** → [test-scripts-reference.md](testing/test-scripts-reference.md)
- **Understand fixture caching?** → [fixtures-complete-guide.md](testing/fixtures-complete-guide.md)
- **See what's not tested?** → [parameter-coverage-audit.md](testing/parameter-coverage-audit.md)
- **Plan post-MVP work?** → [post-mvp-capabilities.md](planning/post-mvp-capabilities.md)
- **Understand SDK parity?** → [sdk-parity-testing.md](testing/sdk-parity-testing.md)
- **Learn about query building?** → [query-builder-design.md](reference/query-builder-design.md)
