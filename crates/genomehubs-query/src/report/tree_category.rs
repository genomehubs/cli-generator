//! Category colouring for tree reports.
//!
//! A node's `cat` label is mapped to a colour from [`CATEGORY_PALETTE`] in the order
//! of `catBounds.labels`, so tree colours match histogram colours. Each renderer
//! reads the node's `catColour` and draws it as a ring fill, branch stroke, dot, or
//! box, as selected by `category_display`.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::f64::consts::{FRAC_PI_2, TAU};

use serde_json::{json, Map, Value};

use super::tree_layout::CategoryDisplay;
use super::tree_rect::LayoutNode;

/// Colours assigned to categories in order, matching Vega's `category10` scheme.
pub(super) const CATEGORY_PALETTE: [&str; 10] = [
    "#1f77b4", "#ff7f0e", "#2ca02c", "#d62728", "#9467bd", "#8c564b", "#e377c2", "#7f7f7f",
    "#bcbd22", "#17becf",
];
/// Node key holding the category colour once [`annotate_category_colours`] has run.
pub(super) const CAT_COLOUR_KEY: &str = "catColour";
/// Stroke colour for branches and tips without a category.
pub(super) const PLAIN_COLOUR: &str = "#555555";
/// Name of the Vega ordinal scale that maps category labels to colours.
const CATEGORY_SCALE: &str = "category";
/// Vega data set holding the dot rows.
const DOT_DATA: &str = "categoryDots";
/// Vega data set holding the box rows.
const BOX_DATA: &str = "categoryBoxes";
/// Diameter of a category dot in pixels.
const DOT_DIAMETER_PX: f64 = 6.0;
/// Gap in pixels between a clade's outermost nodes and its box edge.
const BOX_PADDING_PX: f64 = 6.0;
/// Fill opacity of a category box; the stroke carries the colour.
const BOX_FILL_OPACITY: f64 = 0.12;

/// Geometry of a clade box: a rectangle, or an annular sector around `centre` for polar layouts.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(super) enum BoxShape {
    #[default]
    Rect,
    Sector {
        centre: (f64, f64),
    },
}

/// Dot and box rows drawn over a tree.
#[derive(Debug, Default, PartialEq)]
pub(super) struct CategoryMarks {
    /// One row per categorised node with `x`, `y`, and `colour`.
    dots: Vec<Value>,
    /// One row per categorised clade, shaped by `shape`.
    boxes: Vec<Value>,
    /// Shape of each row in `boxes`.
    shape: BoxShape,
}

impl CategoryMarks {
    /// Add the box layer behind the tree and the dot layer below the tip labels.
    pub(super) fn insert_into(self, spec: &mut Value) {
        if !self.boxes.is_empty() {
            insert_layer(spec, BOX_DATA, self.boxes, box_mark(self.shape), 0);
        }
        if !self.dots.is_empty() {
            let label_index = spec
                .get("marks")
                .and_then(Value::as_array)
                .map_or(0, |marks| marks.len().saturating_sub(1));
            insert_layer(spec, DOT_DATA, self.dots, dot_mark(), label_index);
        }
    }
}

/// Return the palette colour for the category at `index`, cycling past the end.
fn palette_colour(index: usize) -> &'static str {
    CATEGORY_PALETTE[index % CATEGORY_PALETTE.len()]
}

/// Colour every categorised node and return the legend as `(label, colour)` pairs.
///
/// Labels keep the order of `catBounds.labels`; labels missing from it follow in
/// sorted order. Nodes without a `cat` get no `catColour`.
pub(super) fn annotate_category_colours(
    tree_nodes: &mut Map<String, Value>,
    cat_bounds: Option<&Value>,
) -> Vec<(String, String)> {
    let order = category_order(tree_nodes, cat_bounds);
    let colour_of: HashMap<&str, &str> = order
        .iter()
        .enumerate()
        .map(|(index, label)| (label.as_str(), palette_colour(index)))
        .collect();
    for node in tree_nodes.values_mut().filter_map(Value::as_object_mut) {
        let colour = node
            .get("cat")
            .and_then(Value::as_str)
            .and_then(|label| colour_of.get(label))
            .copied();
        if let Some(colour) = colour {
            node.insert(CAT_COLOUR_KEY.to_string(), json!(colour));
        }
    }
    order
        .iter()
        .enumerate()
        .map(|(index, label)| (label.clone(), palette_colour(index).to_string()))
        .collect()
}

