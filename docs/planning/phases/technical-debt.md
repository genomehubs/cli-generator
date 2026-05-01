# Technical Debt & Cleanup Tasks

Ongoing maintenance and refactoring work that improves code quality but is not
part of the main feature phases.

---

## Refactor `create_js_package()` for Reduced Cognitive Complexity

**File:** `src/commands/new.rs` line 851
**Issue:** Function exceeds clippy's cognitive_complexity threshold
**Impact:** Pre-submit check requires `#[allow(clippy::cognitive_complexity)]`
**Suggested approach:** Extract helper functions for:

- WASM package detection and copying (try pkg-nodejs → pkg-web → pkg)
- Tera template rendering and file writing
- JavaScript package.json configuration

**Priority:** Low (suppressible via allow attribute, function works correctly)
**Effort:** 1–2 hours refactoring + testing

---

## Notes

All suppressions should include a comment explaining why the lint is benign:

```rust
#[allow(clippy::cognitive_complexity)]
// This function orchestrates multi-step SDK generation with intentional branching
// for WASM package detection. Refactoring would require passing 10+ state variables
// between helpers. Revisit if logic grows beyond current scope.
fn create_js_package(...) { ... }
```
