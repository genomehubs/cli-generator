# Planning Gaps & Overlooked Areas

Before you pick this project back up for integration work, consider these areas that may benefit from upfront planning:

---

## 🚨 Critical Planning Gaps (Before Integration)

### 1. **Integration Runbook** ⭐ PRIORITY (USER-IDENTIFIED)
**Current State**: api-refactoring-phases.md outlines the architectural strategy but lacks concrete integration steps.

**Gap**: How will downstream repos (boat-cli, assessment-api) actually _use_ the generated SDKs?
- Which generation flow will they use (cargo run vs. API)?
- How are generated projects versioned in their repos (committed, CI-generated, etc.)?
- How are updates rolled out (breaking changes, deprecation policy)?

**Suggested Doc**: `integration-runbook.md` — Step-by-step guide for onboarding a new repo to use cli-generator

---

### 2. **Extension Guide** ⭐ PRIORITY (USER-IDENTIFIED)
**Current State**: Templates exist but instructions for customization are scattered.

**Gap**: How does a user:
- Add a new query parameter (e.g., `set_custom_field()`)?
- Add a new language to code generation (e.g., Go)?
- Extend the validator with custom rules?
- Add a new snippet language (e.g., R queries)?

**Suggested Doc**: `extension-guide.md` — Checklist for each extension type with examples

---

### 3. **Release Strategy** ✅ COMPLETE
**Status**: [release-strategy.md](./release-strategy.md) — Comprehensive plan for multi-language SDK releases.

**Coverage**:
- Independent semantic versioning per generated site (decoupled from cli-generator version)
- All four package managers: PyPI, conda-forge, npm, CRAN
- Release cadence tied to repo changes + API drift detection (polling job)
- Pre-release testing + signing (unsigned in dev, signed in production)
- API schema polling strategy to detect when target APIs change
- Per-site version manifest for tracking state across releases
- CI/CD skeleton (ready to activate post-MVP user testing)

**Next Step**: Implement manifest file + polling config post-MVP; activate CI/CD after initial testing

---

### 4. **Site-Agnostic Fixture Generation** ⭐ PRIORITY (USER-IDENTIFIED)
**Current State**: Test fixtures rely on goat-specific taxa and field names; fixture generation script hardcoded for goat.

**Gap**: How do tests work for other sites (boat, assessment-api, etc.) that have different field metadata?
- Fixture discovery script needs site-agnostic parameter coverage (not assume "Mammalia" exists)
- Fixtures should be regeneratable for any site without manual hardcoding
- E2E tests must handle unfamiliar data structures

**Suggested Doc**: `site-agnostic-fixture-generation.md` — Parameterized discovery, validation against any schema, per-site fixture caching

---

### 5. **Full Tutorials** ⭐ SECONDARY (USER-IDENTIFIED)
**Current State**: Reference docs (query-builder-design.md, python-sdk-design.md) are good but lack guided walkthroughs.

**Gap**: New users need step-by-step tutorials:
- "Generate your first SDK in 10 minutes"
- "Build a multi-step query" (chaining, merge, combine)
- "Add custom validation to your generated SDK"
- "Extend with a new parameter type"

**Suggested Doc**: `tutorials/` folder with worked examples + jupyter notebooks

---

### 6. **File IO Integration** ⭐ SECONDARY (USER-IDENTIFIED)
**Current State**: CLI reads/writes output files; no consistent file abstraction in SDKs.

**Gap**: Eventually use blobtk crate for file operations (reading/writing locally, to cloud stores, etc.):
- CLI-side: blobtk integration for query input/output files
- SDK-side: expose file operations to Python/JS/R where appropriate
- Standardized file formats for results? (JSON, parquet, CSV?)
- Query caching strategy?

**Suggested Doc**: `file-io-strategy.md` — blobtk integration points, Python/JS/R bindings, file format decisions

---

### 7. **Upgrade & Migration Strategy**
**Current State**: No versioning or deprecation strategy documented.

**Gap**: As cli-generator evolves:
- How will breaking template changes be communicated?
- How long are old template versions supported?
- Can generated projects opt into schema migrations automatically?

**Suggested Doc**: `versioning-and-migration.md` — Versioning scheme, deprecation cycle, migration tooling

---

## 🔍 Secondary Planning Gaps (Nice-to-Have, Post-MVP)

### 4. **Performance & Load Testing Strategy**
**Current State**: Functional tests pass; no mention of performance baselines.

**Gap**:
- What are acceptable latencies for URL building, query validation, batch operations?
- Should `MultiQueryBuilder` have performance benchmarks?
- Are there known bottlenecks in WASM builds or FFI calls?

