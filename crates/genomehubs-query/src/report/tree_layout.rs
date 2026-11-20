//! Cumulative branch-length depths for tree reports.
//!
//! A node's `branch_length` is the length of the edge to its parent. Depth is the
//! sum of edge lengths from the root, so the root has depth 0 and its own length is
//! not used. A missing length counts as 1.0, which makes an all-default tree report
//! hop counts.

use std::collections::{HashMap, HashSet};

/// How tip positions are chosen for a tree report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TipAlignment {
    /// Depths follow branch lengths and tip labels sit at their tips.
    #[default]
    None,
    /// Branch lengths are ignored and every tip sits at the same depth.
    Tips,
    /// Branch lengths are kept and tip labels sit in a column joined by dotted leaders.
    Labels,
    /// Branch lengths are kept and terminal branches extend so all tips share one depth.
    Ultrametric,
}

impl TipAlignment {
    /// Parse the `tip_alignment` display option; a missing value means `None`.
    pub fn parse(value: Option<&str>) -> Result<Self, String> {
        match value {
            None | Some("none") => Ok(Self::None),
            Some("tips") => Ok(Self::Tips),
            Some("labels") => Ok(Self::Labels),
            Some("ultrametric") => Ok(Self::Ultrametric),
            Some(other) => Err(format!("unknown tip_alignment: {other}")),
        }
    }
}

/// How circular tip labels are rotated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LabelOrientation {
    /// Labels read outward, and the left half is turned around so the text stays upright.
    #[default]
    Flip,
    /// Labels read outward all the way round, so the left half reads upside down.
    Continuous,
}

impl LabelOrientation {
    /// Parse the `label_orientation` display option; a missing value means `Flip`.
    pub fn parse(value: Option<&str>) -> Result<Self, String> {
        match value {
            None | Some("flip") => Ok(Self::Flip),
            Some("continuous") => Ok(Self::Continuous),
            Some(other) => Err(format!("unknown label_orientation: {other}")),
        }
    }
}

/// Display options shared by every tree layout.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct TreeRenderOptions {
    /// Tip placement for `"rect"`, `"circ"`, and `"radial"` layouts.
    pub alignment: TipAlignment,
    /// Label rotation for `"circ"`, `"radial"`, and `"ring"` layouts.
    pub label_orientation: LabelOrientation,
    /// Label font size in pixels; `None` keeps each layout's default size.
    pub font_size: Option<f64>,
}

impl TreeRenderOptions {
    /// Options with the given tip alignment and every other option at its default.
    pub fn with_alignment(alignment: TipAlignment) -> Self {
        Self {
            alignment,
            ..Self::default()
        }
    }
}

/// Resolve the label font size in pixels from `font_size` or `font_scale`.
///
/// `font_scale` is a share of the shorter canvas side. Returns an error when both
/// are set or either value is not a positive finite number.
pub fn resolve_font_size(
    font_size: Option<f64>,
    font_scale: Option<f64>,
    width: f64,
    height: f64,
) -> Result<Option<f64>, String> {
    match (font_size, font_scale) {
        (Some(_), Some(_)) => Err("set either font_size or font_scale, not both".to_string()),
        (Some(px), None) => positive(px, "font_size").map(Some),
        (None, Some(scale)) => {
            positive(scale, "font_scale").map(|scale| Some(scale * width.min(height)))
        }
        (None, None) => Ok(None),
    }
}

/// Return `value` when it is a positive finite number, otherwise an error naming `name`.
fn positive(value: f64, name: &str) -> Result<f64, String> {
    if value.is_finite() && value > 0.0 {
        return Ok(value);
    }
    Err(format!("{name} must be a positive number, got {value}"))
}

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

/// Compute tip-aligned depths that ignore branch lengths, keyed by `taxon_id`.
///
/// Each node sits at `max_height - height`, where `height` is the most hops from
/// the node down to a tip. Every tip therefore shares the maximum depth, and
/// shorter internal branches stretch to reach it.
pub fn tip_aligned_depths(nodes: &[TreeLayoutNode]) -> Result<HashMap<String, f64>, String> {
    let hop_nodes: Vec<TreeLayoutNode> = nodes
        .iter()
        .map(|node| TreeLayoutNode {
            branch_length: None,
            ..node.clone()
        })
        .collect();
    let hop_depths = cumulative_depths(&hop_nodes)?;
    let mut deepest_first: Vec<&TreeLayoutNode> = nodes.iter().collect();
    deepest_first.sort_by(|a, b| hop_depths[&b.taxon_id].total_cmp(&hop_depths[&a.taxon_id]));
    let mut heights: HashMap<&str, f64> = nodes
        .iter()
        .map(|node| (node.taxon_id.as_str(), 0.0))
        .collect();
    // Every child is strictly deeper than its parent, so children settle before their parent reads them.
    for node in deepest_first {
        let Some(parent_id) = node.parent_id.as_deref() else {
            continue;
        };
        let candidate = heights[node.taxon_id.as_str()] + 1.0;
        if let Some(parent_height) = heights.get_mut(parent_id) {
            *parent_height = parent_height.max(candidate);
        }
    }
    let max_height = heights.values().copied().fold(0.0, f64::max);
    Ok(heights
        .into_iter()
        .map(|(taxon_id, height)| (taxon_id.to_string(), max_height - height))
        .collect())
}

