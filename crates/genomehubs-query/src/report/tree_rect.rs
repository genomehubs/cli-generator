//! Rectangular cladogram geometry and Vega v6 marks for tree reports.
//!
//! Horizontal position is the cumulative branch length scaled across the plot
//! area, so default lengths give hop depth. Tips share the vertical range evenly
//! in depth-first order, and an internal node sits midway between its first and
//! last child. Coordinates are pixels, so the Vega spec needs no scales.

use std::collections::HashMap;

use serde_json::{json, Map, Value};

use super::tree_category::{category_marks, link_rows, parent_by_child, BoxShape};
use super::tree_layout::{
    cumulative_depths, tip_aligned_depths, ultrametric_depths, TipAlignment, TreeLayoutNode,
    TreeRenderOptions,
};

/// Share of the plot width reserved on the right for tip labels.
const TIP_LABEL_FRACTION: f64 = 0.25;
/// Pixel margin between the plot edge and the outermost root, tips, and label band.
pub(super) const PADDING_PX: f64 = 10.0;
/// Pixel gap between a leaf and its tip label.
pub(super) const LABEL_OFFSET_PX: f64 = 4.0;
/// Smallest tip label font size in pixels; keeps the spec valid when rows are very tight.
const MIN_TIP_FONT_PX: f64 = 1.0;
/// Largest tip label font size in pixels.
const MAX_TIP_FONT_PX: f64 = 12.0;
/// Fraction of a leaf row that a tip label may fill vertically.
const TIP_ROW_FILL: f64 = 0.8;

/// Pixel geometry for one node of a rectangular cladogram.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutNode {
    /// Taxon identifier.
    pub taxon_id: String,
    /// Horizontal position in pixels.
    pub x: f64,
    /// Vertical position in pixels.
    pub y: f64,
    /// Whether the node has no children.
    pub is_leaf: bool,
    /// Horizontal arm from the parent's x to this node's x at this node's y; `None` for the root.
    pub arm_path: Option<String>,
    /// Vertical bar at this node's x spanning its first to last child; `None` for tips.
    pub bar_path: Option<String>,
}

/// Lookup tables shared by every node while building the layout.
struct Geometry<'a> {
    children: HashMap<&'a str, Vec<&'a str>>,
    parent_of: HashMap<&'a str, &'a str>,
    x_by_id: HashMap<&'a str, f64>,
    y_by_id: HashMap<&'a str, f64>,
}

/// Lay out a rectangular cladogram for a plot of `width` by `height` pixels.
///
/// Returns nodes in depth-first order with children sorted by `taxon_id`.
/// Empty input gives an empty layout. Returns an error when the tree has no
/// single root or a branch length is invalid.
pub fn layout_rectangular(
    nodes: &[TreeLayoutNode],
    width: f64,
    height: f64,
    alignment: TipAlignment,
) -> Result<Vec<LayoutNode>, String> {
    if nodes.is_empty() {
        return Ok(Vec::new());
    }
    let topo = topology(nodes, alignment)?;
    let y_span = (height - 2.0 * PADDING_PX).max(0.0);
    let y_by_id = spread_positions(&topo.order, &topo.children, |row, leaf_count| {
        if leaf_count > 1 {
            PADDING_PX + row as f64 / (leaf_count - 1) as f64 * y_span
        } else {
            height / 2.0
        }
    });
    let geometry = Geometry {
        x_by_id: horizontal_positions(nodes, &topo.depths, width),
        y_by_id,
        parent_of: topo.parent_of,
        children: topo.children,
    };
    Ok(topo
        .order
        .iter()
        .map(|id| rect_node(id, &geometry))
        .collect())
}

/// Font size in pixels for a tip label given the spacing between neighbouring leaves.
pub(super) fn tip_font_for_row(row_px: f64) -> f64 {
    (row_px * TIP_ROW_FILL).clamp(MIN_TIP_FONT_PX, MAX_TIP_FONT_PX)
}

/// Font size in pixels for tip labels, shrinking as leaf rows get tighter.
pub fn tip_font_size(height: f64, leaf_count: usize) -> f64 {
    if leaf_count <= 1 {
        return MAX_TIP_FONT_PX;
    }
    tip_font_for_row((height - 2.0 * PADDING_PX) / (leaf_count - 1) as f64)
}