/// Category labels in legend order: `catBounds.labels` first, then any other labels sorted.
fn category_order(tree_nodes: &Map<String, Value>, cat_bounds: Option<&Value>) -> Vec<String> {
    let mut order: Vec<String> = cat_bounds
        .and_then(|bounds| bounds.get("labels"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect();
    let listed: HashSet<String> = order.iter().cloned().collect();
    let unlisted: BTreeSet<&str> = tree_nodes
        .values()
        .filter_map(|node| node.get("cat").and_then(Value::as_str))
        .filter(|label| !listed.contains(*label))
        .collect();
    order.extend(unlisted.into_iter().map(str::to_string));
    order
}

/// Parent taxon ID for every child, read from each node's `children` object.
pub(super) fn parent_by_child(tree_nodes: &Map<String, Value>) -> HashMap<&str, &str> {
    let mut parent_of = HashMap::new();
    for parent_id in tree_nodes.keys() {
        for child_id in child_ids(tree_nodes, parent_id) {
            parent_of.insert(child_id, parent_id.as_str());
        }
    }
    parent_of
}

/// Taxon IDs listed under the `children` object of `taxon_id`.
fn child_ids<'a>(tree_nodes: &'a Map<String, Value>, taxon_id: &str) -> Vec<&'a str> {
    tree_nodes
        .get(taxon_id)
        .and_then(|node| node.get("children"))
        .and_then(Value::as_object)
        .map(|children| children.keys().map(String::as_str).collect())
        .unwrap_or_default()
}

/// The `cat` label of `taxon_id`, if it has one.
fn category_of<'a>(tree_nodes: &'a Map<String, Value>, taxon_id: &str) -> Option<&'a str> {
    tree_nodes.get(taxon_id)?.get("cat")?.as_str()
}

/// The `catColour` of `taxon_id`, if it has one.
pub(super) fn node_colour<'a>(
    tree_nodes: &'a Map<String, Value>,
    taxon_id: &str,
) -> Option<&'a str> {
    tree_nodes.get(taxon_id)?.get(CAT_COLOUR_KEY)?.as_str()
}

/// Link rows for a rectangular or polar layout, each carrying its stroke `colour`.
///
/// In `Branch` mode both paths of a node take that node's category colour. Every
/// other mode uses the plain colour.
pub(super) fn link_rows(
    nodes: &[LayoutNode],
    tree_nodes: &Map<String, Value>,
    display: CategoryDisplay,
) -> Vec<Value> {
    nodes
        .iter()
        .flat_map(|node| {
            let colour = branch_colour(tree_nodes, &node.taxon_id, display);
            [node.arm_path.as_deref(), node.bar_path.as_deref()]
                .into_iter()
                .flatten()
                .map(move |path| json!({ "d": path, "colour": colour }))
        })
        .collect()
}

/// Stroke colour for the branches of `taxon_id` under `display`.
fn branch_colour<'a>(
    tree_nodes: &'a Map<String, Value>,
    taxon_id: &str,
    display: CategoryDisplay,
) -> &'a str {
    match display {
        CategoryDisplay::Branch => node_colour(tree_nodes, taxon_id).unwrap_or(PLAIN_COLOUR),
        CategoryDisplay::Fill | CategoryDisplay::Dot | CategoryDisplay::Box => PLAIN_COLOUR,
    }
}

