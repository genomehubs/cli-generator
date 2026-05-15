//! Parser for two-column name→category mapping files.
//!
//! The file format is a tab-separated (or space-separated) text file with one
//! mapping per line: `feature_name<TAB>category_label`.  Leading `#` comment
//! lines and blank lines are skipped.  An optional header row (e.g.
//! `name<TAB>cat`) is automatically skipped when neither column contains a tab.
//!
//! This can be used to override the `cat` field on features parsed by the
//! BUSCO or other parsers — for example to assign merian units or orthogroup
//! clade labels based on BUSCO gene IDs.

use std::collections::HashMap;

/// Parse a two-column name→category mapping file.
///
/// Returns a `HashMap<feature_name, category_label>`.
///
/// - Lines starting with `#` are treated as comments and skipped.
/// - Lines with fewer than 2 tab-separated fields are skipped.
/// - If the first data line looks like a header (e.g. `name\tcat`), it is
///   included in the map — this is harmless because no real feature will have
///   the exact name of a header field.
pub fn parse_cat_file(content: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((name, cat)) = line.split_once('\t') {
            let name = name.trim();
            let cat = cat.trim();
            if !name.is_empty() && !cat.is_empty() {
                map.insert(name.to_string(), cat.to_string());
            }
        }
    }
    map
}

/// Parse a two-column name→category file and return a JSON object string.
///
/// Returns `{"name1":"cat1","name2":"cat2",...}` on success or
/// `{"error":"<message>"}` on serialisation failure.
pub fn parse_cat_file_json(content: &str) -> String {
    let map = parse_cat_file(content);
    serde_json::to_string(&map)
        .unwrap_or_else(|e| serde_json::json!({"error": e.to_string()}).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_two_col_tsv() {
        let content = "EOG001\tMZ-1\nEOG002\tMZ-2\nEOG003\tMZ-1\n";
        let map = parse_cat_file(content);
        assert_eq!(map.get("EOG001").map(String::as_str), Some("MZ-1"));
        assert_eq!(map.get("EOG002").map(String::as_str), Some("MZ-2"));
        assert_eq!(map.len(), 3);
    }

    #[test]
    fn skips_comment_lines() {
        let content = "# name\tcat\nEOG001\tClade-A\n# comment\nEOG002\tClade-B\n";
        let map = parse_cat_file(content);
        assert_eq!(map.len(), 2);
        assert!(!map.contains_key("# name"));
    }

    #[test]
    fn skips_blank_lines() {
        let content = "\nEOG001\tMZ-1\n\nEOG002\tMZ-2\n";
        let map = parse_cat_file(content);
        assert_eq!(map.len(), 2);
    }

    #[test]
    fn returns_json_object() {
        let content = "gene1\tred\ngene2\tblue\n";
        let json_str = parse_cat_file_json(content);
        let v: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(v["gene1"], "red");
        assert_eq!(v["gene2"], "blue");
    }
}