/// Build layout nodes from a `treeNodes` object, taking each parent from its `children`.
///
/// A node's optional numeric `branch_length` is used as its edge length.
pub fn layout_nodes_from_tree_nodes(tree_nodes: &Map<String, Value>) -> Vec<TreeLayoutNode> {
    let parent_of = parent_by_child(tree_nodes);
    tree_nodes
        .iter()
        .map(|(taxon_id, node)| TreeLayoutNode {
            taxon_id: taxon_id.clone(),
            parent_id: parent_of
                .get(taxon_id.as_str())
                .map(|parent| (*parent).to_string()),
            branch_length: node.get("branch_length").and_then(Value::as_f64),
        })
        .collect()
}

/// Build a Vega v6 specification for a rectangular cladogram from a `treeNodes` object.
///
/// Elbow connectors are emitted as `links` and tip labels as `tips`, both in pixels.
/// Returns an error when the tree has no single root or a branch length is invalid.
pub fn rect_tree_vega(
    tree_nodes: &Map<String, Value>,
    width: u64,
    height: u64,
    title: Option<&str>,
    options: TreeRenderOptions,
) -> Result<Value, String> {
    let (plot_width, plot_height) = (width as f64, height as f64);
    let rect_nodes = layout_rectangular(
        &layout_nodes_from_tree_nodes(tree_nodes),
        plot_width,
        plot_height,
        options.alignment,
    )?;
    let leaf_count = rect_nodes.iter().filter(|node| node.is_leaf).count();
    let font_size = options
        .font_size
        .unwrap_or_else(|| tip_font_size(plot_height, leaf_count));
    let links = link_rows(&rect_nodes, tree_nodes, options.category_display);
    let (tips, leaders) = rect_tip_marks(tree_nodes, &rect_nodes, plot_width, options.alignment);
    let mut spec = tree_vega_spec(links, leaders, tips, width, height, title, font_size);
    category_marks(
        tree_nodes,
        &rect_nodes,
        options.category_display,
        BoxShape::Rect,
    )
    .insert_into(&mut spec);
    Ok(spec)
}

/// Tip label anchors and dotted leaders for the leaves of a rectangular layout.
///
/// With `TipAlignment::Labels` every label moves to the label column and a leader
/// runs from its tip to that column. Otherwise labels sit just past their tips.
fn rect_tip_marks(
    tree_nodes: &Map<String, Value>,
    rect_nodes: &[LayoutNode],
    plot_width: f64,
    alignment: TipAlignment,
) -> (Vec<Value>, Vec<Value>) {
    let label_x = label_column_x(plot_width);
    let mut leaders: Vec<Value> = Vec::new();
    let tips: Vec<Value> = rect_nodes
        .iter()
        .filter(|node| node.is_leaf)
        .map(|node| {
            let x = match alignment {
                TipAlignment::Labels => {
                    let leader = leader_path((node.x, node.y), (label_x, node.y));
                    leaders.push(json!({ "d": leader }));
                    label_x
                }
                TipAlignment::None | TipAlignment::Tips | TipAlignment::Ultrametric => {
                    node.x + LABEL_OFFSET_PX
                }
            };
            json!({
                "x": x,
                "y": node.y,
                "label": tip_label(tree_nodes, &node.taxon_id),
                "angle": 0,
                "align": "left"
            })
        })
        .collect();
    (tips, leaders)
}

/// Build a Vega v6 spec from link, leader, and tip label data in pixel coordinates.
///
/// Each tip carries its anchor, rotation in degrees, and alignment, so the text
/// mark only maps fields to channels.
pub(super) fn tree_vega_spec(
    links: Vec<Value>,
    leaders: Vec<Value>,
    tips: Vec<Value>,
    width: u64,
    height: u64,
    title: Option<&str>,
    font_size: f64,
) -> Value {
    let mut spec = json!({
        "$schema": "https://vega.github.io/schema/vega/v6.json",
        "width": width,
        "height": height,
        "data": [
            {"name": "links", "values": links},
            {"name": "tips", "values": tips},
            {"name": "leaders", "values": leaders}
        ],
        "marks": [
            {
                "type": "path",
                "from": {"data": "links"},
                "encode": {"enter": {
                    "path": {"field": "d"},
                    "stroke": {"field": "colour"},
                    "strokeWidth": {"value": 1}
                }}
            },
            {
                "type": "path",
                "from": {"data": "leaders"},
                "encode": {"enter": {
                    "path": {"field": "d"},
                    "stroke": {"value": "#999999"},
                    "strokeWidth": {"value": 1},
                    "strokeDash": {"value": [2, 3]}
                }}
            },
            {
                "type": "text",
                "from": {"data": "tips"},
                "encode": {"enter": {
                    "x": {"field": "x"},
                    "y": {"field": "y"},
                    "text": {"field": "label"},
                    "angle": {"field": "angle"},
                    "align": {"field": "align"},
                    "baseline": {"value": "middle"},
                    "fontSize": {"value": font_size},
                    "fontStyle": {"value": "italic"}
                }}
            }
        ]
    });
    if let Some(text) = title {
        spec["title"] = json!({"text": text});
    }
    spec
}

