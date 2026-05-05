# SearchQuery Structure and YAML Parsing Guide

## Overview

The `SearchQuery` struct is defined in `crates/genomehubs-query/src/query/mod.rs` and represents the _what_ to search for in genomehubs API queries. It can be loaded from YAML strings via `SearchQuery::from_yaml()` and describes both single queries and multi-query (OR/AND) combinations.

---

## 1. SearchQuery Struct Definition

### Top-Level Structure

```rust
pub struct SearchQuery {
    /// Which index to search (for single-query mode; ignored in multi-query).
    pub index: SearchIndex,

    /// Taxon, assembly, and sample identifiers with rank and filter type.
    /// Flattened into the YAML (no "identifiers:" wrapper).
    pub identifiers: Identifiers,

    /// Attribute filters, return fields, name classes, and rank columns.
    /// Flattened into the YAML (no "attributes:" wrapper).
    pub attributes: AttributeSet,

    /// Multiple queries to combine (enables multi-query mode).
    pub queries: Option<Vec<SearchQuery>>,

    /// How to combine multiple queries: AND or OR (default: AND).
    pub combine_with: CombineStrategy,
}
```

### SearchIndex Enum

```rust
pub enum SearchIndex {
    Taxon,    // "taxon" in YAML
    Assembly, // "assembly" in YAML
    Sample,   // "sample" in YAML
}
```

---

## 2. Identifiers Structure (Flattened)

### Rust Definition

```rust
pub struct Identifiers {
    /// Scientific taxon names or IDs. "!" prefix = NOT filter.
    /// Deserialized from YAML "taxa: [...]" list.
    pub taxa: Option<TaxaIdentifier>,

    /// Assembly accession IDs (e.g. "GCF_000002305.6").
    pub assemblies: Vec<String>,

    /// Sample accession IDs (e.g. "SRR1234567").
    pub samples: Vec<String>,

    /// Taxonomic rank for filtering results (maps to `tax_rank(X)` in the query).
    pub rank: Option<String>,
}

pub struct TaxaIdentifier {
    pub names: Vec<String>,
    pub filter_type: TaxonFilterType,
}
```

### TaxaIdentifier Deserialization

When the YAML parser encounters:

```yaml
taxa: [Mammalia, "!Felis"]
taxon_filter_type: tree
```

It deserializes into:

```rust
TaxaIdentifier {
    names: vec!["Mammalia".to_string(), "!Felis".to_string()],
    filter_type: TaxonFilterType::Tree,
}
```

### TaxonFilterType Enum

Controls which API function wraps each taxon name:

| YAML Value       | Function         | Description           |
| ---------------- | ---------------- | --------------------- |
| `name` (default) | `tax_name(X)`    | Exact name match only |
| `tree`           | `tax_tree(X)`    | All descendants       |
| `lineage`        | `tax_lineage(X)` | All ancestors         |

**Important**: Only `name` is the default; if you omit `taxon_filter_type`, it defaults to `name`.

---

## 3. AttributeSet Structure (Flattened)

### Rust Definition

```rust
pub struct AttributeSet {
    /// Attribute filter conditions (e.g. `genome_size < 3G`).
    pub attributes: Vec<Attribute>,

    /// Columns to return in search results.
    pub fields: Vec<Field>,

    /// Taxon name classes to include (maps to `&names=`, NOT `&fields=`).
    pub names: Vec<String>,

    /// Taxonomic rank columns to include in results (maps to `&ranks=`).
    pub ranks: Vec<String>,

    /// Fields to exclude ancestrally derived estimates for (maps to `&excludeAncestral=`).
    pub exclude_ancestral: Vec<String>,

    /// Fields to exclude descendant-derived estimates for (maps to `&excludeDescendant=`).
    pub exclude_descendant: Vec<String>,

    /// Fields to exclude directly estimated values for (maps to `&excludeDirect=`).
    pub exclude_direct: Vec<String>,

    /// Fields to exclude missing values for (maps to `&excludeMissing=`).
    pub exclude_missing: Vec<String>,
}
```

