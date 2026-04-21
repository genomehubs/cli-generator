# Documentation Reorganization Checklist

**Created**: 2026-04-21
**Status**: Ready for manual review & execution
**Timeline**: 30 min to execute

---

## 📋 Actions to Take

### Move Current Files to New Folders

#### **planning/** folder (API refactoring + roadmap)

```bash
# Files to move here:
mv docs/api-aggregation-refactoring-plan.md docs/planning/

# Files to create/rename:
# - Rename multi-language-sdk-plan.md → multi-language-roadmap.md
# - Rename post-mvp-roadmap.md → post-mvp-capabilities.md
# - Already created: planning/GAPS-AND-OPPORTUNITIES.md (integration runbook gaps)
```

#### **reference/** folder (Design docs & completed analysis)

```bash
# Files to move here:
mv docs/python-sdk-design.md docs/reference/
mv docs/goat-cli-gap-analysis.md docs/reference/
mv docs/coverage-measurement.md docs/reference/

# Files to rename & move:
# mv docs/query-builder-plan.md docs/reference/query-builder-design.md
```

#### **testing/** folder (Test infrastructure & audits)

```bash
# Files to move here:
mv docs/parameter-coverage-audit.md docs/testing/
mv docs/test-scripts-reference.md docs/testing/
mv docs/test-fixtures-quick-reference.md docs/testing/

# Files to CONSOLIDATE into one:
# 1. Read:
#    - test-fixtures-strategy.md
#    - test-fixtures-usage.md
#    - phase-6-complete-reference.md (fixture section)
# 2. Create: docs/testing/fixtures-complete-guide.md (merged from above)
# 3. Move consolidated file to docs/testing/

# Files to create:
# - docs/testing/sdk-parity-testing.md
#   (consolidate phase-6-sdk-testing.md + phase-6-documentation-parity.md)
```

#### **Root docs/** (Keep visible)

```bash
# Already created:
# - MAIN.md (index & current status)
# - HISTORY.md (archive guide)
```

---

### Consolidate Phase Documents

**Into `docs/HISTORY/` (defer these)**:

```bash
# Phase 1 docs (no execution yet):
mkdir -p docs/HISTORY/phase-1
mv docs/phase-1-error-testing-plan.md docs/HISTORY/phase-1/
mv docs/phase-1-multilanguage-foundation.md docs/HISTORY/phase-1/

# Phase 6 docs (complete; keep as reference):
mkdir -p docs/HISTORY/phase-6
mv docs/phase-6-complete-reference.md docs/HISTORY/phase-6/
mv docs/phase-6-documentation-parity.md docs/HISTORY/phase-6/
mv docs/phase-6-sdk-testing.md docs/HISTORY/phase-6/

# Superseded planning:
mkdir -p docs/HISTORY/superseded
mv docs/test-coverage-strategy.md docs/HISTORY/superseded/
mv docs/testing-generated-sdks.md docs/HISTORY/superseded/
mv docs/test-fixtures-strategy.md docs/HISTORY/superseded/ (before consolidation)
```

---

## Final Structure After Reorganization

```
docs/
├── MAIN.md ⭐                          (Entry point + current status)
├── HISTORY.md                          (Archive guide)
├──
├── planning/
│   ├── api-aggregation-refactoring-plan.md      (PRIORITY: phases 0-5)
│   ├── multi-language-roadmap.md                (Renamed from plan)
│   ├── post-mvp-capabilities.md                 (Renamed from roadmap)
│   └── GAPS-AND-OPPORTUNITIES.md                (NEW: overlooked planning areas)
│
├── testing/
│   ├── fixtures-complete-guide.md              (NEW: consolidated fixtures guide)
│   ├── sdk-parity-testing.md                   (NEW: consolidated parity + docs tests)
│   ├── parameter-coverage-audit.md             (Current; update monthly)
│   └── test-scripts-reference.md
│
├── reference/
│   ├── python-sdk-design.md
│   ├── query-builder-design.md                 (Renamed from plan)
│   ├── goat-cli-gap-analysis.md
│   └── coverage-measurement.md
│
└── HISTORY/                                     (Completed & planned phases)
    ├── phase-1/
    │   ├── error-testing-plan.md               (Planned; no execution)
    │   └── multilanguage-foundation.md         (Planned; no execution)
    ├── phase-6/
    │   ├── complete-reference.md               (✅ Complete)
    │   ├── documentation-parity.md             (✅ Complete)
    │   └── sdk-testing.md                      (✅ Complete)
    └── superseded/
        ├── test-coverage-strategy.md           (Superseded)
        ├── test-fixtures-strategy.md           (Consolidated)
        └── testing-generated-sdks.md           (Superseded)
```

---

## 🎯 Critical Files to Double-Check Before Moving

| File                                  | Action          | Reason                                                     |
| ------------------------------------- | --------------- | ---------------------------------------------------------- |
| `multi-language-sdk-plan.md`          | Review & rename | 33KB; may want to prune outdated sections                  |
| `phase-1-multilanguage-foundation.md` | Review          | 37KB; related to multi-language plan; may consolidate      |
| `api-aggregation-refactoring-plan.md` | KEEP PROMINENT  | 28KB; main active planning doc; don't suppress             |
| `parameter-coverage-audit.md`         | UPDATE STATUS   | Add "Last audit: 2026-04-21; Coverage 58%; Review monthly" |

---

## 📝 Updates to Reference Files

After moving files, update these links in MAIN.md:

- ✅ Already done (MAIN.md uses correct new paths)

Create symlink or redirect if old links break:

- Phase 1 plans → docs/HISTORY/phase-1/
- Phase 6 docs → docs/HISTORY/phase-6/

---

## ✅ Verification Checklist

- [ ] All `planning/` files reference correct successor docs
- [ ] MAIN.md links still work (test with `grep planning/ docs/MAIN.md`)
- [ ] HISTORY.md index matches file locations
- [ ] Consolidated fixture guide is readable (not broken by merge)
- [ ] No broken links in any file (`grep -r "../docs/"` to find old relative paths)
- [ ] README.md or project root points to docs/MAIN.md as entry point

---

## 🚀 Next Steps After Reorganization

1. **Update project README** — Add "📚 [See docs/MAIN.md](docs/MAIN.md) for documentation index"
2. **Update agent-logs** — Link to new consolidated docs instead of old plan files
3. **Commit reorganization** — "refactor: consolidate docs into function-based organization"
4. **Pin MAIN.md** — Make it the first thing people see when opening docs/

---

## 📌 Notes

- **Don't delete files yet** — archive them in HISTORY/ so git history is preserved
- **Be cautious with consolidated files** — Test that merged content is still coherent
- **Docstring updates** — Some files (esp. phase docs) reference each other; update cross-refs
