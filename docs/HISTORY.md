# Documentation History & Archived Plans

This folder contains completed Phase documentation and superseded plans.
See [MAIN.md](MAIN.md) for current active documentation.

---

## ✅ Completed Phases

### Phase 6: SDK Testing & Documentation Parity (2026-03-19 → 2026-04-15)

**Outcome**: Cross-language SDK consistency verified; Quarto reference complete.

**Consolidated Documents:**

- `phase-6-complete-reference.md` → Fixture testing lifecycle ✅
- `phase-6-documentation-parity.md` → All canonical methods documented ✅
- `phase-6-sdk-testing.md` → Parity test suite (15 tests) ✅

**Key Achievement**:

- All 30+ canonical methods present in Python, JavaScript, and R SDKs
- Constructor parameters consistent (`validation_level`, `api_base`)
- Quarto reference includes all methods with signatures
- Pytest fixture system fully operational (26 cached fixtures, parametrized tests)

---

## 📋 Planned Phases (Not Yet Started)

These plans were drafted but execution has not begun. They are blocked on:

1. Completion of Phase 0 (parse parity) — **In-progress**
2. Readiness of downstream repos (boat-cli, assessment-api) for integration
3. Decision on multi-language snippet generation approach

### Phase 1: Error Scenarios & Property-Based Testing

**Document**: `phase-1-error-testing-plan.md`
**Status**: Planned; no agent-logs for execution
**Scope**: Add error scenario tests to maintain 90%+ coverage (currently 90%+ achieved via different mechanism)
**Blocker**: Can defer until Phase 0 completion

### Phase 1–5: Multi-Language Foundation

**Document**: `phase-1-multilanguage-foundation.md`
**Status**: Planned; no execution
**Scope**: Refactor codegen for language-agnostic query DSL (2-week estimate)
**Blocker**: Requires other repos ready for integration

---

## 🔄 Superseded Documents

| Document                     | Reason                                                                   | Successor                                                        |
| ---------------------------- | ------------------------------------------------------------------------ | ---------------------------------------------------------------- |
| `test-coverage-strategy.md`  | Multi-phase approach fragmented; replaced by per-document coverage goals | Coverage goals per file (see `MAIN.md`)                          |
| `test-fixtures-strategy.md`  | Strategic overview; merged into consolidated fixtures guide              | [fixtures-complete-guide.md](testing/fixtures-complete-guide.md) |
| `multi-language-sdk-plan.md` | R/JS/Go snippet approach; consolidated into api-refactoring plan         | [api-refactoring-phases.md](planning/api-refactoring-phases.md)  |

---

## 📂 File Organization (Pre-2026-04-21)

Before cleanup, docs were distributed by phase iteration with overlapping scope:

```
docs/ (old structure)
├── phase-1-*.md (3 files, no execution)
├── phase-6-*.md (3 files, complete)
├── test-*.md (5 files, overlapping scope)
├── *.md feature plans (scattered)
```

**Issues**:

- Readers had to cross-reference 6 docs to understand "how to test"
- No clear index of what's complete vs. planned
- Phase numbering didn't reflect actual project flow

**Resolution** (2026-04-21):

- Created `MAIN.md` as single entry point
- Moved to function-based organization (testing/, planning/, reference/)
- Archived old phase docs here
- Consolidated overlapping guides (fixtures 3→1 file)

---

## 🎯 When to Read These

**Want historical context?** → Read the relevant phase doc here
**Implementing a planned phase?** → Use corresponding planned-phase doc, then move to MAIN structure
**Just need current state?** → Use [MAIN.md](MAIN.md)
