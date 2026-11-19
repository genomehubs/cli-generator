//! Rectangular cladogram geometry and Vega v5 marks for tree reports.
//!
//! Horizontal position is the cumulative branch length scaled across the plot
//! area, so default lengths give hop depth. Tips share the vertical range evenly
//! in depth-first order, and an internal node sits midway between its first and
//! last child. Coordinates are pixels, so the Vega spec needs no scales.

use std::collections::HashMap;

use serde_json::{json, Map, Value};

use super::tree_layout::{cumulative_depths, TreeLayoutNode};

/// Share of the plot width reserved on the right for tip labels.
const TIP_LABEL_FRACTION: f64 = 0.25;
/// Pixel margin between the plot edge and the outermost root, tips, and label band.
const PADDING_PX: f64 = 10.0;
/// Smallest tip label font size in pixels; keeps the spec valid when rows are very tight.
const MIN_TIP_FONT_PX: f64 = 1.0;
/// Largest tip label font size in pixels.
const MAX_TIP_FONT_PX: f64 = 12.0;
/// Fraction of a leaf row that a tip label may fill vertically.
const TIP_ROW_FILL: f64 = 0.8;

/// Pixel geometry for one node of a rectangular cladogram.
#[derive(Debug, Clone, PartialEq)]
pub struct RectNode {
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
) -> Result<Vec<RectNode>, String> {
    if nodes.is_empty() {
        return Ok(Vec::new());
    }
    let depths = cumulative_depths(nodes)?;
    let children = children_by_parent(nodes);
    let order = depth_first_order(find_root(nodes)?, &children);
    let y_by_id = vertical_positions(&order, &children, height);
    let geometry = Geometry {
        parent_of: nodes
            .iter()
            .filter_map(|node| Some((node.taxon_id.as_str(), node.parent_id.as_deref()?)))
            .collect(),
        x_by_id: horizontal_positions(nodes, &depths, width),
        y_by_id,
        children,
    };
    Ok(order.iter().map(|id| rect_node(id, &geometry)).collect())
}

/// Font size in pixels for tip labels, shrinking as leaf rows get tighter.
pub fn tip_font_size(height: f64, leaf_count: usize) -> f64 {
    if leaf_count <= 1 {
        return MAX_TIP_FONT_PX;
    }
    let row_px = (height - 2.0 * PADDING_PX) / (leaf_count - 1) as f64;
    (row_px * TIP_ROW_FILL).clamp(MIN_TIP_FONT_PX, MAX_TIP_FONT_PX)
}