### Attribute Structure

```rust
pub struct Attribute {
    pub name: String,
    pub operator: Option<AttributeOperator>,
    pub value: Option<AttributeValue>,
    pub modifier: Vec<Modifier>,
}

pub enum AttributeOperator {
    Eq, Ne, Lt, Le, Gt, Ge, Exists, Missing
}

pub enum AttributeValue {
    Single(String),
    List(Vec<String>),
}
```

### Field Structure

```rust
pub struct Field {
    pub name: String,
    pub modifier: Vec<Modifier>,
}
```

---

## 4. From YAML Parsing

### Custom Deserializers

The `Identifiers` and `AttributeValue` types have custom deserializers that handle:

#### Identifiers Deserialization

```yaml
# Input YAML
taxa: [Mammalia, "!Felis", "*bat*"]
taxon_filter_type: tree
assemblies: []
samples: []
rank: species

# Deserialized to:
Identifiers {
    taxa: Some(TaxaIdentifier {
        names: ["Mammalia", "!Felis", "*bat*"],
        filter_type: TaxonFilterType::Tree,
    }),
    assemblies: vec![],
    samples: vec![],
    rank: Some("species"),
}
```

#### AttributeValue Normalization

Size suffixes and scientific notation are automatically normalized:

```yaml
# Input YAML
- name: genome_size
  value: "3G" # expands to "3000000000"

- name: other_field
  value: "1.5e6" # expands to "1500000"

- name: list_field
  value: [val1, val2, val3] # stays as list
```

---

## 5. Field Mapping: SearchQuery → Elasticsearch Query

### The Build Pipeline

```
SearchQuery
    ↓
build_raw_query_fragment() [url.rs]
    ↓
Query string components:
  - taxa:         tax_name(X) / tax_tree(X) / tax_lineage(X)
  - rank:         tax_rank(species)
  - assembly:     assembly_id=GCF_000002305.6
  - sample:       sample_id=SRR1234567
  - attributes:   genome_size<3000000000, taxonomy=Mammalia, etc.
    ↓
Joined with " AND ":
  "tax_tree(Mammalia) AND tax_rank(species) AND genome_size<3000000000"
    ↓
Passed to build_search_body() as `query` parameter
    ↓
Elasticsearch query body:
{
  "query": {
    "bool": {
      "filter": [
        extracted terms from query string
      ]
    }
  },
  "size": 10,
  "from": 0,
  ...
}
```

### How Each Field Maps

| SearchQuery Field              | YAML Key                  | ES Component                           | Notes                                                        |
| ------------------------------ | ------------------------- | -------------------------------------- | ------------------------------------------------------------ |
| `identifiers.taxa`             | `taxa`                    | `bool.filter` / `term` on taxon        | Uses `tax_name()`, `tax_tree()`, or `tax_lineage()` function |
| `identifiers.taxa.filter_type` | `taxon_filter_type`       | Function wrapper                       | Determines which taxonomy traversal function                 |
| `identifiers.rank`             | `rank`                    | `bool.filter` / `term` on rank         | Filters by exact rank (e.g. "species")                       |
| `identifiers.assemblies`       | `assemblies`              | `bool.filter` / `terms` on assembly_id | Multiple assemblies: OR'd together                           |
| `identifiers.samples`          | `samples`                 | `bool.filter` / `terms` on sample_id   | Multiple samples: OR'd together                              |
| `attributes.attributes[*]`     | `attributes`              | `bool.filter` / `term`/`range`/etc.    | Operator determines clause type                              |
| `attributes.fields`            | `fields`                  | `aggs` nested aggregations             | Return columns + modifiers                                   |
| `attributes.names`             | `names`                   | `&names=` URL param (web API only)     | Name classes to include                                      |
| `attributes.ranks`             | `ranks`                   | `&ranks=` URL param (web API only)     | Rank columns to return                                       |
| `attributes.exclude_*`         | `exclude_ancestral`, etc. | `&excludeXxx=` URL params              | Exclude records by source type                               |

