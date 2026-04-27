#!/usr/bin/env node

import fs from "fs";
import path from "path";

const args = process.argv.slice(2);
if (args.length < 1) {
  console.error(
    "Usage: node scripts/validate_javascript_sdk_deep.mjs /path/to/js/goat",
  );
  process.exit(2);
}
const sdkDir = path.resolve(args[0]);
if (!fs.existsSync(sdkDir) || !fs.statSync(sdkDir).isDirectory()) {
  console.error(`✗ JavaScript SDK directory not found: ${sdkDir}`);
  process.exit(1);
}

const queryJs = path.join(sdkDir, "query.js");
if (!fs.existsSync(queryJs)) {
  console.error(`✗ query.js not found in: ${sdkDir}`);
  process.exit(1);
}

const qUrl = `file://${queryJs}`;
try {
  const mod = await import(qUrl);
  const QueryBuilder = mod.QueryBuilder || mod.default?.QueryBuilder;
  const parseSearchJson =
    mod.parseSearchJson || mod.parse_search_json || mod.parseSearch || null;
  const parseResponseStatus =
    mod.parseResponseStatus || mod.parse_response_status || null;
  const annotateSourceLabels =
    mod.annotateSourceLabels || mod.annotate_source_labels || null;
  const splitSourceColumns =
    mod.splitSourceColumns || mod.split_source_columns || null;
  const annotatedValues = mod.annotatedValues || mod.annotated_values || null;
  const toTidyRecords = mod.toTidyRecords || mod.to_tidy_records || null;

  console.log("\n== Deep Validation: JavaScript SDK ==\n");

  // Test 1: Count
  console.log("Test 1: Count (count())");
  const qb1 = new QueryBuilder("taxon").setTaxa(["Mammalia"], "tree");
  const count = await qb1.count();
  if (typeof count !== "number")
    throw new Error("count() should return number");
  if (!(count > 0)) throw new Error("Expected count > 0");
  console.log(`  ✓ count() works: ${count} records found`);

  // Test 2: Search
  console.log("Test 2: Search (search())");
  const qb2 = new QueryBuilder("taxon")
    .setTaxa(["Mammalia"], "tree")
    .addField("genome_size")
    .setSize(10);
  const raw = await qb2.search();
  const results = parseSearchJson
    ? parseSearchJson(raw)
    : JSON.parse(raw).map((r) => r.result || r);
  if (!Array.isArray(results) || results.length === 0)
    throw new Error("search() should return non-empty array");
  console.log(`  ✓ search() works: returned ${results.length} results`);
  console.log(
    `    First result: ${JSON.stringify(results[0]).substring(0, 120)}...`,
  );

  // Test 3: Response parsing
  console.log("Test 3: Response parsing (parseResponseStatus)");
  const qb3 = new QueryBuilder("taxon")
    .setTaxa(["Insecta"], "tree")
    .addField("genome_size")
    .setSize(5);
  const raw3 = await qb3.search();
  if (parseResponseStatus) {
    const status = parseResponseStatus(raw3);
    console.log("  ✓ parseResponseStatus works:", JSON.stringify(status));
  } else {
    console.log("  ⊙ parseResponseStatus not exported; skipping");
  }

  // Deterministic fixture checks
  console.log("Test 4: Deterministic fixture-based checks");
  const fixturePath = path.resolve(
    process.cwd(),
    "scripts",
    "fixtures-goat",
    "fixture_mammalia_search_raw.json",
  );
  if (!fs.existsSync(fixturePath)) {
    console.log(
      `  ⊙ Fixture not found at ${fixturePath} — skipping deterministic checks`,
    );
    process.exit(0);
  }
  const rawFixture = fs.readFileSync(fixturePath, "utf8");
  const parsed = parseSearchJson
    ? parseSearchJson(rawFixture)
    : JSON.parse(rawFixture).results.map((r) => r.result);
  console.log(`  ✓ Parsed fixture into ${parsed.length} records`);

  // Derive expected presence of direct/descendant from parsed rows' `*__source` keys
  let expectedDirect = false;
  let expectedDescendant = false;
  if (Array.isArray(parsed)) {
    for (const row of parsed) {
      if (row && typeof row === "object") {
        for (const [k, v] of Object.entries(row)) {
          if (!k.endsWith("__source")) continue;
          if (v === "direct") expectedDirect = true;
          if (v === "descendant") expectedDescendant = true;
        }
      }
    }
  }

  // Inspect split output for column suffixes
  let hasDirect = false;
  let hasDescendant = false;
  if (splitSourceColumns) {
    let split = splitSourceColumns(
      parsed ? parsed : rawFixture ? rawFixture : JSON.stringify(parsed),
    );
    if (!Array.isArray(split)) split = Object.values(split);
    for (const row of split) {
      for (const k of Object.keys(row)) {
        if (k.endsWith("__direct")) hasDirect = true;
        if (k.endsWith("__descendant")) hasDescendant = true;
      }
    }
  } else if (Array.isArray(parsed)) {
    for (const row of parsed) {
      for (const k of Object.keys(row)) {
        if (k.endsWith("__direct")) hasDirect = true;
        if (k.endsWith("__descendant")) hasDescendant = true;
      }
    }
  }
  console.log(
    `    Found __direct columns: ${hasDirect}, __descendant columns: ${hasDescendant}`,
  );

  // Assert split output matches expectations derived from `{field}__source` keys
  if (expectedDirect) {
    if (!hasDirect)
      throw new Error(
        "Expected __direct split columns based on fixture `{field}__source` values",
      );
  } else {
    if (hasDirect)
      throw new Error("Did not expect __direct split columns for this fixture");
  }
  if (expectedDescendant) {
    if (!hasDescendant)
      throw new Error(
        "Expected __descendant split columns based on fixture `{field}__source` values",
      );
  } else {
    if (hasDescendant)
      throw new Error(
        "Did not expect __descendant split columns for this fixture",
      );
  }

  // Annotated values check for Descendant label
  let foundDescendantLabel = false;
  if (annotatedValues) {
    let ann = annotatedValues(parsed);
    if (!Array.isArray(ann)) ann = Object.values(ann);
    for (const row of ann) {
      for (const v of Object.values(row)) {
        if (typeof v === "string" && v.includes("Descendant"))
          foundDescendantLabel = true;
      }
    }
  } else {
    for (const row of parsed) {
      for (const v of Object.values(row)) {
        if (typeof v === "string" && v.includes("Descendant"))
          foundDescendantLabel = true;
      }
    }
  }
  console.log(
    `    Found 'Descendant' label in annotated values: ${foundDescendantLabel}`,
  );
  if (expectedDescendant) {
    if (!foundDescendantLabel)
      throw new Error(
        "Expected at least one 'Descendant' label in annotated values based on `{field}__source` values",
      );
  } else {
    if (foundDescendantLabel)
      throw new Error(
        "Did not expect 'Descendant' labels in annotated values for this fixture",
      );
  }

  console.log("\n✓ All deep validation tests passed!");
  process.exit(0);
} catch (err) {
  console.error("✗ Error during JS deep validation:", err.message || err);
  process.exit(1);
}
