# Cached API Schemas

This directory stores snapshots of target site API schemas at each SDK release.

## Purpose

When the polling job detects API changes, it compares live APIs against these cached versions.
Hashing these files lets us detect drift without manual review of large schema files.

## File naming

```
<site>-api-schema.json
```

Examples:
- `goat-api-schema.json` — Goat database API schema (cached at last goat-cli release)
- `boat-api-schema.json` — Boat database API schema (if released)

## Structure

Each cached schema file should include:
- Timestamp (when cached)
- API version (if available from the live API)
- Full schema structure (field definitions, types, available queries/endpoints)

Example:
```json
{
  "cached_at": "2026-04-21T14:30:00Z",
  "api_version": "v2",
  "schema": {
    "fields": { ... },
    "queries": { ... },
    "mutations": { ... }
  }
}
```

## When to update

- **After releasing an SDK** → Download live API schema and save here
- **During polling job** → Copy new schema if API changed
- **Manual refresh** → If API changed and polling job should be re-run

## Related Files

- [polling-config.yml](../planning/polling-config.yml) — Configuration for polling job
- [sites-version-manifest.yml](../planning/sites-version-manifest.yml) — Version tracking
- [release-strategy.md](../planning/release-strategy.md) — Full release process