/// Dot and box rows for `display`; `Fill` and `Branch` add no extra marks.
pub(super) fn category_marks(
    tree_nodes: &Map<String, Value>,
    points: &[LayoutNode],
    display: CategoryDisplay,
    shape: BoxShape,
) -> CategoryMarks {
    match display {
        CategoryDisplay::Dot => CategoryMarks {
            dots: dot_rows(tree_nodes, points),
            ..CategoryMarks::default()
        },
        CategoryDisplay::Box => CategoryMarks {
            boxes: clade_box_rows(tree_nodes, points, shape),
            shape,
            ..CategoryMarks::default()
        },
        CategoryDisplay::Fill | CategoryDisplay::Branch => CategoryMarks::default(),
    }
}

/// One dot per categorised node, at the node's position.
fn dot_rows(tree_nodes: &Map<String, Value>, points: &[LayoutNode]) -> Vec<Value> {
    points
        .iter()
        .filter_map(|point| {
            let colour = node_colour(tree_nodes, &point.taxon_id)?;
            Some(json!({ "x": point.x, "y": point.y, "colour": colour }))
        })
        .collect()
}

/// One padded box per clade of two or more nodes sharing a category.
///
/// A clade starts at a categorised node whose parent has a different category, so
/// nested nodes of the same category are enclosed by their clade's box.
fn clade_box_rows(
    tree_nodes: &Map<String, Value>,
    points: &[LayoutNode],
    shape: BoxShape,
) -> Vec<Value> {
    let positions: HashMap<&str, (f64, f64)> = points
        .iter()
        .map(|point| (point.taxon_id.as_str(), (point.x, point.y)))
        .collect();
    let parents = parent_by_child(tree_nodes);
    let mut rows = Vec::new();
    for taxon_id in tree_nodes.keys() {
        let Some(label) = category_of(tree_nodes, taxon_id) else {
            continue;
        };
        let parent_label = parents
            .get(taxon_id.as_str())
            .and_then(|parent| category_of(tree_nodes, parent));
        if parent_label == Some(label) {
            continue;
        }
        let members = clade_members(tree_nodes, taxon_id, label);
        if members.len() < 2 {
            continue;
        }
        let member_points: Vec<(f64, f64)> = members
            .iter()
            .filter_map(|member| positions.get(member).copied())
            .collect();
        let Some(colour) = node_colour(tree_nodes, taxon_id) else {
            continue;
        };
        if let Some(row) = box_row(&member_points, shape, colour) {
            rows.push(row);
        }
    }
    rows
}

/// Taxon IDs of the nodes under `root` that carry `label` and connect to it, `root` included.
fn clade_members<'a>(
    tree_nodes: &'a Map<String, Value>,
    root: &'a str,
    label: &str,
) -> Vec<&'a str> {
    let mut members = Vec::new();
    let mut stack = vec![root];
    while let Some(taxon_id) = stack.pop() {
        members.push(taxon_id);
        stack.extend(
            child_ids(tree_nodes, taxon_id)
                .into_iter()
                .filter(|child| category_of(tree_nodes, child) == Some(label)),
        );
    }
    members
}

/// Bounding box `(x, y, width, height)` around `points`, grown by `padding` on every side.
///
/// Returns `None` when there are no points.
pub(super) fn bounding_box(points: &[(f64, f64)], padding: f64) -> Option<(f64, f64, f64, f64)> {
    let first = *points.first()?;
    let (min_x, min_y, max_x, max_y) = points.iter().fold(
        (first.0, first.1, first.0, first.1),
        |(low_x, low_y, high_x, high_y), &(x, y)| {
            (low_x.min(x), low_y.min(y), high_x.max(x), high_y.max(y))
        },
    );
    Some((
        min_x - padding,
        min_y - padding,
        max_x - min_x + 2.0 * padding,
        max_y - min_y + 2.0 * padding,
    ))
}