/// Parent, child, depth, and depth-first structure shared by every tree layout.
pub(super) struct Topology<'a> {
    /// Children of each taxon, sorted by `taxon_id`.
    pub(super) children: HashMap<&'a str, Vec<&'a str>>,
    /// Parent of each non-root taxon.
    pub(super) parent_of: HashMap<&'a str, &'a str>,
    /// Taxa in depth-first order from the root.
    pub(super) order: Vec<&'a str>,
    /// Cumulative branch length from the root to each taxon.
    pub(super) depths: HashMap<String, f64>,
}

/// Build the shared topology for non-empty `nodes`.
///
/// Returns an error when the tree has no single root or a branch length is invalid.
pub(super) fn topology(
    nodes: &[TreeLayoutNode],
    alignment: TipAlignment,
) -> Result<Topology<'_>, String> {
    let children = children_by_parent(nodes);
    let order = depth_first_order(find_root(nodes)?, &children);
    let parent_of = nodes
        .iter()
        .filter_map(|node| Some((node.taxon_id.as_str(), node.parent_id.as_deref()?)))
        .collect();
    let depths = match alignment {
        TipAlignment::Tips => tip_aligned_depths(nodes)?,
        TipAlignment::Ultrametric => ultrametric_depths(nodes)?,
        TipAlignment::None | TipAlignment::Labels => cumulative_depths(nodes)?,
    };
    Ok(Topology {
        children,
        parent_of,
        order,
        depths,
    })
}

/// Map each taxon to its children, sorted by `taxon_id`. Every node has an entry.
fn children_by_parent(nodes: &[TreeLayoutNode]) -> HashMap<&str, Vec<&str>> {
    let mut children: HashMap<&str, Vec<&str>> = nodes
        .iter()
        .map(|node| (node.taxon_id.as_str(), Vec::new()))
        .collect();
    for node in nodes {
        if let Some(parent_id) = node.parent_id.as_deref() {
            children
                .entry(parent_id)
                .or_default()
                .push(node.taxon_id.as_str());
        }
    }
    for kids in children.values_mut() {
        kids.sort_unstable();
    }
    children
}

/// Return the only node without a parent.
fn find_root(nodes: &[TreeLayoutNode]) -> Result<&str, String> {
    let mut roots = nodes.iter().filter(|node| node.parent_id.is_none());
    let root = roots.next().ok_or("tree has no root node")?;
    if roots.next().is_some() {
        return Err("tree has more than one root node".to_string());
    }
    Ok(root.taxon_id.as_str())
}

/// List node ids depth-first, visiting children in the order given.
fn depth_first_order<'a>(root: &'a str, children: &HashMap<&'a str, Vec<&'a str>>) -> Vec<&'a str> {
    let mut order = Vec::with_capacity(children.len());
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        order.push(id);
        stack.extend(children[id].iter().rev().copied());
    }
    order
}

/// Scale each node's cumulative depth across the plot area, starting at the left padding.
fn horizontal_positions<'a>(
    nodes: &'a [TreeLayoutNode],
    depths: &HashMap<String, f64>,
    width: f64,
) -> HashMap<&'a str, f64> {
    let max_depth = depths.values().copied().fold(0.0, f64::max);
    let span = (width * (1.0 - TIP_LABEL_FRACTION) - 2.0 * PADDING_PX).max(0.0);
    nodes
        .iter()
        .map(|node| {
            let fraction = if max_depth > 0.0 {
                depths[&node.taxon_id] / max_depth
            } else {
                0.0
            };
            (node.taxon_id.as_str(), PADDING_PX + fraction * span)
        })
        .collect()
}

