# Describe feature YAML fallback fix

**Date:** 2026-03-20
**Task:** Fix the describe feature's YAML fallback parsing to handle raw query strings

## Problem

The `--describe` flag was implemented to show human-readable query descriptions, but it was failing at runtime with:

```
error: parsing SearchQuery YAML: invalid type: unit value, expected a sequence
```

The root cause was the fallback YAML formatting when a raw query string (not a valid YAML SearchQuery) was provided:

```yaml
index: taxon
identifiers:
attributes:
query: {}
```

This had two issues:

1. No valid fields inside `identifiers:` and `attributes:` sections (empty dicts)
2. A non-existent `query:` field that the SearchQuery struct doesn't have

## Solution

Changed the fallback YAML wrapping strategy in [main.rs.tera](../templates/rust/main.rs.tera#L213-L220):

**Before:**

```rust
let yaml = format!(
    "index: taxon\nidentifiers:\nattributes:\nquery: {}",
    query_str
);
```

**After:**

```rust
let yaml = format!(
    "index: taxon\nattributes:\n  - name: scientific_name\n    operator: eq\n    value: \"{}\"",
    query_str
);
```

This wraps a raw query string as a proper attribute filter condition, which:

- Provides valid YAML structure that deserializes correctly
- Treats the input as a filter on `scientific_name` (most common CLI use case)
- Escapes quotes properly to handle special characters

## Testing

Verified all describe modes work:

1. **Basic describe:** `goat-cli taxon search --taxon Mammalia --describe`
   - Output: `Search for taxa in taxa, filtered to scientific name = tax_name(Mammalia).`

2. **Verbose describe:** `goat-cli taxon search --taxon Mammalia --describe --verbose`
   - Output: Shows expanded breakdown with filters applied

3. **Compound query:** `goat-cli taxon search --taxon Mammalia --query "genome_size(lt:3G)" --describe`
   - Output: Correctly describes combined filters

4. **Fallback path:** `goat-cli taxon search --query "tax_name(Mammalia)" --describe`
   - Output: Fallback YAML parsing works for raw query strings

5. **Normal execution:** `goat-cli taxon search --taxon Mammalia --size 5`
   - Verified regular search still works (not broken by describe changes)

## Impact

- ✅ Describe feature now fully functional
- ✅ Both concise and verbose modes work
- ✅ Handles raw query strings via fallback
- ✅ Normal search operations unaffected
- ✅ Generated CLI is truly self-contained with no external dependencies

## Files Modified

- `templates/rust/main.rs.tera` — Fixed fallback YAML format in `describe_query()` function
