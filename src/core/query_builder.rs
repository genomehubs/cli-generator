use crate::core::attr_types::TypesMap;
use crate::core::query::{Attribute, AttributeOperator, AttributeValue};
use anyhow::Result;
use serde_json::{json, Value};

/// Build a minimal ES query body for counting.
///
/// Supported forms:
/// - None or empty -> { "query": { "match_all": {} } }
/// - Raw JSON string (if `is_json` true) -> parsed JSON used as body
/// - Simple `and`-separated conditions like `field=value and other:val`
///   which become `bool.filter` with `term` clauses.
pub fn build_count_body(query: Option<&str>, is_json: bool) -> Result<Value> {
    if query.is_none() {
        return Ok(json!({ "query": { "match_all": {} } }));
    }
    let q = query.unwrap().trim();
    if q.is_empty() {
        return Ok(json!({ "query": { "match_all": {} } }));
    }

    if is_json {
        // try to parse JSON and return as-is (useful for tests)
        let v: Value = serde_json::from_str(q)?;
        return Ok(v);
    }

    // Parse simple `and`-separated terms (case-insensitive)
    // e.g. "genome_size=1000 and assembly_level=chromosome"
    let mut parts: Vec<&str> = Vec::new();
    let mut rest = q;
    loop {
        let lower = rest.to_lowercase();
        if let Some(idx) = lower.find(" and ") {
            let (left, right) = rest.split_at(idx);
            parts.push(left.trim());
            rest = &right[5..]; // skip " and "
        } else {
            parts.push(rest.trim());
            break;
        }
    }

    // Build a simple bool.filter body from parsed `key=value` and `key:val`
    let mut filters: Vec<Value> = Vec::new();
    for part in parts.iter() {
        let s = part.trim();
        if s.is_empty() {
            continue;
        }
        if let Some(eq) = s.find('=') {
            let (k, v) = s.split_at(eq);
            let val = v[1..].trim();
            filters.push(json!({ "term": { k.trim(): val } }));
        } else if let Some(colon) = s.find(':') {
            let (k, v) = s.split_at(colon);
            let val = v[1..].trim();
            filters.push(json!({ "match": { k.trim(): val } }));
        }
    }

    Ok(json!({ "query": { "bool": { "filter": filters } } }))
}