/// Row for one clade box: a padded rectangle, or an annular sector for `BoxShape::Sector`.
fn box_row(points: &[(f64, f64)], shape: BoxShape, colour: &str) -> Option<Value> {
    match shape {
        BoxShape::Rect => {
            let (x, y, width, height) = bounding_box(points, BOX_PADDING_PX)?;
            Some(json!({ "x": x, "y": y, "width": width, "height": height, "colour": colour }))
        }
        BoxShape::Sector { centre } => sector_row(points, centre, colour),
    }
}

/// Annular sector row enclosing `points` around `centre`, with Vega arc angles.
fn sector_row(points: &[(f64, f64)], centre: (f64, f64), colour: &str) -> Option<Value> {
    let radii: Vec<f64> = points
        .iter()
        .map(|&(x, y)| (x - centre.0).hypot(y - centre.1))
        .collect();
    let angles: Vec<f64> = points
        .iter()
        .map(|&(x, y)| (y - centre.1).atan2(x - centre.0))
        .collect();
    let inner_px = radii.iter().copied().fold(f64::INFINITY, f64::min);
    let outer_px = radii.iter().copied().fold(0.0, f64::max);
    let (start, end) = angular_span(&angles)?;
    let pad_angle = BOX_PADDING_PX / inner_px.max(BOX_PADDING_PX);
    let (start, end) = if end - start + 2.0 * pad_angle >= TAU {
        (0.0, TAU)
    } else {
        (start - pad_angle, end + pad_angle)
    };
    Some(json!({
        "x": centre.0,
        "y": centre.1,
        "innerRadius": (inner_px - BOX_PADDING_PX).max(0.0),
        "outerRadius": outer_px + BOX_PADDING_PX,
        "startAngle": start + FRAC_PI_2,
        "endAngle": end + FRAC_PI_2,
        "colour": colour
    }))
}

/// Smallest arc `(start, end)` containing `angles`, found by skipping the widest gap.
///
/// `end` may exceed `TAU` when the arc crosses the `-PI`/`PI` seam.
fn angular_span(angles: &[f64]) -> Option<(f64, f64)> {
    let mut sorted = angles.to_vec();
    sorted.sort_by(f64::total_cmp);
    let first = *sorted.first()?;
    let (_, gap_index) = (0..sorted.len())
        .map(|index| {
            let next = sorted.get(index + 1).copied().unwrap_or(first + TAU);
            (next - sorted[index], index)
        })
        .max_by(|a, b| a.0.total_cmp(&b.0))?;
    let start = sorted.get(gap_index + 1).copied().unwrap_or(first);
    let end = if gap_index + 1 < sorted.len() {
        sorted[gap_index] + TAU
    } else {
        sorted[gap_index]
    };
    Some((start, end))
}

/// Vega mark drawn behind the tree for each category box, matching its `shape`.
fn box_mark(shape: BoxShape) -> Value {
    match shape {
        BoxShape::Rect => json!({
            "type": "rect",
            "from": {"data": BOX_DATA},
            "encode": {"enter": {
                "x": {"field": "x"},
                "y": {"field": "y"},
                "width": {"field": "width"},
                "height": {"field": "height"},
                "fill": {"field": "colour"},
                "fillOpacity": {"value": BOX_FILL_OPACITY},
                "stroke": {"field": "colour"},
                "strokeWidth": {"value": 1},
                "cornerRadius": {"value": 4}
            }}
        }),
        BoxShape::Sector { .. } => json!({
            "type": "arc",
            "from": {"data": BOX_DATA},
            "encode": {"enter": {
                "x": {"field": "x"},
                "y": {"field": "y"},
                "innerRadius": {"field": "innerRadius"},
                "outerRadius": {"field": "outerRadius"},
                "startAngle": {"field": "startAngle"},
                "endAngle": {"field": "endAngle"},
                "fill": {"field": "colour"},
                "fillOpacity": {"value": BOX_FILL_OPACITY},
                "stroke": {"field": "colour"},
                "strokeWidth": {"value": 1}
            }}
        }),
    }
}