**Suggested Doc**: `performance-strategy.md` — Baseline metrics, test scenarios, profiling approach

---

### 5. **Contribution Guide** (for cli-generator itself)
**Current State**: `.github/copilot-instructions.md` exists but no human contribution guide.

**Gap**:
- How do developers set up a dev environment?
- What are the testing requirements for a PR?
- Are there architectural decisions that PRs should respect?

**Suggested Doc**: `CONTRIBUTING.md` in root — DX-focused guide for contributors

---

### 6. **Multi-Repo Coordination & Release Planning**
**Current State**: api-refactoring-phases.md mentions "other repos ready" as a blocker but gives no concrete timeline.

**Gap**:
- Which repos depend on cli-generator? (boat, assessment-api, others?)
- What's the release cadence (monthly, per-feature, on-demand)?
- Who approves changes and coordinates releases across repos?

**Suggested Doc**: `multi-repo-coordination.md` — Dependency map, release calendar, approval workflow

---

### 7. **Quality Assurance & Release Checklist**
**Current State**: CI pipeline exists; no documented pre-release checks.

**Gap**:
- What manual testing is expected before release?
- How are regressions caught?
- Is there a staged rollout strategy (alpha, beta, stable)?

**Suggested Doc**: `release-checklist.md` — Pre-release verification, rollout stages, hot-fix process

---

### 8. **Troubleshooting & Known Issues**
**Current State**: No troubleshooting guide.

**Gap**:
- What are common errors users encounter when generating SDKs?
- How to debug WASM build failures across platforms?
- What to do if a generated project has type conflicts?

**Suggested Doc**: `troubleshooting.md` — FAQ, error catalog, diagnostic steps

---

## 📊 Priority Matrix

| Area | Impact | Effort | Priority | Timeline | User Notes |
|------|--------|--------|----------|----------|-----------|
| **Integration Runbook** | 🔴 High | Low | 🔴 NOW | Before integration work | CRITICAL for boat-cli/assessment-api onboarding |
| **Extension Guide** | 🔴 High | Medium | 🔴 NOW | Before external users | CRITICAL for customization path |
| **Release Strategy** | 🔴 High | Medium | 🔴 NOW | Before first release | CRITICAL: conda/pip/npm/cran distribution decisions |
| **Site-Agnostic Fixtures** | 🔴 High | Medium | 🔴 SOON | After MVP, before other sites test | CRITICAL blocker for boat, assessment-api testing |
| **Full Tutorials** | 🟡 Medium | Medium | 🟡 SOON | Before external users | Walkthroughs & worked examples needed |
| **File IO Strategy** | 🟡 Medium | Medium | 🟡 LATER | Post-MVP, design phase | blobtk integration roadmap |
| **Upgrade/Migration** | 🔴 High | Medium | 🟡 SOON | Before version 1.0 | Needed for multi-repo stability |
| **Performance Strategy** | 🟢 Low | Medium | 🟢 LATER | Post-MVP, if bottlenecks found | Only if perf issues emerge |
| **Contributing Guide** | 🟡 Medium | Low | 🟡 SOON | If expecting external PRs | Standard practice |
| **Troubleshooting** | 🟢 Low | Low | 🟢 LATER | As issues come up | Build as problems arise |

---

## 🎯 Recommended Pre-Integration Actions

1. **Write Integration Runbook** (1 doc)
   - How to generate an SDK in an existing repo
   - Where to commit generated files
   - How to keep in sync with cli-generator updates

2. **Write Multi-Repo Coordination Plan** (1 doc)
   - Which repos integrate cli-generator?
   - Release schedule
   - Who owns what?

3. **Write Extension Guide** (1 doc, comprehensive)
   - Adding a new query parameter (checklist)
   - Adding a new language template (checklist)
   - Adding custom validation rules (checklist)

---

## 📝 Template Stub for New Docs

Each planning doc should include:

```markdown
# [Document Title]

## Overview
[1-2 sentence summary]

## Current State
✅ What's done
⚠️ What's partial
❌ What's missing

## Blockers
- [ ] Blocker 1
- [ ] Blocker 2

## Timeline & Effort
- Estimated effort: X weeks
- Owner: [Who should lead this?]
- Dependencies: [What else needs to be done first?]

## Next Steps
1. [Action 1]
2. [Action 2]
```

---

## 🔗 Related Documents
- [MAIN.md](MAIN.md) — Current project status
- [api-refactoring-phases.md](planning/api-refactoring-phases.md) — Long-term architecture strategy
- [post-mvp-capabilities.md](planning/post-mvp-capabilities.md) — Deferred features
