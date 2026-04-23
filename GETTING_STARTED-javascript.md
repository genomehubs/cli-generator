# JavaScript SDK — Quick Reference

This is a quick reference for the JavaScript SDK. **For comprehensive examples with all methods,
operators, and options, see the [QueryBuilder reference](docs/reference/query-builder.html) in the full documentation** or run `quarto preview docs/` in the generated project.

## Installation

The pre-built WASM module is included in the artifact. No installation needed—just use it:

```bash
cd js/goat
node
```

## Quick Start

```javascript
// In Node.js REPL or .js script file
import { QueryBuilder } from "./query.js";

// Create a query builder
const qb = new QueryBuilder("taxon");

// Add filters and fields (methods chain)
qb.setTaxa(["Mammalia"], "tree").addField("genome_size");

// Generate the URL (no network call, synchronous)
console.log(qb.toUrl());

// Or fetch results
const count = await qb.count();
const results = await qb.search();
```

## Core Operations

### Building Queries

| Operation               | Example                                    |
| ----------------------- | ------------------------------------------ |
| **Create builder**      | `new QueryBuilder("taxon")`                |
| **Set taxa**            | `.setTaxa(["Mammalia"], "tree")`           |
| **Add field**           | `.addField("genome_size")`                 |
| **Filter by attribute** | `.addAttribute("genome_size", "ge", "1G")` |
| **Set result size**     | `.setSize(100)`                            |
| **Sort results**        | `.setSort("genome_size", "desc")`          |

### Fetching (Async)

| Operation          | Example                                        | Returns                    |
| ------------------ | ---------------------------------------------- | -------------------------- |
| **Count**          | `await qb.count()`                             | Number                     |
| **Search**         | `await qb.search()`                            | Array of objects           |
| **Parse response** | JavaScript SDK does not expose parse functions | (use Python/R for parsing) |

## Examples

### Example 1: Simple Count

```javascript
const { QueryBuilder } = await import("./query.js");

const qb = new QueryBuilder("taxon").setTaxa(["Mammalia"], "tree");

const count = await qb.count();
console.log(`Mammals: ${count} records`);
```

### Example 2: Filter by Attribute

```javascript
// Find mammals with genome size >= 1 gigabase
const qb = new QueryBuilder("taxon")
  .setTaxa(["Mammalia"], "tree")
  .addAttribute("genome_size", "ge", "1G")
  .addField("genome_size");

const results = await qb.search();
console.log(`Found ${results.length} records`);
console.log(results.slice(0, 3));
```

### Example 3: Multiple Operators

```javascript
// Mammals with genome size between 1G and 3G, with specific fields
const qb = new QueryBuilder("taxon")
  .setTaxa(["Mammalia"], "tree")
  .addAttribute("genome_size", "ge", "1G")
  .addAttribute("genome_size", "le", "3G")
  .addField("genome_size")
  .addField("assembly_span")
  .setSize(100);

const results = await qb.search();
console.log(`Found ${results.length} Mammals with 1-3G genomes`);
results.forEach((r) => {
  console.log(`${r.taxon_name}: ${r.genome_size} bp`);
});
```

### Example 4: Complex Query with Sorting

```javascript
// Insects with genome size info, sorted descending
const qb = new QueryBuilder("taxon")
  .setTaxa(["Insecta"], "tree")
  .addAttribute("genome_size", "exists")
  .addField("genome_size")
  .addField("assembly_span")
  .setSort("genome_size", "desc")
  .setSize(50);

const results = await qb.search();
console.log(`Returned ${results.length} Insects with genome_size info`);
```

## Attribute Operators

When using `.addAttribute()`, the available operators depend on the field type:

| Operator   | Meaning               | Example                                                    |
| ---------- | --------------------- | ---------------------------------------------------------- |
| `"gt"`     | Greater than          | `.addAttribute("genome_size", "gt", "1G")`                 |
| `"ge"`     | Greater than or equal | `.addAttribute("genome_size", "ge", "1G")`                 |
| `"lt"`     | Less than             | `.addAttribute("genome_size", "lt", "5G")`                 |
| `"le"`     | Less than or equal    | `.addAttribute("genome_size", "le", "3G")`                 |
| `"eq"`     | Equals (enum fields)  | `.addAttribute("assembly_level", "eq", "complete genome")` |
| `"exists"` | Field has a value     | `.addAttribute("c_value", "exists")`                       |

