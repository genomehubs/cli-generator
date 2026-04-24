#!/usr/bin/env node

import { fileURLToPath } from "url";
import fs from "fs";
import path from "path";

async function main() {
  const args = process.argv.slice(2);
  if (args.length < 1) {
    console.error(
      "Usage: node scripts/validate_javascript_sdk.js /path/to/js/goat",
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

  const pkgNode = path.join(sdkDir, "pkg-nodejs");
  if (!fs.existsSync(pkgNode) || !fs.statSync(pkgNode).isDirectory()) {
    console.error(
      "✗ JavaScript SDK WASM module missing (pkg-nodejs/ not found)",
    );
    process.exit(1);
  }

  // dynamic import requires file:// URL
  const qUrl = `file://${queryJs}`;
  try {
    const mod = await import(qUrl);
    const QueryBuilder = mod.QueryBuilder || mod.default?.QueryBuilder;
    if (!QueryBuilder) {
      console.error("✗ QueryBuilder export not found in query.js");
      process.exit(1);
    }
    console.log("✓ Import QueryBuilder works");

    // Instantiate
    const qb = new QueryBuilder("taxon");
    if (qb._index !== "taxon") {
      console.error("✗ QueryBuilder instantiation failed (index mismatch)");
      process.exit(1);
    }
    console.log("✓ QueryBuilder instantiation works");

    // Methods
    qb.setTaxa(["Mammalia"], "tree");
    qb.addField("genome_size");
    if (!Array.isArray(qb._taxa) || qb._taxa.length === 0) {
      console.error("✗ Taxa not set");
      process.exit(1);
    }
    if (!Array.isArray(qb._fields) || qb._fields.length === 0) {
      console.error("✗ Fields not set");
      process.exit(1);
    }
    console.log("✓ QueryBuilder methods (setTaxa, addField) work");

    // URL generation
    const url = qb.toUrl();
    if (typeof url !== "string" || url.length === 0) {
      console.error("✗ URL generation failed");
      process.exit(1);
    }
    console.log("✓ URL generation works");
    console.log("  URL:", url);

    console.log("\n✓ JavaScript SDK validation passed");
    process.exit(0);
  } catch (err) {
    console.error("✗ Error during JS validation:", err.message || err);
    process.exit(1);
  }
}

main();