---

## 6. Valid YAML Examples

### Single Query: Taxon with Descendants

```yaml
index: taxon
taxa: [Mammalia]
taxon_filter_type: tree # Include all descendants
rank: species # Only return species rank
```

### Single Query: Taxon Exclusion (NOT filter)

```yaml
index: taxon
taxa: [Mammalia, "!Felis"]
taxon_filter_type: tree
```

### Single Query: With Attributes

```yaml
index: taxon
taxa: [Mammalia]
taxon_filter_type: name
rank: species
attributes:
  - name: genome_size
    operator: lt
    value: "3G"
    modifier: [min, direct]
fields:
  - name: genome_size
    modifier: [min]
  - name: assembly_level
```

### Multi-Query OR: Multiple Taxa

```yaml
index: taxon
combine_with: OR
queries:
  - taxa: [Mammalia]
    taxon_filter_type: tree
  - taxa: [Aves]
    taxon_filter_type: tree
```

### Multi-Query AND: Combine Filters

```yaml
index: taxon
combine_with: AND
queries:
  - taxa: [Mammalia]
    taxon_filter_type: tree
  - attributes:
      - name: genome_size
        operator: lt
        value: "3G"
```

### Single Query: Assembly by ID

```yaml
index: assembly
assemblies: ["GCF_000002305.6"]
fields:
  - name: assembly_level
```

### Single Query: All Fields (Permissive)

```yaml
index: taxon
taxa: [Homo]
taxon_filter_type: name
rank: species
fields:
  - name: genome_size
  - name: assembly_count
  - name: assembly_level
names: [scientific_name, common_name]
ranks: [phylum, class, order, family, genus]
attributes: []
```

---

## 7. YAML Deserialization Behavior

### Flattening (`#[serde(flatten)]`)

Both `identifiers` and `attributes` use `#[serde(flatten)]`, which means their fields appear at the top level of the YAML:

```yaml
# NOT like this (incorrect):
identifiers:
  taxa: [Mammalia]
  rank: species
attributes:
  fields:
    - name: genome_size

# But like this (correct):
taxa: [Mammalia]
rank: species
fields:
  - name: genome_size
```

### Default Values

Fields with `#[serde(default)]` are optional:

```yaml
# Minimal YAML (all other fields default)
index: taxon
taxa: [Mammalia]

# Equivalent to:
index: taxon
taxa: [Mammalia]
taxon_filter_type: name          # defaults to "name"
rank: null                       # defaults to None
attributes: []                   # defaults to empty
fields: []                       # defaults to empty
names: []                        # defaults to empty
ranks: []                        # defaults to empty
queries: null                    # stays None (single-query mode)
combine_with: AND                # defaults to AND (but only used in multi-query)
```

---

## 8. Known Issues and Gotchas

### Issue #1: Missing Taxon Fragment in API Search Handler ⚠️ **CRITICAL BUG**