/// Compute cumulative depths with every tip extended to the deepest tip's depth.
///
/// Internal nodes keep their cumulative depth; only terminal branches are lengthened.
pub fn ultrametric_depths(nodes: &[TreeLayoutNode]) -> Result<HashMap<String, f64>, String> {
    let mut depths = cumulative_depths(nodes)?;
    let parent_ids: HashSet<&str> = nodes
        .iter()
        .filter_map(|node| node.parent_id.as_deref())
        .collect();
    let tip_ids: Vec<&str> = nodes
        .iter()
        .map(|node| node.taxon_id.as_str())
        .filter(|taxon_id| !parent_ids.contains(taxon_id))
        .collect();
    let deepest_tip = tip_ids
        .iter()
        .map(|taxon_id| depths[*taxon_id])
        .fold(0.0, f64::max);
    for taxon_id in tip_ids {
        depths.insert(taxon_id.to_string(), deepest_tip);
    }
    Ok(depths)
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

    #[test]
    fn label_orientation_parses_known_values_and_rejects_others() {
        assert_eq!(LabelOrientation::parse(None), Ok(LabelOrientation::Flip));
        assert_eq!(
            LabelOrientation::parse(Some("flip")),
            Ok(LabelOrientation::Flip)
        );
        assert_eq!(
            LabelOrientation::parse(Some("continuous")),
            Ok(LabelOrientation::Continuous)
        );
        assert!(LabelOrientation::parse(Some("upside")).is_err());
    }

    #[test]
    fn font_size_is_absolute_or_scaled_to_shorter_side() {
        assert_eq!(resolve_font_size(None, None, 800.0, 600.0), Ok(None));
        assert_eq!(
            resolve_font_size(Some(7.0), None, 800.0, 600.0),
            Ok(Some(7.0))
        );
        assert_eq!(
            resolve_font_size(None, Some(0.02), 800.0, 600.0),
            Ok(Some(12.0))
        );
    }

    #[test]
    fn font_size_rejects_both_options_and_non_positive_values() {
        assert!(resolve_font_size(Some(7.0), Some(0.02), 800.0, 600.0).is_err());
        assert!(resolve_font_size(Some(0.0), None, 800.0, 600.0).is_err());
        assert!(resolve_font_size(None, Some(f64::NAN), 800.0, 600.0).is_err());
    }

    #[test]
    fn tip_alignment_parses_known_values_and_rejects_others() {
        assert_eq!(TipAlignment::parse(None), Ok(TipAlignment::None));
        assert_eq!(TipAlignment::parse(Some("none")), Ok(TipAlignment::None));
        assert_eq!(TipAlignment::parse(Some("tips")), Ok(TipAlignment::Tips));
        assert_eq!(
            TipAlignment::parse(Some("labels")),
            Ok(TipAlignment::Labels)
        );
        assert_eq!(
            TipAlignment::parse(Some("ultrametric")),
            Ok(TipAlignment::Ultrametric)
        );
        assert!(TipAlignment::parse(Some("sideways")).is_err());
    }

    #[test]
    fn tip_aligned_depths_ignore_branch_lengths_and_stretch_internal_nodes() {
        let nodes = vec![
            node("root", None, None),
            TreeLayoutNode {
                taxon_id: "a".to_string(),
                parent_id: Some("root".to_string()),
                branch_length: Some(5.0),
            },
            node("a1", Some("a"), Some(1.0)),
            node("b", Some("root"), Some(1.0)),
        ];

        let depths = tip_aligned_depths(&nodes).unwrap();

        assert_eq!(depths["root"], 0.0);
        assert_eq!(depths["a"], 1.0);
        assert_eq!(depths["a1"], 2.0);
        assert_eq!(depths["b"], 2.0);
    }

    #[test]
    fn tip_aligned_depths_reject_invalid_tree() {
        let nodes = vec![node("a", Some("missing"), None)];

        assert!(tip_aligned_depths(&nodes).is_err());
    }

    #[test]
    fn ultrametric_extends_only_terminal_branches() {
        let nodes = vec![
            node("root", None, None),
            node("a", Some("root"), Some(5.0)),
            node("a1", Some("a"), Some(1.0)),
            node("b", Some("root"), Some(1.0)),
        ];

        let depths = ultrametric_depths(&nodes).unwrap();

        assert_eq!(depths["root"], 0.0);
        assert_eq!(depths["a"], 5.0);
        assert_eq!(depths["a1"], 6.0);
        assert_eq!(depths["b"], 6.0);
    }

    #[test]
    fn ultrametric_depths_reject_invalid_tree() {
        let nodes = vec![node("a", Some("missing"), None)];

        assert!(ultrametric_depths(&nodes).is_err());
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
