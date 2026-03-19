# Integration Tests for Preview & Update Commands

**Date:** 2026-03-19
**Session:** Phase 0.2 (Coverage Gaps)
**Objective:** Close critical coverage gaps (0% coverage) in `preview` and `update` CLI commands

---

## Summary

Added 4 integration tests to `tests/generated_goat_cli.rs` to exercise the preview and update CLI commands.
These tests contributed to a **+8.62% coverage gain** (77.90% → 86.52%), closing two critical 0% gaps:

- `src/commands/preview.rs`: 0% → 90.20% (+46 lines)
- `src/commands/update.rs`: 0% → 90.91% (+20 lines)
- `src/main.rs`: 42.9% → 85.7% (bonus: CLI parsing now tested)

---

## Changes Made

### New Tests in `tests/generated_goat_cli.rs`

| Test                                            | Purpose                                                  | Coverage Impact                          |
| ----------------------------------------------- | -------------------------------------------------------- | ---------------------------------------- |
| `preview_new_site_produces_output()`            | Verify `preview --site goat` renders Rust code to stdout | preview.rs `run_new()`                   |
| `preview_update_repo_diffs_changes()`           | Verify `preview --repo` diffs modified files             | preview.rs `run_update()` + diff helpers |
| `update_command_modifies_existing_repo()`       | Verify `update` regenerates stale files                  | update.rs `run()` + file I/O             |
| `update_command_preserves_hand_written_files()` | Verify `update` doesn't touch non-generated paths        | update.rs write-safety contract          |

### Key Design Decisions

1. **Used existing fixture:** All tests reused `generate_goat_cli()` helper to scaffold repos quickly
2. **Subprocess-based:** Tests invoke CLI binary via `Command::new()` rather than calling functions directly
   — More realistic (tests the actual CLI parsing + dispatch)
   — Simpler than unit tests (no need to mock I/O)
3. **Minimal assertions:** Tests focus on happy path (commands succeed, output appears)
   — Error scenarios deferred to Phase 1 (error cascade tests)

---

## Baseline → Post-Implementation

```
Rust Coverage:  77.90% → 86.52% (+8.62pp)
Python Coverage: 79.70% (unchanged)

Module Breakdown:

  commands/preview.rs    0% →  90.20%  ✅ NEW
  commands/update.rs     0% →  90.91%  ✅ NEW
  commands/new.rs       93%  →  93%    ✓ Stable
  commands/validate.rs  50%  →  50%    (requires error tests)
  main.rs             42.9% →  85.7%  ✅ CLI flags now tested

  core/codegen.rs      100%  →  100%   ✓ Complete
  core/config.rs       100%  →  100%   ✓ Complete
  core/fetch.rs         79%  →   79%   (needs error handling tests)
  core/query/url.rs     91%  →   91%   ✓ Solid
  core/query/validation 68%  →   68%   (API edge cases)
```

---

## Technical Details

### Preview Command Behavior

The preview command has two modes (not subcommands, but flags):

- `--site <name>`: Render templates for a new site, print all files to stdout
- `--repo <path>`: Render for an existing repo, diff against current disk state

Key implementation (src/commands/preview.rs):

```rust
pub fn run_new(site_name: &str, sites_dir: &Path, force_fresh: bool) -> Result<()> {
    let site = load_site_config(site_name, sites_dir)?;
    let rendered = gen.render_all(&site, &options, &fields_by_index)?;
    print_rendered(&rendered);  // ← Tested
    Ok(())
}

pub fn run_update(repo_path: &Path, force_fresh: bool) -> Result<()> {
    let rendered = gen.render_all(...)?;
    diff_against_disk(repo_path, &rendered);  // ← Tested
    Ok(())
}
```

### Update Command Behavior

Re-renders an existing repo's `src/generated/` and `src/cli_meta.rs`, leaves hand-written code intact.

```rust
pub fn run(repo_path: &Path, config_dir: Option<&Path>, force_fresh: bool) -> Result<()> {
    let site = SiteConfig::from_file(...)?;
    let rendered = gen.render_all(&site, &options, &fields_by_index)?;
    write_generated_files(repo_path, &rendered)?;  // ← Tested
    Ok(())
}
```

---

## Remaining Coverage Gaps (Phase 1)

After these tests, three modules remain sub-85%:

| Module                     | Coverage | Gap                                 | Type            |
| -------------------------- | -------- | ----------------------------------- | --------------- |
| `commands/validate.rs`     | 50%      | Missing happy path + error tests    | CLI integration |
| `core/query/validation.rs` | 68%      | Edge cases in field/flag validation | Core logic      |
| `core/query/attributes.rs` | 58%      | Attribute selection ordering logic  | Core logic      |
| `core/fetch.rs`            | 79%      | HTTP error scenarios                | External API    |

**Phase 1 priorities:**

1. Add error scenario tests (HTTP 5xx, malformed YAML, missing files)
2. Add property-based tests for query builders (proptest)
3. Re-run coverage and commit baseline before expanding to R/JS SDKs

---

## Lessons Learned

1. **CLI flag discovery:** The `preview` command flags (`--repo`, `--site`) weren't obvious from first reading; had to check `src/main.rs` to understand the clap structure
2. **Integration vs. unit:** These tests would have been ~100 LOC as unit tests (mocking file I/O); subprocess approach was 20 LOC and more realistic
3. **Fixture reuse:** Sharing `generate_goat_cli()` with existing tests avoided duplicated setup logic

---

## Test Run Output

```
test result: ok. 12 passed; 0 failed; 0 ignored
Coverage measurement:   77.90% → 86.52%
Lines tested:           705/905 → 783/905
```

All tests completed without errors. Ready for Phase 1 (error + property tests).

---

## Next Steps

1. ✅ **Completed:** Phase 0.2 - Preview/Update CLI coverage (this session)
2. 📋 **Phase 1.0:** Error scenario tests (commands/_, core/_ error paths)
3. 📋 **Phase 1.1:** Property-based tests (query builder invariants)
4. 📋 **Phase 1.2:** Enforce 85%+ coverage in CI
5. 📋 **Phase 2:** R SDK infrastructure (template reorganization, multi-language)
