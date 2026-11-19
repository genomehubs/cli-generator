//! Cumulative branch-length depths for tree reports.
//!
//! A node's `branch_length` is the length of the edge to its parent. Depth is the
//! sum of edge lengths from the root, so the root has depth 0 and its own length is
//! not used. A missing length counts as 1.0, which makes an all-default tree report
//! hop counts.

use std::collections::HashMap;

/// One node of a tree to lay out.
#[derive(Debug, Clone, PartialEq)]
pub struct TreeLayoutNode {
    /// Taxon identifier, unique within the tree.
    pub taxon_id: String,
    /// Parent taxon identifier; `None` for the root.
    pub parent_id: Option<String>,
    /// Length of the edge to the parent; `None` means 1.0.
    pub branch_length: Option<f64>,
}

/// Compute the cumulative branch-length depth of every node, keyed by `taxon_id`.
///
/// Every `branch_length` must be finite and non-negative. Returns an error for an
/// invalid length, a parent that is not in `nodes`, or a cycle.
pub fn cumulative_depths(nodes: &[TreeLayoutNode]) -> Result<HashMap<String, f64>, String> {
    for node in nodes {
        edge_length(node)?;
    }
    let by_id: HashMap<&str, &TreeLayoutNode> = nodes
        .iter()
        .map(|node| (node.taxon_id.as_str(), node))
        .collect();
    nodes
        .iter()
        .map(|node| -> Result<(String, f64), String> {
            Ok((node.taxon_id.clone(), depth_from_root(node, &by_id)?))
        })
        .collect()
}

/// Sum edge lengths from `start` up to the root.
fn depth_from_root(
    start: &TreeLayoutNode,
    by_id: &HashMap<&str, &TreeLayoutNode>,
) -> Result<f64, String> {
    let mut depth = 0.0;
    let mut current = start;
    // A path longer than the node count must revisit a node, so it contains a cycle.
    for _ in 0..=by_id.len() {
        let Some(parent_id) = current.parent_id.as_deref() else {
            return Ok(depth);
        };
        depth += edge_length(current)?;
        current = by_id
            .get(parent_id)
            .copied()
            .ok_or_else(|| format!("unknown parent '{parent_id}'"))?;
    }
    Err(format!("cycle detected at '{}'", start.taxon_id))
}

/// Return the edge length of `node`, defaulting to 1.0 when unset.
fn edge_length(node: &TreeLayoutNode) -> Result<f64, String> {
    let length = node.branch_length.unwrap_or(1.0);
    if length.is_finite() && length >= 0.0 {
        return Ok(length);
    }
    Err(format!(
        "branch_length for '{}' must be finite and non-negative, got {length}",
        node.taxon_id
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn node(taxon_id: &str, parent_id: Option<&str>, branch_length: Option<f64>) -> TreeLayoutNode {
        TreeLayoutNode {
            taxon_id: taxon_id.to_string(),
            parent_id: parent_id.map(str::to_string),
            branch_length,
        }
    }

    #[test]
    fn explicit_lengths_accumulate_from_root() {
        let nodes = vec![
            node("root", None, None),
            node("a", Some("root"), Some(2.5)),
            node("b", Some("a"), Some(0.5)),
        ];

        let depths = cumulative_depths(&nodes).unwrap();

        assert_eq!(depths["root"], 0.0);
        assert_eq!(depths["a"], 2.5);
        assert_eq!(depths["b"], 3.0);
    }

    #[test]
    fn root_branch_length_is_not_counted() {
        let nodes = vec![node("root", None, Some(9.0)), node("a", Some("root"), None)];

        let depths = cumulative_depths(&nodes).unwrap();

        assert_eq!(depths["root"], 0.0);
        assert_eq!(depths["a"], 1.0);
    }

    #[test]
    fn negative_branch_length_is_error() {
        let nodes = vec![
            node("root", None, None),
            node("a", Some("root"), Some(-1.0)),
        ];

        let err = cumulative_depths(&nodes).unwrap_err();

        assert!(err.contains("'a'"), "unexpected error: {err}");
    }

    #[test]
    fn non_finite_branch_length_is_error() {
        for length in [f64::NAN, f64::INFINITY] {
            let nodes = vec![
                node("root", None, None),
                node("a", Some("root"), Some(length)),
            ];

            assert!(
                cumulative_depths(&nodes).is_err(),
                "length {length} was accepted"
            );
        }
    }

    #[test]
    fn unknown_parent_is_error() {
        let nodes = vec![node("a", Some("missing"), None)];

        let err = cumulative_depths(&nodes).unwrap_err();

        assert!(err.contains("missing"), "unexpected error: {err}");
    }

    #[test]
    fn cycle_is_error() {
        let nodes = vec![node("a", Some("b"), None), node("b", Some("a"), None)];

        let err = cumulative_depths(&nodes).unwrap_err();

        assert!(err.contains("cycle"), "unexpected error: {err}");
    }

    proptest! {
        #[test]
        fn default_lengths_give_hop_depth(chain_length in 1usize..200) {
            let nodes: Vec<TreeLayoutNode> = (0..chain_length)
                .map(|index| TreeLayoutNode {
                    taxon_id: format!("t{index}"),
                    parent_id: index.checked_sub(1).map(|parent| format!("t{parent}")),
                    branch_length: None,
                })
                .collect();

            let depths = cumulative_depths(&nodes).unwrap();

            for (index, layout_node) in nodes.iter().enumerate() {
                prop_assert_eq!(depths[&layout_node.taxon_id], index as f64);
            }
        }
    }
}