/// Place leaves with `place_leaf(row, leaf_count)` in depth-first order, then centre each internal node.
pub(super) fn spread_positions<'a>(
    order: &[&'a str],
    children: &HashMap<&'a str, Vec<&'a str>>,
    place_leaf: impl Fn(usize, usize) -> f64,
) -> HashMap<&'a str, f64> {
    let leaves: Vec<&'a str> = order
        .iter()
        .copied()
        .filter(|id| children[id].is_empty())
        .collect();
    let mut positions: HashMap<&'a str, f64> = leaves
        .iter()
        .enumerate()
        .map(|(row, &leaf)| (leaf, place_leaf(row, leaves.len())))
        .collect();
    // Pre-order puts every child after its parent, so reverse order sees children first.
    for &id in order.iter().rev() {
        if let (Some(first), Some(last)) = (children[id].first(), children[id].last()) {
            let midpoint = (positions[first] + positions[last]) / 2.0;
            positions.insert(id, midpoint);
        }
    }
    positions
}

/// Build the pixel geometry for one node from the shared lookup tables.
fn rect_node(id: &str, geometry: &Geometry) -> LayoutNode {
    let x = geometry.x_by_id[id];
    let y = geometry.y_by_id[id];
    let kids = &geometry.children[id];
    let arm_path = geometry.parent_of.get(id).map(|parent| {
        let parent_x = geometry.x_by_id[parent];
        format!("M{parent_x:.2},{y:.2}H{x:.2}")
    });
    let bar_path = match (kids.first(), kids.last()) {
        (Some(first), Some(last)) => {
            let first_y = geometry.y_by_id[first];
            let last_y = geometry.y_by_id[last];
            Some(format!("M{x:.2},{first_y:.2}V{last_y:.2}"))
        }
        _ => None,
    };
    LayoutNode {
        taxon_id: id.to_string(),
        x,
        y,
        is_leaf: kids.is_empty(),
        arm_path,
        bar_path,
    }
}

/// Horizontal position of the tip label column for a plot `plot_width` pixels wide.
fn label_column_x(plot_width: f64) -> f64 {
    plot_width * (1.0 - TIP_LABEL_FRACTION) - PADDING_PX + LABEL_OFFSET_PX
}

/// SVG path for a straight line between two pixel points.
pub(super) fn leader_path(from: (f64, f64), to: (f64, f64)) -> String {
    format!("M{:.2},{:.2}L{:.2},{:.2}", from.0, from.1, to.0, to.1)
}