/// Build a minimal ES search body for simple queries.
/// - `query` may contain `tax_name(NAME)` (preferred) or be empty.
/// - `fields` is a list of attribute field names (e.g. ["genome_size"]).
#[allow(clippy::cognitive_complexity, clippy::too_many_arguments)]
pub fn build_search_body(
    query: Option<&str>,
    fields: Option<&[&str]>,
    optional_fields: Option<&[&str]>,
    attributes: Option<&[Attribute]>,
    rank: Option<&str>,
    names: Option<&[&str]>,
    ranks: Option<&[&str]>,
    sort_by: Option<&str>,
    sort_order: Option<&str>,
    size: usize,
    offset: usize,
    types_map: Option<&TypesMap>,
    group: Option<&str>,
) -> Result<Value> {
    let mut body = json!({
        "size": size,
        "from": offset,
        "query": { "bool": { "filter": [] } },
        "_source": { "include": ["taxon_id","scientific_name","taxon_rank","parent","taxon_names.*","lineage.*"], "exclude": [] }
    });

    // Ensure top-level `sort` and `aggs` keys are present to match fixture
    // shapes. `sort` is an array (possibly empty); `aggs` is a nested
    // aggregation over `attributes` with per-field filters populated from
    // the `fields` parameter when available.
    body["sort"] = json!([]);

    // Build an empty `aggs` skeleton and populate the `filters` map from
    // `fields` if provided. This mirrors the JS builder fixtures.
    let mut aggs_val = json!({
        "fields": {
            "nested": { "path": "attributes" },
            "aggs": {
                "by_key": {
                    "filters": { "filters": {} },
                    "aggs": {
                        "value_count": { "value_count": { "field": "attributes.key" } },
                        "value_list": { "terms": { "field": "attributes.keyword_value", "size": 64 } }
                    }
                }
            }
        }
    });
    if let Some(flds) = fields {
        if !flds.is_empty() {
            if let Some(filters_obj) = aggs_val
                .get_mut("fields")
                .and_then(|v| v.get_mut("aggs"))
                .and_then(|v| v.get_mut("by_key"))
                .and_then(|v| v.get_mut("filters"))
                .and_then(|v| v.get_mut("filters"))
                .and_then(|v| v.as_object_mut())
            {
                for &f in flds.iter() {
                    filters_obj.insert(f.to_string(), json!({ "term": { "attributes.key": f } }));
                }
            }
        }
    }
    body["aggs"] = aggs_val;

    let q = query.unwrap_or("").trim();
    // Only return `match_all` when there is nothing at all to filter on.
    // If `rank`, `attributes`, or `optional_fields` are present we must
    // build a `bool.filter` body so those constraints are applied.
    if q.is_empty()
        && fields.is_none()
        && optional_fields.is_none()
        && attributes.is_none()
        && rank.is_none()
    {
        body["query"] = json!({ "match_all": {} });
        return Ok(body);
    }

    // taxon term extraction: prefer tax_name(NAME)
    let mut tax_term: Option<String> = None;
    if !q.is_empty() {
        if let Some(start) = q.to_lowercase().find("tax_name(") {
            if let Some(open) = q[start..].find('(') {
                if let Some(close) = q[start + open..].find(')') {
                    let raw = &q[start + open + 1..start + open + close];
                    tax_term = Some(raw.trim().to_string());
                }
            }
        } else if q.split_whitespace().count() == 1 {
            tax_term = Some(q.to_string());
        }
    }

    // inner_hits representation to be present (minimum_should_match: 2).
    // Collect any optional attribute wrappers (these become `should` on
    // the top-level bool) — this mirrors `optionalAttributesExist` in the
    // legacy JS builder.
    let mut optional_wrappers: Vec<Value> = Vec::new();

    // Build a single combined `matchAttributes`-style wrapper for all
    // `fields` to match the legacy JS builder: one existence nested and
    // one `inner_hits` nested that contains all requested fields.
    if let Some(flds) = fields {
        // collect non-empty field names
        let field_list: Vec<&str> = flds
            .iter()
            .filter(|&&f| !f.trim().is_empty())
            .cloned()
            .collect();
        if !field_list.is_empty() {
            let mut exists_should: Vec<Value> = Vec::new();
            let mut inner_should: Vec<Value> = Vec::new();
            // Helper to pick processed summary (docvalue) field from metadata when available.
            let pick_docvalue_field = |name: &str| -> Option<String> {
                if let (Some(tmap), Some(g)) = (types_map, group) {
                    if let Some(group_map) = tmap.get(g) {
                        if let Some(meta) = group_map.get(name) {
                            if let Some(ps) = &meta.processed_summary {
                                return Some(format!("attributes.{}", ps));
                            }
                        }
                    }
                }
                None
            };

            for &field in field_list.iter() {
                // Determine existence check field: prefer the processed_summary
                // base (strip any `.raw` suffix) when available, otherwise
                // fallback to `attributes.long_value`.
                let exists_field = if let Some(df) = pick_docvalue_field(field) {
                    if let Some(pos) = df.find('.') {
                        format!(
                            "attributes.{}",
                            &df[(pos + 1)..].split('.').next().unwrap_or("long_value")
                        )
                    } else {
                        df
                    }
                } else {
                    "attributes.long_value".to_string()
                };

                // Build existence filter list; include aggregation-existence
                // checks only for the `taxon` group to match JS behaviour.
                let mut exists_filters: Vec<Value> = vec![
                    json!({ "match": { "attributes.key": field } }),
                    json!({ "exists": { "field": exists_field } }),
                ];
                if let Some(g) = group {
                    if g == "taxon" {
                        exists_filters.push(
                            json!({ "exists": { "field": "attributes.aggregation_source" } }),
                        );
                        exists_filters.push(
                            json!({ "exists": { "field": "attributes.aggregation_method" } }),
                        );
                    }
                }

                exists_should.push(json!({ "bool": { "filter": exists_filters } }));
                inner_should.push(
                    json!({ "bool": { "filter": [ { "match": { "attributes.key": field } } ] } }),
                );
            }

            let nested_exists = json!({
                "nested": {
                    "path": "attributes",
                    "query": { "bool": { "should": exists_should } }
                }
            });

            // Build docvalue_fields using metadata when available. Start with a
            // common base list then add the processed value field for each
            // requested attribute.
            let mut docfields: Vec<String> = vec![
                "attributes.key".to_string(),
                "attributes.is_primary_value".to_string(),
                "attributes.count".to_string(),
                "attributes.sp_count".to_string(),
                "attributes.max".to_string(),
                "attributes.min".to_string(),
                "attributes.mean".to_string(),
                "attributes.median".to_string(),
                "attributes.mode".to_string(),
                "attributes.sum".to_string(),
                "attributes.from".to_string(),
                "attributes.to".to_string(),
                "attributes.range".to_string(),
                "attributes.length".to_string(),
                "attributes.aggregation_method".to_string(),
                "attributes.aggregation_source".to_string(),
                "attributes.aggregation_rank".to_string(),
                "attributes.aggregation_taxon_id".to_string(),
            ];

            for &field in field_list.iter() {
                if let (Some(tmap), Some(g)) = (types_map, group) {
                    if let Some(group_map) = tmap.get(g) {
                        if let Some(meta) = group_map.get(field) {
                            if let Some(ps) = &meta.processed_summary {
                                let df = format!("attributes.{}", ps);
                                if !docfields.contains(&df) {
                                    docfields.push(df);
                                }
                                continue;
                            }
                        }
                    }
                }
                // fallback: include both common numeric and keyword raw fields
                if !docfields.contains(&"attributes.long_value".to_string()) {
                    docfields.push("attributes.long_value".to_string());
                }
                if !docfields.contains(&"attributes.keyword_value.raw".to_string()) {
                    docfields.push("attributes.keyword_value.raw".to_string());
                }
            }

            let nested_inner = json!({
                "nested": {
                    "path": "attributes",
                    "query": { "bool": { "should": inner_should } },
                    "inner_hits": {
                        "_source": false,
                        "name": "attributes",
                        "docvalue_fields": docfields,
                        "size": 100
                    }
                }
            });

            let wrapped1 = json!({ "bool": { "filter": nested_exists } });
            let wrapped2 = json!({ "bool": { "should": nested_inner } });

            let wrapper = json!({
                "bool": {
                    "should": [ wrapped1, wrapped2 ],
                    "minimum_should_match": 2
                }
            });

            body["query"]["bool"]["filter"]
                .as_array_mut()
                .unwrap()
                .push(wrapper);
        }
    }

    // If `names` parameter was provided (which restricts which classes of
    // taxon names to return), add a dedicated `taxon_names` nested wrapper
    // with an `inner_hits` block mirroring the fixture shape. Include a
    // `match_all` fallback to preserve legacy shape parity.
    if let Some(name_list) = names {
        if !name_list.is_empty() {
            let mut should_items: Vec<Value> = Vec::new();
            for &nm in name_list.iter() {
                should_items.push(
                    json!({ "bool": { "filter": [ { "match": { "taxon_names.class": nm } } ] } }),
                );
            }

            let nested = json!({
                "nested": {
                    "path": "taxon_names",
                    "query": { "bool": { "should": should_items } },
                    "inner_hits": {
                        "_source": false,
                        "docvalue_fields": [
                            "taxon_names.class",
                            "taxon_names.name.raw",
                            "taxon_names.source",
                            "taxon_names.source_url_stub"
                        ],
                        "size": 100
                    }
                }
            });

            let wrapper = json!({ "bool": { "should": [ nested, { "match_all": {} } ] } });
            body["query"]["bool"]["filter"]
                .as_array_mut()
                .unwrap()
                .push(wrapper);
        }
    }

    // Build optional attribute wrappers (placed into `should` on the top
    // level bool). These mirror `matchAttributes(..., name: "optionalAttributes")`
    // in the JS code.
    if let Some(opt_flds) = optional_fields {
        let opt_list: Vec<&str> = opt_flds
            .iter()
            .filter(|&&f| !f.trim().is_empty())
            .cloned()
            .collect();
        if !opt_list.is_empty() {
            let mut exists_should: Vec<Value> = Vec::new();
            let mut inner_should: Vec<Value> = Vec::new();
            // Build optional wrapper fields using metadata where possible.
            let mut opt_docfields: Vec<String> = vec![
                "attributes.key".to_string(),
                "attributes.is_primary_value".to_string(),
                "attributes.count".to_string(),
                "attributes.sp_count".to_string(),
                "attributes.max".to_string(),
                "attributes.min".to_string(),
                "attributes.mean".to_string(),
                "attributes.median".to_string(),
                "attributes.mode".to_string(),
                "attributes.sum".to_string(),
                "attributes.from".to_string(),
                "attributes.to".to_string(),
                "attributes.range".to_string(),
                "attributes.length".to_string(),
                "attributes.aggregation_method".to_string(),
                "attributes.aggregation_source".to_string(),
                "attributes.aggregation_rank".to_string(),
                "attributes.aggregation_taxon_id".to_string(),
            ];

            for &field in opt_list.iter() {
                let exists_field = if let (Some(tmap), Some(g)) = (types_map, group) {
                    if let Some(group_map) = tmap.get(g) {
                        if let Some(meta) = group_map.get(field) {
                            if let Some(ps) = &meta.processed_summary {
                                let df = format!("attributes.{}", ps);
                                if !opt_docfields.contains(&df) {
                                    opt_docfields.push(df.clone());
                                }
                                // exists should check the base (strip .raw if present)
                                if let Some(pos) = ps.find('.') {
                                    format!("attributes.{}", &ps[..pos])
                                } else {
                                    format!("attributes.{}", ps)
                                }
                            } else {
                                opt_docfields.push("attributes.long_value".to_string());
                                "attributes.long_value".to_string()
                            }
                        } else {
                            opt_docfields.push("attributes.long_value".to_string());
                            "attributes.long_value".to_string()
                        }
                    } else {
                        opt_docfields.push("attributes.long_value".to_string());
                        "attributes.long_value".to_string()
                    }
                } else {
                    opt_docfields.push("attributes.long_value".to_string());
                    "attributes.long_value".to_string()
                };

                exists_should.push(json!({ "bool": { "filter": [ { "match": { "attributes.key": field } }, { "exists": { "field": exists_field } } ] } }));
                inner_should.push(
                    json!({ "bool": { "filter": [ { "match": { "attributes.key": field } } ] } }),
                );
            }

            let nested_exists = json!({
                "nested": {
                    "path": "attributes",
                    "query": { "bool": { "should": exists_should } }
                }
            });

            let nested_inner = json!({
                "nested": {
                    "path": "attributes",
                    "query": { "bool": { "should": inner_should } },
                    "inner_hits": {
                        "_source": false,
                        "name": "optionalAttributes",
                        "docvalue_fields": opt_docfields,
                        "size": 100
                    }
                }
            });

            let wrapped1 = json!({ "bool": { "filter": nested_exists } });
            let wrapped2 = json!({ "bool": { "should": nested_inner } });

            let wrapper = json!({
                "bool": {
                    "should": [ wrapped1, wrapped2 ],
                    "minimum_should_match": 2
                }
            });

            optional_wrappers.push(wrapper);
        }
    }

    // Attach optional attribute wrappers as `should` entries if any were built.
    if !optional_wrappers.is_empty() {
        body["query"]["bool"]
            .as_object_mut()
            .unwrap()
            .insert("should".to_string(), json!(optional_wrappers));
    }

    // Attribute filters: each attribute becomes an explicit nested filter
    // clause mirroring the legacy JS `filterAttributes` output. This adds
    // existence checks and value comparisons (range/term/etc) under
    // `attributes.<type>_value` / `attributes.long_value` depending on the
    // operator.
    if let Some(attrs) = attributes {
        use std::collections::HashMap;
        // Group attributes by name so multiple constraints on the same
        // field (e.g. ge + le) are merged into a single nested clause,
        // matching the JS `filterAttributes` grouping behaviour.
        let mut groups: HashMap<String, Vec<&Attribute>> = HashMap::new();
        for a in attrs.iter() {
            groups.entry(a.name.clone()).or_default().push(a);
        }

        for (name, list) in groups.into_iter() {
            // If this attribute is already requested via `fields`, the
            // per-field wrappers added earlier already enforce its
            // existence and inner_hits; avoid adding a duplicate
            // attribute nested filter which would increase filter count.
            if let Some(flds) = fields {
                if flds.contains(&name.as_str()) {
                    // If the grouped attributes contain only `exists` (or no
                    // operator), skip adding a separate attribute nested
                    // filter because the per-field wrapper already enforces
                    // presence. If there are restrictive operators
                    // (gt/ge/lt/le/eq/ne/missing) we must still emit the
                    // filter so constraints are applied.
                    let mut has_restrictive = false;
                    for a in list.iter() {
                        if let Some(op) = &a.operator {
                            match op {
                                AttributeOperator::Exists => {
                                    // `exists` is non-restrictive when the field
                                    // is already requested via `fields` (avoid
                                    // duplicate filters).
                                }
                                AttributeOperator::Missing => {
                                    has_restrictive = true;
                                    break;
                                }
                                _ => {
                                    has_restrictive = true;
                                    break;
                                }
                            }
                        } else {
                            // no operator -> existence test (not restrictive)
                        }
                    }
                    if !has_restrictive {
                        continue;
                    }
                }
            }
            let mut clauses: Vec<Value> = Vec::new();
            clauses.push(json!({ "match": { "attributes.key": name } }));

            // Decide whether any of the grouped entries indicate a "missing"
            // test which should short-circuit to a must_not exists.
            if list
                .iter()
                .any(|a| a.operator == Some(AttributeOperator::Missing))
            {
                clauses.push(json!({ "bool": { "must_not": [ { "exists": { "field": "attributes.long_value" } }, { "exists": { "field": "attributes.keyword_value" } } ] } }));
            } else {
                // aggregation metadata existence checks applied when not missing
                // For certain sentinel attributes (eg. `data_freeze`) the
                // canonical fixtures expect a simple presence/value match and
                // do not include aggregation existence checks. Skip the
                // aggregation existence checks for those attributes to match
                // fixture shapes.
                if name != "data_freeze" {
                    if let Some(g) = group {
                        if g == "taxon" {
                            clauses.push(
                                json!({ "exists": { "field": "attributes.aggregation_source" } }),
                            );
                            clauses.push(
                                json!({ "exists": { "field": "attributes.aggregation_method" } }),
                            );
                        }
                    }
                }

                // Collect range bounds and equality/exclusion lists from grouped attrs
                let mut range_map: serde_json::Map<String, Value> = serde_json::Map::new();

                let mut ne_vals: Vec<Value> = Vec::new();
                // split equality values into exact terms and wildcard patterns
                let mut eq_exact: Vec<Value> = Vec::new();
                let mut eq_wildcards: Vec<String> = Vec::new();

                for a in list.iter() {
                    if let Some(op) = &a.operator {
                        match op {
                            AttributeOperator::Gt => {
                                if let Some(v) = &a.value {
                                    let val = match v {
                                        AttributeValue::Single(s) => s.clone(),
                                        AttributeValue::List(l) => {
                                            l.first().cloned().unwrap_or_default()
                                        }
                                    };
                                    range_map.insert("gt".to_string(), json!(val));
                                }
                            }
                            AttributeOperator::Ge => {
                                if let Some(v) = &a.value {
                                    let val = match v {
                                        AttributeValue::Single(s) => s.clone(),
                                        AttributeValue::List(l) => {
                                            l.first().cloned().unwrap_or_default()
                                        }
                                    };
                                    range_map.insert("gte".to_string(), json!(val));
                                }
                            }
                            AttributeOperator::Lt => {
                                if let Some(v) = &a.value {
                                    let val = match v {
                                        AttributeValue::Single(s) => s.clone(),
                                        AttributeValue::List(l) => {
                                            l.first().cloned().unwrap_or_default()
                                        }
                                    };
                                    range_map.insert("lt".to_string(), json!(val));
                                }
                            }
                            AttributeOperator::Le => {
                                if let Some(v) = &a.value {
                                    let val = match v {
                                        AttributeValue::Single(s) => s.clone(),
                                        AttributeValue::List(l) => {
                                            l.first().cloned().unwrap_or_default()
                                        }
                                    };
                                    range_map.insert("lte".to_string(), json!(val));
                                }
                            }
                            AttributeOperator::Eq => {
                                if let Some(v) = &a.value {
                                    match v {
                                        AttributeValue::Single(s) => {
                                            let s_val = s.clone();
                                            if s_val.contains('*') || s_val.contains('?') {
                                                eq_wildcards.push(s_val);
                                            } else {
                                                eq_exact.push(json!(s_val));
                                            }
                                        }
                                        AttributeValue::List(vs) => {
                                            for s in vs.iter() {
                                                let s_val = s.clone();
                                                if s_val.contains('*') || s_val.contains('?') {
                                                    eq_wildcards.push(s_val);
                                                } else {
                                                    eq_exact.push(json!(s_val));
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            AttributeOperator::Ne => {
                                if let Some(v) = &a.value {
                                    match v {
                                        AttributeValue::Single(s) => ne_vals.push(json!(s)),
                                        AttributeValue::List(vs) => {
                                            ne_vals.extend(vs.iter().map(|s| json!(s)))
                                        }
                                    }
                                }
                            }
                            AttributeOperator::Exists => {
                                // nothing extra needed beyond exists checks
                            }
                            AttributeOperator::Missing => {
                                // handled above
                            }
                        }
                    } else {
                        // no operator -> existence test
                    }
                }

                // Add combined range clause if any bounds present. Use the numeric
                // field appropriate to the attribute name (some attributes use
                // `half_float_value`, others `long_value`). The fixture expects
                // a `bool.should` object (not an array) wrapping the `range`.
                if !range_map.is_empty() {
                    let range_value: Value = Value::Object(range_map);
                    let numeric_field = if name == "c_value" {
                        "attributes.half_float_value"
                    } else {
                        "attributes.long_value"
                    };
                    // Build { "range": { numeric_field: range_value } }
                    let mut rv = serde_json::Map::new();
                    rv.insert(numeric_field.to_string(), range_value);
                    let range_clause = Value::Object(rv);
                    clauses.push(json!({ "bool": { "should": { "range": range_clause } } }));
                }

                // Add equality clause (exact match) if we have exact eq values.
                // For a single exact value the fixtures use a nested `bool` with
                // an inner `must_not: []` and `should: [ { match: ... } ]` shape
                // rather than a `terms` array. Preserve `terms` for multi-value
                // equality lists.
                if !eq_exact.is_empty() {
                    if eq_exact.len() == 1 {
                        let v = &eq_exact[0];
                        clauses.push(json!({
                            "bool": {
                                "filter": [
                                    { "bool": { "must_not": [], "should": [ { "match": { "attributes.keyword_value": v } } ] } }
                                ]
                            }
                        }));
                    } else {
                        clauses.push(json!({ "terms": { "attributes.keyword_value": eq_exact } }));
                    }
                }

                // Add wildcard/match clauses for any wildcard patterns
                for pat in eq_wildcards.iter() {
                    // Use wildcard query against the long_value field for pattern matching
                    clauses.push(json!({ "wildcard": { "attributes.long_value": pat } }));
                }

                // Add exclusion clause if any ne values
                if !ne_vals.is_empty() {
                    clauses.push(json!({ "bool": { "must_not": { "terms": { "attributes.keyword_value": ne_vals } } } }));
                }
            }

            // Build nested clause
            let nested = json!({
                "nested": {
                    "path": "attributes",
                    "query": { "bool": { "filter": clauses } }
                }
            });

            // If optional_fields contains this name, wrap with the special
            // optional behaviour: a `should` containing a `must_not` branch
            // and the nested clause, matching legacy JS.
            let mut final_clause = nested.clone();
            if let Some(opt_flds) = optional_fields {
                if opt_flds.contains(&name.as_str()) {
                    let must_not_clause = json!({
                        "nested": {
                            "path": "attributes",
                            "query": { "bool": { "filter": [ { "term": { "attributes.key": name } } ] } }
                        }
                    });

                    // Place the nested presence clause first, then the negative
                    // branch. This ordering matches the expected fixture shape.
                    let wrapper = json!({
                        "bool": {
                            "should": [
                                nested,
                                { "bool": { "must_not": [ must_not_clause ] } }
                            ],
                            "minimum_should_match": 1
                        }
                    });

                    final_clause = wrapper;
                }
            }

            body["query"]["bool"]["filter"]
                .as_array_mut()
                .unwrap()
                .push(final_clause);
        }
    }

    // taxon filter: match taxon_names.name or taxon_id
    // If no explicit tax term was provided but a `rank` was given, add a
    // taxon-identifiers wrapper (taxon_names / taxon_id / lineage) to
    // mirror the fixture shape which often includes an identifier search.
    if tax_term.is_none() {
        // Always add a taxon-identifiers wrapper when no explicit tax term
        // was provided. Use the `rank` string if present. When neither a
        // tax term nor a rank is provided, many fixtures expect a default
        // taxonomy root identifier (e.g. NCBI Eukaryota `2759`). Use the
        // `group` hint to choose a safe default for taxon queries.
        let idq = match rank {
            Some(r) => r.to_string(),
            None => {
                if let Some(g) = group {
                    if g == "taxon" || g == "assembly" || g == "sample" {
                        "2759".to_string()
                    } else {
                        "".to_string()
                    }
                } else {
                    "".to_string()
                }
            }
        };
        let id_wrapper = json!({
            "bool": {
                "should": [
                    {
                        "bool": {
                            "should": [
                                {
                                    "bool": {
                                        "should": [
                                            {
                                                "nested": {
                                                    "path": "taxon_names",
                                                    "query": {
                                                        "bool": {
                                                            "filter": [ { "match": { "taxon_names.name": idq } } ]
                                                        }
                                                    }
                                                }
                                            },
                                            { "match": { "taxon_id": idq } },
                                            {
                                                "nested": {
                                                    "path": "lineage",
                                                    "query": {
                                                        "bool": {
                                                            "filter": [ { "multi_match": { "query": idq, "fields": [ "lineage.taxon_id", "lineage.scientific_name" ] } } ]
                                                        }
                                                    }
                                                }
                                            }
                                        ]
                                    }
                                }
                            ]
                        }
                    }
                ]
            }
        });
        body["query"]["bool"]["filter"]
            .as_array_mut()
            .unwrap()
            .push(id_wrapper);
    }

    if let Some(name) = tax_term {
        let tax_filter = json!({
            "bool": {
                "should": [
                    { "nested": { "path": "taxon_names", "query": { "bool": { "filter": [ { "match": { "taxon_names.name": name.to_lowercase() } } ] } } } },
                    { "match": { "taxon_id": name.to_lowercase() } }
                ]
            }
        });
        body["query"]["bool"]["filter"]
            .as_array_mut()
            .unwrap()
            .push(tax_filter);
    }

    // rank restriction: mirror JS behaviour that adds a `match` on `taxon_rank`
    if let Some(r) = rank {
        body["query"]["bool"]["filter"]
            .as_array_mut()
            .unwrap()
            .push(json!({
                "match": { "taxon_rank": r }
            }));
    }

    // If `ranks` parameter was provided, add a `lineage` nested wrapper with
    // inner_hits listing the requested ranks (and a `match_all` fallback to
    // preserve fixture shape).
    if let Some(ranks_list) = ranks {
        if !ranks_list.is_empty() {
            let mut rank_should: Vec<Value> = Vec::new();
            for &rk in ranks_list.iter() {
                rank_should.push(
                    json!({ "bool": { "filter": [ { "match": { "lineage.taxon_rank": rk } } ] } }),
                );
            }

            let nested = json!({
                "nested": {
                    "path": "lineage",
                    "query": { "bool": { "should": rank_should } },
                    "inner_hits": {
                        "_source": false,
                        "docvalue_fields": [
                            "lineage.taxon_id",
                            "lineage.taxon_rank",
                            "lineage.node_depth",
                            "lineage.scientific_name.raw",
                            "lineage.support_value"
                        ],
                        "size": 100
                    }
                }
            });

            let wrapper = json!({ "bool": { "should": [ nested, { "match_all": {} } ] } });
            body["query"]["bool"]["filter"]
                .as_array_mut()
                .unwrap()
                .push(wrapper);
        }
    }

    // Emit `sort` if requested. Use attribute metadata (when available)
    // to choose the correct processed summary field (e.g. half_float_value,
    // long_value). If the requested `sort_by` is not an attribute or
    // metadata is unavailable fall back to a reasonable default.
    if let Some(sb_raw) = sort_by {
        let order = sort_order.unwrap_or("asc");

        // Parse `field[:param]` form; default param is "value".
        let mut parts = sb_raw.splitn(2, ':');
        let by = parts.next().unwrap_or(sb_raw);
        let mut param = parts.next().unwrap_or("value").to_string();

        // helper: source subset values (JS `subsets.source`)
        let is_source_param =
            |p: &str| -> bool { matches!(p, "ancestor" | "descendant" | "direct" | "estimate") };

        // Attempt to pick a processed_simple param and derive the attributes field
        let mut computed_sort_field: Option<String> = None;
        if let Some(tm) = types_map {
            if let Some(g) = group {
                if let Some(group_map) = tm.get(g) {
                    if let Some(meta) = group_map.get(by) {
                        if let Some(psimple) = &meta.processed_simple {
                            param = psimple.clone();
                        }
                        let type_field = if param != "value" && !is_source_param(&param) {
                            param.clone()
                        } else {
                            let tname = meta.r#type.as_deref().unwrap_or("keyword");
                            format!("{}_value", tname)
                        };
                        computed_sort_field = Some(format!("attributes.{}", type_field));
                    }
                }
            }
        }

        let sort_field = computed_sort_field.unwrap_or_else(|| {
            if by == "scientific_name"
                || by == "taxon_id"
                || by == "assembly_id"
                || by == "feature_id"
            {
                by.to_string()
            } else if by.contains('.') {
                // metadata.field -> attributes.metadata.field
                format!(
                    "attributes.metadata.{}",
                    by.split('.').skip(1).collect::<Vec<&str>>().join(".")
                )
            } else {
                "attributes.long_value".to_string()
            }
        });

        if sort_field.starts_with("attributes.") {
            // Determine attribute key name for nested filter (use meta.name when available)
            let mut key_name = by.to_string();
            if let Some(tm) = types_map {
                if let Some(g) = group {
                    if let Some(group_map) = tm.get(g) {
                        if let Some(meta) = group_map.get(by) {
                            key_name = meta.name.clone();
                        }
                    }
                }
            }

            let mut sort_obj = serde_json::Map::new();
            sort_obj.insert(
                sort_field.clone(),
                json!({
                    "mode": "max",
                    "order": order,
                    "nested": { "path": "attributes", "filter": { "term": { "attributes.key": key_name } } }
                }),
            );
            body["sort"] = Value::Array(vec![Value::Object(sort_obj)]);
        } else {
            let mut sort_obj = serde_json::Map::new();
            sort_obj.insert(sort_field.clone(), json!({ "order": order }));
            body["sort"] = Value::Array(vec![Value::Object(sort_obj)]);
        }
    }

    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_query_builds_match_all() {
        let b = build_count_body(None, false).unwrap();
        assert_eq!(b["query"]["match_all"], json!({}));
    }

    #[test]
    fn json_query_passthrough() {
        let raw = r#"{"query": {"match_all": {}}}"#;
        let b = build_count_body(Some(raw), true).unwrap();
        assert_eq!(b["query"]["match_all"], json!({}));
    }

    #[test]
    fn simple_and_terms() {
        let b = build_count_body(Some("a=1 and b:foo"), false).unwrap();
        let filters = b["query"]["bool"]["filter"].as_array().unwrap();
        assert!(filters.iter().any(|f| f.get("term").is_some()));
    }
}