See [QueryBuilder reference → Attribute filters](docs/reference/query-builder.html#attribute-filters) for the full list of operators and field-specific options.

## Named Parameters vs Operators

Some filters use named parameters instead of operators:

```javascript
// Named parameters (setTaxa, setRank, setAssemblies, etc.)
const qb = new QueryBuilder("taxon")
  .setTaxa(["Mammalia"], "tree")
  .setRank("species");

// Operators (addAttribute)
const qb2 = new QueryBuilder("taxon").addAttribute("genome_size", "ge", "1G");
```

See [QueryBuilder reference](docs/reference/query-builder.html) for a complete list of all methods and their parameters.

## Advanced: Query Description & Code Generation

```javascript
// Get a human-readable description of the query
const description = qb.describe();
console.log(description);

// Generate code snippets in other languages
const snippets = qb.snippet({
  siteName: "goat",
  sdkName: "goat_sdk",
  languages: ["python", "r", "javascript"],
});
console.log(snippets["python"]);
console.log(snippets["r"]);
```

See [Quickstart → Description & Snippet Generation](docs/quickstart.html#description--code-snippets) for examples.

## Full Documentation

For the complete API reference:

1. **In the repo:**

   ```bash
   quarto preview docs/
   ```

   Opens an interactive preview in your browser.

2. **In artifacts:**
   The rendered HTML docs are included. Open `docs/index.html` in your browser.

3. **Static files:**
   - [docs/reference/query-builder.html](docs/reference/query-builder.html) — Complete method reference
   - [docs/quickstart.html](docs/quickstart.html) — Full tutorials with all methods

## Validation & Debugging

Use the validation scripts to verify the SDK works:

```bash
# Quick smoke test (import, instantiate, build URL)
bash scripts/validate_javascript_sdk.sh ./js/goat

# Deep validation (test count(), search() with real API calls)
bash scripts/validate_javascript_sdk.sh --deep ./js/goat
```

If you encounter issues, the deep validation shows which methods are working with actual examples.

## Common Patterns

### Chain vs. Step-by-step

```javascript
// Chain style (recommended)
const qb = new QueryBuilder("taxon")
  .setTaxa(["Mammalia"])
  .addField("genome_size")
  .setSize(50);

// Step-by-step
const qb2 = new QueryBuilder("taxon");
qb2.setTaxa(["Mammalia"]);
qb2.addField("genome_size");
qb2.setSize(50);

// Both work identically
```

### REPL vs. Script File

**In the Node.js REPL** (interactive):

```javascript
const { QueryBuilder } = await import("./query.js");
const qb = new QueryBuilder("taxon").setTaxa(["Mammalia"]);
const results = await qb.search();
```

**In a script file** (\*.js):

```javascript
import { QueryBuilder } from "./query.js";

const qb = new QueryBuilder("taxon").setTaxa(["Mammalia"]);
qb.search().then((results) => {
  console.log(`Found ${results.length} records`);
  console.log(results.slice(0, 3));
});
```

### Handling Results

```javascript
// Search returns an array of plain objects
const results = await qb.search();
console.log(typeof results); // "object"
console.log(Array.isArray(results)); // true
console.log(results[0]); // { taxon_name: "...", genome_size: 123, ... }

// Iterate over results
results.forEach((record) => {
  console.log(record.taxon_name, record.genome_size);
});
```

### Checking URL Without Fetching

```javascript
// URL generation is synchronous, never makes network calls
const url = qb.toUrl();
console.log(url); // https://goat.genomehubs.org/api/v2/search?...

// Check if it looks valid
if (url.includes("genome_size")) {
  console.log("URL includes genome_size field");
}
```

## Getting Help

1. **Check the examples** — See all methods with code samples in [docs/reference/query-builder.html](docs/reference/query-builder.html)
2. **Run deep validation** — Test all methods with real API calls: `bash scripts/validate_javascript_sdk.sh --deep ./js/goat`
3. **Read the tutorials** — [docs/quickstart.html](docs/quickstart.html) has step-by-step walkthroughs
4. **Debug URLs** — Use `.toUrl()` to verify query structure before making expensive `.search()` calls

---

**Last updated:** April 2026 | **SDK:** WASM-compiled Rust with Node.js bindings