/// Build layout nodes from a `treeNodes` object, taking each parent from its `children`.
///
/// A node's optional numeric `branch_length` is used as its edge length.
pub fn layout_nodes_from_tree_nodes(tree_nodes: &Map<String, Value>) -> Vec<TreeLayoutNode> {
    let mut parent_of: HashMap<&str, &str> = HashMap::new();
    for (parent_id, node) in tree_nodes {
        if let Some(kids) = node.get("children").and_then(Value::as_object) {
            for child_id in kids.keys() {
                parent_of.insert(child_id.as_str(), parent_id.as_str());
            }
        }
    }
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

/// Build a Vega v5 specification for a rectangular cladogram from a `treeNodes` object.
///
/// Elbow connectors are emitted as `links` and tip labels as `tips`, both in pixels.
/// Returns an error when the tree has no single root or a branch length is invalid.
pub fn rect_tree_vega(
    tree_nodes: &Map<String, Value>,
    width: u64,
    height: u64,
    title: Option<&str>,
) -> Result<Value, String> {
    let (plot_width, plot_height) = (width as f64, height as f64);
    let rect_nodes = layout_rectangular(
        &layout_nodes_from_tree_nodes(tree_nodes),
        plot_width,
        plot_height,
    )?;
    let leaf_count = rect_nodes.iter().filter(|node| node.is_leaf).count();
    let font_size = tip_font_size(plot_height, leaf_count);
    let links: Vec<Value> = rect_nodes
        .iter()
        .flat_map(|node| [node.arm_path.as_deref(), node.bar_path.as_deref()])
        .flatten()
        .map(|path| json!({ "d": path }))
        .collect();
    let tips: Vec<Value> = rect_nodes
        .iter()
        .filter(|node| node.is_leaf)
        .map(|node| {
            json!({
                "x": node.x,
                "y": node.y,
                "label": tip_label(tree_nodes, &node.taxon_id)
            })
        })
        .collect();
    let mut spec = json!({
        "$schema": "https://vega.github.io/schema/vega/v5.json",
        "width": width,
        "height": height,
        "data": [
            {"name": "links", "values": links},
            {"name": "tips", "values": tips}
        ],
        "marks": [
            {
                "type": "path",
                "from": {"data": "links"},
                "encode": {"enter": {
                    "path": {"field": "d"},
                    "stroke": {"value": "#555555"},
                    "strokeWidth": {"value": 1}
                }}
            },
            {
                "type": "text",
                "from": {"data": "tips"},
                "encode": {"enter": {
                    "x": {"field": "x"},
                    "y": {"field": "y"},
                    "text": {"field": "label"},
                    "dx": {"value": 4},
                    "align": {"value": "left"},
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
    Ok(spec)
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

/// Place tips evenly down the plot in depth-first order, then centre each internal node.
fn vertical_positions<'a>(
    order: &[&'a str],
    children: &HashMap<&'a str, Vec<&'a str>>,
    height: f64,
) -> HashMap<&'a str, f64> {
    let leaves: Vec<&'a str> = order
        .iter()
        .copied()
        .filter(|id| children[id].is_empty())
        .collect();
    let y_span = (height - 2.0 * PADDING_PX).max(0.0);
    let mut ys: HashMap<&'a str, f64> = HashMap::new();
    for (row, &leaf) in leaves.iter().enumerate() {
        let y = if leaves.len() > 1 {
            PADDING_PX + row as f64 / (leaves.len() - 1) as f64 * y_span
        } else {
            height / 2.0
        };
        ys.insert(leaf, y);
    }
    // Pre-order puts every child after its parent, so reverse order sees children first.
    for &id in order.iter().rev() {
        if let (Some(first), Some(last)) = (children[id].first(), children[id].last()) {
            let midpoint = (ys[first] + ys[last]) / 2.0;
            ys.insert(id, midpoint);
        }
    }
    ys
}

/// Build the pixel geometry for one node from the shared lookup tables.
fn rect_node(id: &str, geometry: &Geometry) -> RectNode {
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
    RectNode {
        taxon_id: id.to_string(),
        x,
        y,
        is_leaf: kids.is_empty(),
        arm_path,
        bar_path,
    }
}

/// Return the scientific name for a taxon, or its id when the name is absent.
fn tip_label(tree_nodes: &Map<String, Value>, taxon_id: &str) -> String {
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

    fn layout(nodes: &[TreeLayoutNode]) -> Vec<RectNode> {
        layout_rectangular(nodes, WIDTH as f64, HEIGHT as f64).unwrap()
    }

    fn by_id<'a>(rect: &'a [RectNode], taxon_id: &str) -> &'a RectNode {
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

        let err = layout_rectangular(&nodes, WIDTH as f64, HEIGHT as f64).unwrap_err();

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

        let spec = rect_tree_vega(map, WIDTH, HEIGHT, Some("Tree")).unwrap();

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
        let spec = rect_tree_vega(&Map::new(), WIDTH, HEIGHT, None).unwrap();

        assert_eq!(spec["data"][0]["values"].as_array().unwrap().len(), 0);
        assert_eq!(spec["data"][1]["values"].as_array().unwrap().len(), 0);
        assert!(spec.get("title").is_none());
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