/// Vega circle symbol mark drawn on each categorised node.
fn dot_mark() -> Value {
    json!({
        "type": "symbol",
        "from": {"data": DOT_DATA},
        "encode": {"enter": {
            "x": {"field": "x"},
            "y": {"field": "y"},
            "shape": {"value": "circle"},
            "size": {"value": DOT_DIAMETER_PX * DOT_DIAMETER_PX},
            "fill": {"field": "colour"}
        }}
    })
}

/// Add a data set named `data_name` and insert `mark` at `mark_index` in the spec.
fn insert_layer(
    spec: &mut Value,
    data_name: &str,
    values: Vec<Value>,
    mark: Value,
    mark_index: usize,
) {
    if let Some(data) = spec.get_mut("data").and_then(Value::as_array_mut) {
        data.push(json!({"name": data_name, "values": values}));
    }
    if let Some(marks) = spec.get_mut("marks").and_then(Value::as_array_mut) {
        marks.insert(mark_index.min(marks.len()), mark);
    }
}

/// Add an ordinal colour scale and a bottom legend for `entries`, titled `title`.
///
/// Does nothing when there are no entries.
pub(super) fn add_category_legend(spec: &mut Value, entries: &[(String, String)], title: &str) {
    if entries.is_empty() {
        return;
    }
    let labels: Vec<&str> = entries.iter().map(|(label, _)| label.as_str()).collect();
    let colours: Vec<&str> = entries.iter().map(|(_, colour)| colour.as_str()).collect();
    spec["scales"] = json!([{
        "name": CATEGORY_SCALE,
        "type": "ordinal",
        "domain": labels,
        "range": colours
    }]);
    spec["legends"] = json!([{"fill": CATEGORY_SCALE, "title": title, "orient": "bottom"}]);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(value: Value) -> Map<String, Value> {
        value.as_object().cloned().unwrap()
    }

    fn layout_node(taxon_id: &str, x: f64, y: f64, arm_path: Option<&str>) -> LayoutNode {
        LayoutNode {
            taxon_id: taxon_id.to_string(),
            x,
            y,
            is_leaf: true,
            arm_path: arm_path.map(str::to_string),
            bar_path: None,
        }
    }

    fn assert_angle(value: &Value, expected: f64) {
        let actual = value.as_f64().unwrap();
        assert!((actual - expected).abs() < 1e-9, "{actual} != {expected}");
    }

    #[test]
    fn palette_cycles_after_last_colour() {
        assert_eq!(palette_colour(10), palette_colour(0));
        assert_ne!(palette_colour(1), palette_colour(0));
    }

    #[test]
    fn annotate_colours_follow_catbounds_order() {
        let mut nodes = tree(json!({
            "a": {"cat": "y"},
            "b": {"cat": "x"},
            "c": {}
        }));
        let bounds = json!({"labels": ["x", "y"]});

        let legend = annotate_category_colours(&mut nodes, Some(&bounds));

        assert_eq!(nodes["a"]["catColour"], json!(palette_colour(1)));
        assert_eq!(nodes["b"]["catColour"], json!(palette_colour(0)));
        assert!(nodes["c"].get(CAT_COLOUR_KEY).is_none());
        assert_eq!(
            legend,
            vec![
                ("x".to_string(), palette_colour(0).to_string()),
                ("y".to_string(), palette_colour(1).to_string())
            ]
        );
    }

    #[test]
    fn annotate_appends_unlisted_labels_in_sorted_order() {
        let mut nodes = tree(json!({"a": {"cat": "z"}, "b": {"cat": "m"}}));

        let legend = annotate_category_colours(&mut nodes, None);

        assert_eq!(legend[0].0, "m");
        assert_eq!(legend[1].0, "z");
        assert_eq!(nodes["b"]["catColour"], json!(palette_colour(0)));
    }

    #[test]
    fn bounding_box_is_padded_extent_of_points() {
        let bounds = bounding_box(&[(1.0, 2.0), (5.0, 4.0)], 1.0);

        assert_eq!(bounds, Some((0.0, 1.0, 6.0, 4.0)));
    }

    #[test]
    fn bounding_box_of_no_points_is_none() {
        assert_eq!(bounding_box(&[], 1.0), None);
    }

    #[test]
    fn branch_mode_colours_links_of_categorised_nodes_only() {
        let nodes = tree(json!({"a": {"catColour": "#ff0000"}, "b": {}}));
        let layout = vec![
            layout_node("a", 0.0, 0.0, Some("M0,0L1,1")),
            layout_node("b", 0.0, 0.0, Some("M0,0L2,2")),
        ];

        let branch = link_rows(&layout, &nodes, CategoryDisplay::Branch);
        let dot = link_rows(&layout, &nodes, CategoryDisplay::Dot);

        assert_eq!(branch[0]["colour"], json!("#ff0000"));
        assert_eq!(branch[1]["colour"], json!(PLAIN_COLOUR));
        assert_eq!(dot[0]["colour"], json!(PLAIN_COLOUR));
    }

    #[test]
    fn dot_mode_adds_one_dot_per_categorised_node_below_labels() {
        let nodes = tree(json!({"a": {"catColour": "#ff0000"}, "b": {}}));
        let points = vec![
            layout_node("a", 1.0, 2.0, None),
            layout_node("b", 3.0, 4.0, None),
        ];
        let mut spec = json!({
            "data": [{"name": "links", "values": []}],
            "marks": [{"type": "path"}, {"type": "text"}]
        });

        category_marks(&nodes, &points, CategoryDisplay::Dot, BoxShape::Rect)
            .insert_into(&mut spec);

        assert_eq!(spec["data"][1]["values"].as_array().unwrap().len(), 1);
        assert_eq!(spec["data"][1]["values"][0]["x"], json!(1.0));
        assert_eq!(spec["marks"][1]["type"], json!("symbol"));
        assert_eq!(spec["marks"][2]["type"], json!("text"));
    }

    #[test]
    fn box_mode_boxes_each_clade_once_and_skips_lone_nodes() {
        let nodes = tree(json!({
            "A": {"cat": "x", "catColour": "#1f77b4", "children": {"a1": {}, "a2": {}}},
            "B": {"cat": "y", "catColour": "#ff7f0e", "children": {"b1": {}}},
            "a1": {"cat": "x", "catColour": "#1f77b4"},
            "a2": {"cat": "x", "catColour": "#1f77b4"},
            "b1": {"cat": "y", "catColour": "#ff7f0e"},
            "c": {"cat": "z", "catColour": "#2ca02c"},
            "root": {"children": {"A": {}, "B": {}, "c": {}}}
        }));
        let points = vec![
            layout_node("root", 0.0, 0.0, None),
            layout_node("A", 5.0, 0.0, None),
            layout_node("a1", 10.0, -5.0, None),
            layout_node("a2", 10.0, 5.0, None),
            layout_node("B", 5.0, 20.0, None),
            layout_node("b1", 10.0, 20.0, None),
            layout_node("c", 5.0, 10.0, None),
        ];

        let marks = category_marks(&nodes, &points, CategoryDisplay::Box, BoxShape::Rect);

        assert_eq!(marks.boxes.len(), 2);
        assert_eq!(marks.boxes[0]["x"], json!(-1.0));
        assert_eq!(marks.boxes[0]["y"], json!(-11.0));
        assert_eq!(marks.boxes[0]["width"], json!(17.0));
        assert_eq!(marks.boxes[0]["height"], json!(22.0));
    }

    #[test]
    fn box_mark_goes_behind_the_tree() {
        let mut spec = json!({
            "data": [{"name": "links", "values": []}],
            "marks": [{"type": "path"}, {"type": "text"}]
        });
        let marks = CategoryMarks {
            dots: Vec::new(),
            boxes: vec![
                json!({"x": 0.0, "y": 0.0, "width": 1.0, "height": 1.0, "colour": "#000000"}),
            ],
            shape: BoxShape::Rect,
        };

        marks.insert_into(&mut spec);

        assert_eq!(spec["marks"][0]["type"], json!("rect"));
        assert_eq!(spec["marks"][0]["from"]["data"], json!(BOX_DATA));
    }

    #[test]
    fn sector_box_wraps_clade_in_annular_sector() {
        let nodes = tree(json!({
            "A": {"cat": "x", "catColour": "#1f77b4", "children": {"a1": {}}},
            "a1": {"cat": "x", "catColour": "#1f77b4"},
            "root": {"children": {"A": {}}}
        }));
        let points = vec![
            layout_node("A", 10.0, 0.0, None),
            layout_node("a1", 0.0, 10.0, None),
        ];

        let marks = category_marks(
            &nodes,
            &points,
            CategoryDisplay::Box,
            BoxShape::Sector { centre: (0.0, 0.0) },
        );
        let row = &marks.boxes[0];

        assert_eq!(marks.boxes.len(), 1);
        assert_eq!(row["innerRadius"], json!(4.0));
        assert_eq!(row["outerRadius"], json!(16.0));
        assert_angle(&row["startAngle"], -0.6 + FRAC_PI_2);
        assert_angle(&row["endAngle"], FRAC_PI_2 + 0.6 + FRAC_PI_2);
    }

    #[test]
    fn angular_span_takes_the_short_way_across_the_seam() {
        let (start, end) = angular_span(&[3.0, -3.0]).unwrap();

        assert_eq!(start, 3.0);
        assert!((end - start - (TAU - 6.0)).abs() < 1e-9);
    }

    #[test]
    fn sector_box_mark_is_an_arc_behind_the_tree() {
        let mut spec = json!({
            "data": [{"name": "links", "values": []}],
            "marks": [{"type": "path"}]
        });
        let marks = CategoryMarks {
            dots: Vec::new(),
            boxes: vec![
                json!({"x": 0.0, "y": 0.0, "innerRadius": 1.0, "outerRadius": 2.0, "startAngle": 0.0, "endAngle": 1.0, "colour": "#000000"}),
            ],
            shape: BoxShape::Sector { centre: (0.0, 0.0) },
        };

        marks.insert_into(&mut spec);

        assert_eq!(spec["marks"][0]["type"], json!("arc"));
    }

    #[test]
    fn fill_and_branch_modes_add_no_marks() {
        let nodes = tree(json!({"a": {"catColour": "#ff0000"}}));
        let points = vec![layout_node("a", 0.0, 0.0, None)];

        assert_eq!(
            category_marks(&nodes, &points, CategoryDisplay::Fill, BoxShape::Rect),
            CategoryMarks::default()
        );
        assert_eq!(
            category_marks(&nodes, &points, CategoryDisplay::Branch, BoxShape::Rect),
            CategoryMarks::default()
        );
    }

    #[test]
    fn legend_adds_scale_and_bottom_legend() {
        let mut spec = json!({"marks": []});
        let entries = vec![
            ("x".to_string(), "#1f77b4".to_string()),
            ("y".to_string(), "#ff7f0e".to_string()),
        ];

        add_category_legend(&mut spec, &entries, "genus");

        assert_eq!(spec["scales"][0]["domain"], json!(["x", "y"]));
        assert_eq!(spec["scales"][0]["range"], json!(["#1f77b4", "#ff7f0e"]));
        assert_eq!(spec["legends"][0]["title"], json!("genus"));
        assert_eq!(spec["legends"][0]["orient"], json!("bottom"));
    }

    #[test]
    fn legend_is_skipped_without_entries() {
        let mut spec = json!({"marks": []});

        add_category_legend(&mut spec, &[], "genus");

        assert!(spec.get("legends").is_none());
    }
}