**Location**: [crates/genomehubs-api/src/routes/search.rs](search.rs#L190)

**Problem**: When building the Elasticsearch query body from `SearchQuery`, the taxa identifiers are **not** being passed to `build_search_body()`:

```rust
// Current (WRONG):
let body = match cli_generator::core::query_builder::build_search_body(
    None,  // <-- Should pass query fragment here!
    if field_names.is_empty() { None } else { Some(field_names.as_slice()) },
    ...
```

**Expected**: Should pass a query string built from `nested_query.identifiers`:

```rust
// Should be:
let query_fragment = build_raw_query_fragment(
    &nested_query.identifiers,
    &nested_query.attributes
);
let body = match cli_generator::core::query_builder::build_search_body(
    Some(&query_fragment),  // <-- Pass the built query!
    ...
```

**Impact**: POST queries to `/api/v3/search` with taxa filters return **0 hits** even when data exists, because the taxa filter is completely ignored.

**Solution**: Import and use `build_raw_query_fragment()` from `genomehubs_query::query::url` to construct the query fragment, then pass it to `build_search_body()`.

### Gotcha #1: TaxonFilterType Defaults to `name`, not `tree`

```yaml
taxa: [Mammalia]
# Is NOT the same as:
taxa: [Mammalia]
taxon_filter_type: tree

# The first defaults to exact name match only, not descendants!
```

### Gotcha #2: Wildcards Not Supported in taxon_filter_type `name` Mode

```yaml
taxa: ["*bat*"]
taxon_filter_type: name # Exact name match mode
# This will NOT find records with "bat" in the name!
# You must use taxon_filter_type: tree or lineage
```

### Gotcha #3: Rank Filter vs. Rank Columns

Two different meanings of "rank":

```yaml
rank: species # FILTERS results to only species rank
ranks: [genus, family, order] # RETURNS these rank columns
```

They are completely independent.

### Gotcha #4: Field Modifiers and Attributes Behave Differently

```yaml
# Attribute modifier (filters which records are returned):
attributes:
  - name: genome_size
    modifier: [direct] # Exclude records with only inferred values

# Field modifier (how to aggregate the return value):
fields:
  - name: genome_size
    modifier: [min] # Return the minimum value across traversal
```

---

## 9. Query String Examples (Internal Format)

These are the internal query string formats that `build_search_body()` expects:

```
Single taxa:
  "tax_name(Homo)"
  "tax_tree(Mammalia)"
  "tax_lineage(Chordata)"

With rank filter:
  "tax_tree(Mammalia) AND tax_rank(species)"

With attribute filters:
  "tax_name(Homo) AND genome_size<3000000000"
  "tax_tree(Mammalia) AND min(genome_size)>1000"

With assembly/sample:
  "assembly_id=GCF_000002305.6"
  "sample_id=SRR1234567"

Combining multiple (all AND'd):
  "tax_tree(Mammalia) AND tax_rank(species) AND genome_size<3000000000 AND min(assembly_span)>10000"
```

---

## 10. From JSON to YAML Conversion

The `/api/v3/search` endpoint accepts both JSON and YAML formats:

```json
// JSON format (accepts query as object):
{
  "query": {
    "index": "taxon",
    "taxa": ["Mammalia"],
    "taxon_filter_type": "tree"
  },
  "params": {
    "size": 10
  }
}

// YAML format (accepts query as string):
{
  "query_yaml": "index: taxon\ntaxa:\n  - Mammalia\ntaxon_filter_type: tree",
  "params_yaml": "size: 10"
}
```

**Conversion Logic** (in `SearchRequest::deserialize()`):

1. If `query` key exists as JSON object → convert to YAML string via `serde_yaml::to_string()`
2. If `query_yaml` key exists as string → use as-is
3. Same for `params` / `params_yaml`
4. Pass both YAML strings to `SearchQuery::from_yaml()` and `QueryParams::from_yaml()`

---

## Summary of the Problem

**Why POST queries return 0 hits:**

1. ✅ JSON deserialization works: `{"taxa": ["9612"]}` → `SearchQuery` struct successfully
2. ✅ YAML parsing works: `query_yaml` is correctly parsed by `SearchQuery::from_yaml()`
3. ❌ **Query fragment is lost**: When building the ES body, `build_search_body()` is called with `query = None`
4. ❌ **ES body has no filters**: Elasticsearch receives `{ "query": { "match_all": {} } }` instead of a filtered query
5. ❌ **Result: 0 hits**: ES returns all documents (if size=0) or no matching documents

**Root cause**: Missing call to `build_raw_query_fragment()` in `search.rs` line ~190.