/// Return the scientific name for a taxon, or its id when the name is absent.
pub(super) fn tip_label(tree_nodes: &Map<String, Value>, taxon_id: &str) -> String {
    tree_nodes
        .get(taxon_id)
        .and_then(|node| node.get("scientific_name"))
        .and_then(Value::as_str)
        .unwrap_or(taxon_id)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const WIDTH: u64 = 800;
    const HEIGHT: u64 = 600;

    fn node(taxon_id: &str, parent_id: Option<&str>) -> TreeLayoutNode {
        TreeLayoutNode {
            taxon_id: taxon_id.to_string(),
            parent_id: parent_id.map(str::to_string),
            branch_length: None,
        }
    }

    fn layout(nodes: &[TreeLayoutNode]) -> Vec<LayoutNode> {
        layout_rectangular(nodes, WIDTH as f64, HEIGHT as f64, TipAlignment::None).unwrap()
    }

    fn by_id<'a>(rect: &'a [LayoutNode], taxon_id: &str) -> &'a LayoutNode {
        rect.iter().find(|n| n.taxon_id == taxon_id).unwrap()
    }

    #[test]
    fn empty_tree_gives_empty_layout() {
        assert!(layout(&[]).is_empty());
    }

    #[test]
    fn single_node_sits_at_left_edge_and_middle_height() {
        let rect = layout(&[node("root", None)]);

        assert_eq!(rect.len(), 1);
        assert!(rect[0].is_leaf);
        assert_eq!(rect[0].x, PADDING_PX);
        assert_eq!(rect[0].y, HEIGHT as f64 / 2.0);
        assert_eq!(rect[0].arm_path, None);
        assert_eq!(rect[0].bar_path, None);
    }

    #[test]
    fn chain_x_strictly_increases_and_all_nodes_share_the_tip_row() {
        let nodes = vec![
            node("t0", None),
            node("t1", Some("t0")),
            node("t2", Some("t1")),
            node("t3", Some("t2")),
        ];

        let rect = layout(&nodes);
        let xs: Vec<f64> = (0..4).map(|i| by_id(&rect, &format!("t{i}")).x).collect();
        let ys: Vec<f64> = (0..4).map(|i| by_id(&rect, &format!("t{i}")).y).collect();

        assert!(xs.windows(2).all(|pair| pair[0] < pair[1]), "xs: {xs:?}");
        assert!(ys.windows(2).all(|pair| pair[0] == pair[1]), "ys: {ys:?}");
    }

    #[test]
    fn internal_node_lies_within_its_children_range() {
        let nodes = vec![
            node("root", None),
            node("a", Some("root")),
            node("a1", Some("a")),
            node("a2", Some("a")),
            node("a3", Some("a")),
            node("b", Some("root")),
            node("b1", Some("b")),
        ];

        let rect = layout(&nodes);
        let a_y = by_id(&rect, "a").y;
        let child_ys = [
            by_id(&rect, "a1").y,
            by_id(&rect, "a2").y,
            by_id(&rect, "a3").y,
        ];
        let low = child_ys.iter().copied().fold(f64::INFINITY, f64::min);
        let high = child_ys.iter().copied().fold(f64::NEG_INFINITY, f64::max);

        assert!(
            a_y >= low && a_y <= high,
            "a_y {a_y} outside [{low}, {high}]"
        );
    }

    #[test]
    fn layout_is_independent_of_input_order() {
        let forward = vec![
            node("root", None),
            node("a", Some("root")),
            node("b", Some("root")),
            node("a1", Some("a")),
        ];
        let reversed: Vec<TreeLayoutNode> = forward.iter().rev().cloned().collect();

        assert_eq!(layout(&forward), layout(&reversed));
        assert_eq!(layout(&forward), layout(&forward));
    }

    #[test]
    fn branch_lengths_scale_horizontal_position() {
        let nodes = vec![
            TreeLayoutNode {
                taxon_id: "root".to_string(),
                parent_id: None,
                branch_length: None,
            },
            TreeLayoutNode {
                taxon_id: "short".to_string(),
                parent_id: Some("root".to_string()),
                branch_length: Some(1.0),
            },
            TreeLayoutNode {
                taxon_id: "long".to_string(),
                parent_id: Some("root".to_string()),
                branch_length: Some(3.0),
            },
        ];

        let rect = layout(&nodes);
        let short_offset = by_id(&rect, "short").x - PADDING_PX;
        let long_offset = by_id(&rect, "long").x - PADDING_PX;

        assert!((long_offset - 3.0 * short_offset).abs() < 1e-9);
    }

    #[test]
    fn multiple_roots_is_error() {
        let nodes = vec![node("a", None), node("b", None)];

        let err = layout_rectangular(&nodes, WIDTH as f64, HEIGHT as f64, TipAlignment::None)
            .unwrap_err();

        assert!(
            err.contains("more than one root"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn tip_font_size_shrinks_with_leaf_count_within_bounds() {
        assert_eq!(tip_font_size(HEIGHT as f64, 2), MAX_TIP_FONT_PX);
        assert_eq!(tip_font_size(HEIGHT as f64, 10_000), MIN_TIP_FONT_PX);
        assert!(tip_font_size(HEIGHT as f64, 50) > tip_font_size(HEIGHT as f64, 100));
    }

    #[test]
    fn rect_tree_vega_emits_arms_bars_and_one_tip_per_leaf() {
        let tree_nodes = json!({
            "root": {"scientific_name": "Root", "children": {"a": true, "b": true}},
            "a": {"scientific_name": "Alpha", "children": {"a1": true}},
            "a1": {"scientific_name": "Alpha one", "children": {}},
            "b": {"scientific_name": "Beta", "children": {}}
        });
        let map = tree_nodes.as_object().unwrap();

        let spec = rect_tree_vega(
            map,
            WIDTH,
            HEIGHT,
            Some("Tree"),
            TreeRenderOptions::with_alignment(TipAlignment::None),
        )
        .unwrap();

        // Three arms (a, a1, b) and two bars (root, a).
        let links = spec["data"][0]["values"].as_array().unwrap();
        let tips = spec["data"][1]["values"].as_array().unwrap();
        assert_eq!(links.len(), 5);
        assert_eq!(tips.len(), 2);
        assert_eq!(tips[0]["label"], "Alpha one");
        assert_eq!(tips[1]["label"], "Beta");
        assert_eq!(spec["width"], WIDTH);
        assert_eq!(spec["title"]["text"], "Tree");
    }

    #[test]
    fn rect_tree_vega_empty_tree_has_no_marks_data() {
        let spec = rect_tree_vega(
            &Map::new(),
            WIDTH,
            HEIGHT,
            None,
            TreeRenderOptions::with_alignment(TipAlignment::None),
        )
        .unwrap();

        assert_eq!(spec["data"][0]["values"].as_array().unwrap().len(), 0);
        assert_eq!(spec["data"][1]["values"].as_array().unwrap().len(), 0);
        assert!(spec.get("title").is_none());
    }

    #[test]
    fn tips_alignment_puts_every_leaf_in_one_column() {
        let nodes = vec![
            node("root", None),
            node("a", Some("root")),
            node("a1", Some("a")),
            node("b", Some("root")),
        ];

        let rect =
            layout_rectangular(&nodes, WIDTH as f64, HEIGHT as f64, TipAlignment::Tips).unwrap();

        let a1_x = by_id(&rect, "a1").x;
        assert_eq!(a1_x, by_id(&rect, "b").x);
        assert_eq!(a1_x, PADDING_PX + (WIDTH as f64 * 0.75 - 2.0 * PADDING_PX));
        assert!(by_id(&rect, "a").x > by_id(&rect, "root").x);
        assert!(by_id(&rect, "a").x < a1_x);
    }

    #[test]
    fn ultrametric_alignment_aligns_tips_and_keeps_internal_lengths() {
        let nodes = vec![
            TreeLayoutNode {
                taxon_id: "root".to_string(),
                parent_id: None,
                branch_length: None,
            },
            TreeLayoutNode {
                taxon_id: "a".to_string(),
                parent_id: Some("root".to_string()),
                branch_length: Some(5.0),
            },
            node("a1", Some("a")),
            node("b", Some("root")),
        ];

        let ultrametric = layout_rectangular(
            &nodes,
            WIDTH as f64,
            HEIGHT as f64,
            TipAlignment::Ultrametric,
        )
        .unwrap();
        let tips =
            layout_rectangular(&nodes, WIDTH as f64, HEIGHT as f64, TipAlignment::Tips).unwrap();

        assert_eq!(by_id(&ultrametric, "a1").x, by_id(&ultrametric, "b").x);
        assert!(by_id(&ultrametric, "a").x > by_id(&tips, "a").x);
    }

    #[test]
    fn labels_alignment_moves_tips_to_label_column_with_leaders() {
        let tree_nodes = json!({
            "root": {"scientific_name": "Root", "children": {"a": true, "b": true}},
            "a": {"scientific_name": "Alpha", "children": {}, "branch_length": 1.0},
            "b": {"scientific_name": "Beta", "children": {}, "branch_length": 3.0}
        });
        let map = tree_nodes.as_object().unwrap();

        let spec = rect_tree_vega(
            map,
            WIDTH,
            HEIGHT,
            None,
            TreeRenderOptions::with_alignment(TipAlignment::Labels),
        )
        .unwrap();

        let tips = spec["data"][1]["values"].as_array().unwrap();
        let leaders = spec["data"][2]["values"].as_array().unwrap();
        let label_x = label_column_x(WIDTH as f64);
        assert_eq!(tips.len(), 2);
        assert!(tips.iter().all(|tip| tip["x"].as_f64() == Some(label_x)));
        assert_eq!(leaders.len(), 2);
        assert!(leaders
            .iter()
            .all(|leader| leader["d"].as_str().unwrap().contains('L')));
    }

    #[test]
    fn leader_path_joins_two_points() {
        assert_eq!(leader_path((1.0, 2.0), (3.5, 4.0)), "M1.00,2.00L3.50,4.00");
    }

    proptest! {
        #[test]
        fn leaf_rows_are_unique_and_within_plot(leaf_count in 1usize..60) {
            let mut nodes = vec![node("root", None)];
            nodes.extend((0..leaf_count).map(|i| node(&format!("leaf{i:03}"), Some("root"))));

            let rect = layout(&nodes);
            let mut leaf_ys: Vec<f64> = rect.iter().filter(|n| n.is_leaf).map(|n| n.y).collect();
            leaf_ys.sort_by(f64::total_cmp);
            let span = HEIGHT as f64 - 2.0 * PADDING_PX;
            let unit: Vec<f64> = leaf_ys.iter().map(|y| (y - PADDING_PX) / span).collect();

            prop_assert!(unit.iter().all(|u| *u >= -1e-9 && *u <= 1.0 + 1e-9));
            prop_assert!(unit.windows(2).all(|pair| pair[0] < pair[1]));
        }
    }
}
