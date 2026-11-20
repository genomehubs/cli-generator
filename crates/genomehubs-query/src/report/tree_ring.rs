//! Ring (sunburst-style) tree layout for tree reports.
//!
//! Each taxon is an annular sector. Its ring follows its hop count from the root,
//! so branch lengths are ignored and every ring has the same thickness. Leaves
//! share the full sweep evenly and reach the outer edge; an internal node spans
//! all of its leaves. A sector taller than it is wide gets a radial label, and a
//! sector wider than it is tall gets its label curved along the arc. Labels that
//! cannot fit are omitted.

use std::collections::HashMap;
use std::f64::consts::{FRAC_PI_2, PI};

use serde_json::{json, Map, Value};

use super::tree_layout::{LabelOrientation, TipAlignment, TreeLayoutNode, TreeRenderOptions};
use super::tree_polar::{validate_angles, CENTRE_HOLE_PX};
use super::tree_rect::{layout_nodes_from_tree_nodes, tip_label, topology, PADDING_PX};

/// Fill colour of every annular sector.
const RING_FILL: &str = "#9b9ca1";
/// Colour of the sector separators and the labels.
const RING_LIGHT: &str = "#ffffff";
/// Label font size in pixels when no `font_size` is set; shared by every sector.
const DEFAULT_RING_FONT_PX: f64 = 10.0;
/// Stroke width of the sector borders in pixels.
const RING_BORDER_PX: f64 = 0.5;
/// Share of the arc length that a label's font height may fill.
const RING_FONT_ARC_FILL: f64 = 0.8;
/// Average glyph width as a multiple of the font size.
const GLYPH_WIDTH_EM: f64 = 0.55;
/// Fewest characters worth showing; shorter labels are omitted.
const MIN_LABEL_CHARS: usize = 3;
/// Pixel gap kept between a label and the sector edges along the radius.
const RING_LABEL_PAD_PX: f64 = 2.0;
/// Angle in radians kept free so a full-circle sector still has distinct end points.
const FULL_TURN_GAP_RAD: f64 = 1e-4;

/// A taxon drawn as an annular sector, with angles in radians.
#[derive(Debug, Clone, PartialEq)]
pub struct RingArc {
    /// Taxon identifier.
    pub taxon_id: String,
    /// Radius of the inner edge in pixels.
    pub inner_radius: f64,
    /// Radius of the outer edge in pixels.
    pub outer_radius: f64,
    /// Angle of the first edge in radians, clockwise from the positive x axis.
    pub start_angle: f64,
    /// Angle of the last edge in radians, clockwise from the positive x axis.
    pub end_angle: f64,
}

/// Build a Vega v6 spec for a ring tree whose leaves span `total_angle` degrees.
///
/// Angles are in degrees, measured clockwise from the positive x axis, and the
/// leaves start at `start_angle`. The root is not drawn. `options.alignment` does
/// not apply: every leaf already reaches the outer edge. Labels use
/// `options.font_size` when set, otherwise the default ring size. Returns an error
/// when an angle is invalid or the tree is invalid.
pub fn ring_tree_vega(
    tree_nodes: &Map<String, Value>,
    width: u64,
    height: u64,
    title: Option<&str>,
    total_angle: f64,
    start_angle: f64,
    options: TreeRenderOptions,
) -> Result<Value, String> {
    let (plot_width, plot_height) = (width as f64, height as f64);
    let arcs = layout_ring(
        &layout_nodes_from_tree_nodes(tree_nodes),
        plot_width,
        plot_height,
        total_angle,
        start_angle,
    )?;
    let centre = (plot_width / 2.0, plot_height / 2.0);
    let paths: Vec<Value> = arcs
        .iter()
        .map(|arc| json!({ "d": arc_path(arc, centre) }))
        .collect();
    let font_px = options.font_size.unwrap_or(DEFAULT_RING_FONT_PX);
    let labels = ring_labels(
        tree_nodes,
        &arcs,
        centre,
        font_px,
        options.label_orientation,
    );
    Ok(ring_vega_spec(paths, labels, width, height, title))
}

/// Lay out one annular sector per non-root taxon, in depth-first order.
///
/// Leaves share `total_angle` degrees starting at `start_angle`. Returns an error
/// when an angle is invalid or the tree is invalid.
fn layout_ring(
    nodes: &[TreeLayoutNode],
    width: f64,
    height: f64,
    total_angle: f64,
    start_angle: f64,
) -> Result<Vec<RingArc>, String> {
    validate_angles(total_angle, start_angle)?;
    if nodes.is_empty() {
        return Ok(Vec::new());
    }
    let topo = topology(nodes, TipAlignment::None)?;
    let levels = hop_levels(&topo.order, &topo.parent_of);
    let spans = leaf_spans(&topo.order, &topo.children);
    let max_level = levels.values().copied().max().unwrap_or(0).max(1);
    let outer = ring_outer_radius(width, height);
    let hole = CENTRE_HOLE_PX.min(outer);
    let band = (outer - hole) / max_level as f64;
    let leaf_total = spans[topo.order[0]].1 as f64;
    let slot = total_angle.to_radians() / leaf_total;
    let start = start_angle.to_radians();
    Ok(topo
        .order
        .iter()
        .filter(|id| topo.parent_of.contains_key(*id))
        .map(|id| {
            let level = levels[id];
            let (first, end) = spans[id];
            let is_leaf = topo.children[id].is_empty();
            RingArc {
                taxon_id: id.to_string(),
                inner_radius: hole + (level - 1) as f64 * band,
                outer_radius: if is_leaf {
                    outer
                } else {
                    hole + level as f64 * band
                },
                start_angle: start + first as f64 * slot,
                end_angle: start + end as f64 * slot,
            }
        })
        .collect())
}

/// Hop count from the root to each taxon, computed parents first.
fn hop_levels<'a>(
    order: &[&'a str],
    parent_of: &HashMap<&'a str, &'a str>,
) -> HashMap<&'a str, usize> {
    let mut levels = HashMap::with_capacity(order.len());
    for &id in order {
        let level = parent_of.get(id).map_or(0, |parent| levels[*parent] + 1);
        levels.insert(id, level);
    }
    levels
}

/// Half-open range of leaf slots `[first, end)` covered by each taxon.
///
/// Leaves are numbered in depth-first order, so each taxon's leaves are contiguous.
fn leaf_spans<'a>(
    order: &[&'a str],
    children: &HashMap<&'a str, Vec<&'a str>>,
) -> HashMap<&'a str, (usize, usize)> {
    let mut spans = HashMap::with_capacity(order.len());
    let mut leaf_count = 0;
    for &id in order {
        if children[id].is_empty() {
            spans.insert(id, (leaf_count, leaf_count + 1));
            leaf_count += 1;
        }
    }
    // Pre-order puts every child after its parent, so reverse order sees children first.
    for &id in order.iter().rev() {
        if let (Some(first), Some(last)) = (children[id].first(), children[id].last()) {
            spans.insert(id, (spans[*first].0, spans[*last].1));
        }
    }
    spans
}

/// Radius of the outermost ring, leaving the plot padding free.
fn ring_outer_radius(width: f64, height: f64) -> f64 {
    (width.min(height) / 2.0 - PADDING_PX).max(0.0)
}

/// SVG path of one annular sector around `centre`, drawn clockwise from its start angle.
fn arc_path(arc: &RingArc, centre: (f64, f64)) -> String {
    let end = arc
        .end_angle
        .min(arc.start_angle + 2.0 * PI - FULL_TURN_GAP_RAD);
    let large = u8::from(end - arc.start_angle > PI);
    let outer = arc.outer_radius;
    let inner = arc.inner_radius;
    let point = |radius: f64, angle: f64| {
        (
            centre.0 + radius * angle.cos(),
            centre.1 + radius * angle.sin(),
        )
    };
    let (outer_start_x, outer_start_y) = point(outer, arc.start_angle);
    let (outer_end_x, outer_end_y) = point(outer, end);
    let (inner_end_x, inner_end_y) = point(inner, end);
    let (inner_start_x, inner_start_y) = point(inner, arc.start_angle);
    format!(
        "M{outer_start_x:.2},{outer_start_y:.2}A{outer:.2},{outer:.2} 0 {large} 1 {outer_end_x:.2},{outer_end_y:.2}L{inner_end_x:.2},{inner_end_y:.2}A{inner:.2},{inner:.2} 0 {large} 0 {inner_start_x:.2},{inner_start_y:.2}Z"
    )
}

/// Label rows for every sector that can show its taxon name.
fn ring_labels(
    tree_nodes: &Map<String, Value>,
    arcs: &[RingArc],
    centre: (f64, f64),
    font_px: f64,
    orientation: LabelOrientation,
) -> Vec<Value> {
    arcs.iter()
        .flat_map(|arc| {
            let label = tip_label(tree_nodes, &arc.taxon_id);
            sector_label_rows(&label, arc, centre, font_px, orientation)
        })
        .collect()
}

/// Mid radius, arc length at mid radius, and ring thickness of a sector, in pixels.
fn sector_size(arc: &RingArc) -> (f64, f64, f64) {
    let mid_radius = (arc.inner_radius + arc.outer_radius) / 2.0;
    let arc_px = mid_radius * (arc.end_angle - arc.start_angle);
    (mid_radius, arc_px, arc.outer_radius - arc.inner_radius)
}

/// Label rows for one sector, curved when the arc is wider than the ring is thick.
fn sector_label_rows(
    label: &str,
    arc: &RingArc,
    centre: (f64, f64),
    font_px: f64,
    orientation: LabelOrientation,
) -> Vec<Value> {
    let (_, arc_px, radial_px) = sector_size(arc);
    if arc_px > radial_px {
        curved_label_rows(label, arc, centre, font_px, orientation)
    } else {
        radial_label_rows(label, arc, centre, font_px, orientation)
    }
}

/// Gap from the outer edge to a radial label's anchor; right-aligned text clears it by one glyph.
fn radial_outer_gap(align: &str, font_px: f64) -> f64 {
    if align == "right" {
        RING_LABEL_PAD_PX + GLYPH_WIDTH_EM * font_px
    } else {
        RING_LABEL_PAD_PX
    }
}

/// One radial label right aligned to the outer edge, or no rows when it cannot fit.
///
/// The text runs along the radius. `Flip` turns the left half around and aligns it
/// left, so the text stays inside the sector.
fn radial_label_rows(
    label: &str,
    arc: &RingArc,
    centre: (f64, f64),
    font_px: f64,
    orientation: LabelOrientation,
) -> Vec<Value> {
    let (_, arc_px, radial_px) = sector_size(arc);
    if font_px > arc_px * RING_FONT_ARC_FILL {
        return Vec::new();
    }
    let mid_angle = (arc.start_angle + arc.end_angle) / 2.0;
    let (rotation, align) = radial_orientation(mid_angle, orientation);
    let outer_gap = radial_outer_gap(align, font_px);
    let Some(text) = truncate_label(label, radial_px - outer_gap - RING_LABEL_PAD_PX, font_px)
    else {
        return Vec::new();
    };
    let radius = arc.outer_radius - outer_gap;
    vec![json!({
        "x": centre.0 + radius * mid_angle.cos(),
        "y": centre.1 + radius * mid_angle.sin(),
        "label": text,
        "angle": rotation,
        "align": align,
        "fontSize": font_px
    })]
}

/// One row per glyph set along the arc at mid radius, or no rows when it cannot fit.
///
/// Vega cannot place text on a path, so each glyph is centred on its own point and
/// turned to the tangent of the arc.
fn curved_label_rows(
    label: &str,
    arc: &RingArc,
    centre: (f64, f64),
    font_px: f64,
    orientation: LabelOrientation,
) -> Vec<Value> {
    let (mid_radius, arc_px, radial_px) = sector_size(arc);
    if font_px > radial_px - 2.0 * RING_LABEL_PAD_PX {
        return Vec::new();
    }
    let Some(text) = truncate_label(label, arc_px - 2.0 * RING_LABEL_PAD_PX, font_px) else {
        return Vec::new();
    };
    let mid_angle = (arc.start_angle + arc.end_angle) / 2.0;
    let direction = reading_direction(mid_angle, orientation);
    let glyph_px = GLYPH_WIDTH_EM * font_px;
    let glyph_count = text.chars().count() as f64;
    text.chars()
        .enumerate()
        .map(|(index, glyph)| {
            let offset = (index as f64 + 0.5 - glyph_count / 2.0) * glyph_px;
            let angle = mid_angle + direction * offset / mid_radius;
            json!({
                "x": centre.0 + mid_radius * angle.cos(),
                "y": centre.1 + mid_radius * angle.sin(),
                "label": glyph.to_string(),
                "angle": (angle + direction * FRAC_PI_2).to_degrees(),
                "align": "center",
                "fontSize": font_px
            })
        })
        .collect()
}

/// Rotation in degrees and alignment of a radial label at `mid_angle` radians.
///
/// `Continuous` reads outward on every half. `Flip` turns the left half around
/// and aligns it to the inner side.
fn radial_orientation(mid_angle: f64, orientation: LabelOrientation) -> (f64, &'static str) {
    let degrees = mid_angle.to_degrees();
    match orientation {
        LabelOrientation::Continuous => (degrees.rem_euclid(360.0), "right"),
        LabelOrientation::Flip if mid_angle.cos() < 0.0 => {
            ((degrees + 180.0).rem_euclid(360.0), "left")
        }
        LabelOrientation::Flip => (degrees.rem_euclid(360.0), "right"),
    }
}

/// Direction curved text reads along the arc: `1.0` clockwise, `-1.0` anticlockwise.
///
/// `Continuous` always reads clockwise. `Flip` reads whichever way keeps the text
/// upright along the bottom half.
fn reading_direction(mid_angle: f64, orientation: LabelOrientation) -> f64 {
    match orientation {
        LabelOrientation::Continuous => 1.0,
        LabelOrientation::Flip if mid_angle.sin() <= 0.0 => 1.0,
        LabelOrientation::Flip => -1.0,
    }
}

/// Label cut to fit `room_px` at `font_px`, ending in an ellipsis when shortened.
///
/// Returns `None` when fewer than `MIN_LABEL_CHARS` characters fit.
fn truncate_label(label: &str, room_px: f64, font_px: f64) -> Option<String> {
    let fits = (room_px / (GLYPH_WIDTH_EM * font_px)).floor().max(0.0) as usize;
    if fits < MIN_LABEL_CHARS {
        return None;
    }
    if label.chars().count() <= fits {
        return Some(label.to_string());
    }
    let kept: String = label.chars().take(fits - 1).collect();
    Some(format!("{kept}…"))
}

/// Build the Vega v6 spec from sector paths and label rows in pixel coordinates.
fn ring_vega_spec(
    arcs: Vec<Value>,
    labels: Vec<Value>,
    width: u64,
    height: u64,
    title: Option<&str>,
) -> Value {
    let mut spec = json!({
        "$schema": "https://vega.github.io/schema/vega/v6.json",
        "width": width,
        "height": height,
        "data": [
            {"name": "arcs", "values": arcs},
            {"name": "labels", "values": labels}
        ],
        "marks": [
            {
                "type": "path",
                "from": {"data": "arcs"},
                "encode": {"enter": {
                    "path": {"field": "d"},
                    "fill": {"value": RING_FILL},
                    "stroke": {"value": RING_LIGHT},
                    "strokeWidth": {"value": RING_BORDER_PX}
                }}
            },
            {
                "type": "text",
                "from": {"data": "labels"},
                "encode": {"enter": {
                    "x": {"field": "x"},
                    "y": {"field": "y"},
                    "text": {"field": "label"},
                    "angle": {"field": "angle"},
                    "align": {"field": "align"},
                    "baseline": {"value": "middle"},
                    "fontSize": {"field": "fontSize"},
                    "fill": {"value": RING_LIGHT}
                }}
            }
        ]
    });
    if let Some(text) = title {
        spec["title"] = json!({"text": text});
    }
    spec
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

    /// Root `r` with children `a` (leaves `a1`, `a2`) and leaf `b`.
    fn nested() -> Vec<TreeLayoutNode> {
        vec![
            node("r", None),
            node("a", Some("r")),
            node("a1", Some("a")),
            node("a2", Some("a")),
            node("b", Some("r")),
        ]
    }

    fn layout(nodes: &[TreeLayoutNode], start_angle: f64) -> Vec<RingArc> {
        layout_ring(nodes, WIDTH as f64, HEIGHT as f64, 360.0, start_angle).unwrap()
    }

    fn by_id<'a>(arcs: &'a [RingArc], taxon_id: &str) -> &'a RingArc {
        arcs.iter().find(|arc| arc.taxon_id == taxon_id).unwrap()
    }

    fn assert_close(actual: f64, expected: f64) {
        assert!((actual - expected).abs() < 1e-9, "{actual} != {expected}");
    }

    #[test]
    fn root_is_not_drawn_and_rings_follow_hop_depth() {
        let arcs = layout(&nested(), 0.0);
        let outer = ring_outer_radius(WIDTH as f64, HEIGHT as f64);
        let hole = CENTRE_HOLE_PX;
        let band = (outer - hole) / 2.0;

        assert_eq!(arcs.len(), 4);
        assert!(arcs.iter().all(|arc| arc.taxon_id != "r"));
        let a = by_id(&arcs, "a");
        assert_close(a.inner_radius, hole);
        assert_close(a.outer_radius, hole + band);
        let a1 = by_id(&arcs, "a1");
        assert_close(a1.inner_radius, hole + band);
        assert_close(a1.outer_radius, outer);
        let b = by_id(&arcs, "b");
        assert_close(b.inner_radius, hole);
        assert_close(b.outer_radius, outer);
    }

    #[test]
    fn leaves_share_the_sweep_and_parents_span_their_leaves() {
        let arcs = layout(&nested(), 0.0);
        let slot = 2.0 * PI / 3.0;

        let a1 = by_id(&arcs, "a1");
        assert_close(a1.start_angle, 0.0);
        assert_close(a1.end_angle, slot);
        let a = by_id(&arcs, "a");
        assert_close(a.start_angle, 0.0);
        assert_close(a.end_angle, 2.0 * slot);
        let b = by_id(&arcs, "b");
        assert_close(b.start_angle, 2.0 * slot);
        assert_close(b.end_angle, 3.0 * slot);
    }

    #[test]
    fn start_angle_rotates_the_whole_ring() {
        let arcs = layout(&nested(), -90.0);

        assert_close(by_id(&arcs, "a1").start_angle, -PI / 2.0);
    }

    #[test]
    fn non_finite_start_angle_is_rejected() {
        let err = layout_ring(&nested(), WIDTH as f64, HEIGHT as f64, 360.0, f64::NAN).unwrap_err();

        assert!(err.contains("start_angle"), "unexpected error: {err}");
    }

    #[test]
    fn full_turn_sector_is_drawn_as_two_arcs() {
        let arc = RingArc {
            taxon_id: "a".to_string(),
            inner_radius: 0.0,
            outer_radius: 100.0,
            start_angle: 0.0,
            end_angle: 2.0 * PI,
        };

        let path = arc_path(&arc, (400.0, 300.0));

        assert_eq!(path.matches('A').count(), 2);
        assert!(!path.contains("NaN"), "{path}");
    }

    fn sector(inner: f64, outer: f64, start: f64, end: f64) -> RingArc {
        RingArc {
            taxon_id: "a".to_string(),
            inner_radius: inner,
            outer_radius: outer,
            start_angle: start,
            end_angle: end,
        }
    }

    fn labelled_tree() -> Value {
        json!({
            "r": {"scientific_name": "Root", "children": {"a": true, "b": true}},
            "a": {"scientific_name": "Alpha", "children": {"a1": true, "a2": true}},
            "a1": {"scientific_name": "Alpha one", "children": {}},
            "a2": {"scientific_name": "Alpha two", "children": {}},
            "b": {"scientific_name": "Beta", "children": {}}
        })
    }

    #[test]
    fn total_angle_sets_the_sweep_of_the_leaves() {
        let arcs = layout_ring(&nested(), WIDTH as f64, HEIGHT as f64, 180.0, 0.0).unwrap();

        assert_close(by_id(&arcs, "b").end_angle, PI);
        assert_close(by_id(&arcs, "a").end_angle, 2.0 * PI / 3.0);
    }

    #[test]
    fn non_positive_total_angle_is_rejected() {
        assert!(layout_ring(&nested(), WIDTH as f64, HEIGHT as f64, 0.0, 0.0).is_err());
    }

    #[test]
    fn truncate_label_keeps_short_names_whole() {
        assert_eq!(
            truncate_label("Chordata", 60.0, 10.0),
            Some("Chordata".to_string())
        );
    }

    #[test]
    fn truncate_label_ends_with_ellipsis_when_shortened() {
        // 23 px of room at a 10 px font fits 4 glyphs of 5.5 px.
        assert_eq!(
            truncate_label("Chordata", 23.0, 10.0),
            Some("Cho…".to_string())
        );
    }

    #[test]
    fn truncate_label_omits_text_that_cannot_fit() {
        assert_eq!(truncate_label("Chordata", 6.0, 10.0), None);
    }

    #[test]
    fn radial_orientation_flip_turns_left_half_inward() {
        let (rotation, align) = radial_orientation(PI, LabelOrientation::Flip);

        assert_eq!(align, "left");
        assert_close(rotation, 0.0);
    }

    #[test]
    fn radial_orientation_continuous_reads_outward_on_every_half() {
        let (rotation, align) = radial_orientation(PI, LabelOrientation::Continuous);

        assert_eq!(align, "right");
        assert_close(rotation, 180.0);
    }

    #[test]
    fn radial_label_is_right_aligned_one_glyph_inside_outer_edge() {
        let arc = sector(0.0, 100.0, 0.0, 0.5);
        let rows = radial_label_rows(
            "Chordata",
            &arc,
            (0.0, 0.0),
            10.0,
            LabelOrientation::Continuous,
        );

        assert_eq!(rows.len(), 1);
        let mid: f64 = 0.25;
        let radius = 100.0 - RING_LABEL_PAD_PX - GLYPH_WIDTH_EM * 10.0;
        assert_eq!(rows[0]["align"], "right");
        assert_close(rows[0]["x"].as_f64().unwrap(), radius * mid.cos());
        assert_close(rows[0]["y"].as_f64().unwrap(), radius * mid.sin());
    }

    #[test]
    fn radial_label_on_left_half_keeps_only_the_pad() {
        let arc = sector(0.0, 100.0, PI - 0.25, PI + 0.25);
        let rows = radial_label_rows("Chordata", &arc, (0.0, 0.0), 10.0, LabelOrientation::Flip);

        assert_eq!(rows[0]["align"], "left");
        assert_close(rows[0]["x"].as_f64().unwrap(), -(100.0 - RING_LABEL_PAD_PX));
    }

    #[test]
    fn narrow_sector_omits_radial_label_when_font_exceeds_arc() {
        let arc = sector(0.0, 100.0, 0.0, 0.01);

        assert!(
            radial_label_rows("Chordata", &arc, (0.0, 0.0), 10.0, LabelOrientation::Flip)
                .is_empty()
        );
    }

    #[test]
    fn wide_sector_gets_one_curved_glyph_per_character() {
        let arc = sector(100.0, 110.0, 0.0, 1.0);
        let centre = (400.0, 300.0);

        let rows = sector_label_rows("Chordata", &arc, centre, 4.0, LabelOrientation::Flip);

        let text: String = rows
            .iter()
            .map(|row| row["label"].as_str().unwrap())
            .collect();
        assert_eq!(text, "Chordata");
        for row in &rows {
            assert_eq!(row["align"], "center");
            let x = row["x"].as_f64().unwrap() - centre.0;
            let y = row["y"].as_f64().unwrap() - centre.1;
            let position = y.atan2(x).to_degrees();
            let tangent = row["angle"].as_f64().unwrap();
            assert!(((tangent - position).rem_euclid(180.0) - 90.0).abs() < 1e-6);
        }
    }

    #[test]
    fn font_size_option_sets_every_ring_label() {
        let tree = labelled_tree();
        let options = TreeRenderOptions {
            font_size: Some(7.0),
            ..TreeRenderOptions::default()
        };

        let spec = ring_tree_vega(
            tree.as_object().unwrap(),
            WIDTH,
            HEIGHT,
            None,
            360.0,
            -90.0,
            options,
        )
        .unwrap();

        let labels = spec["data"][1]["values"].as_array().unwrap();
        assert!(!labels.is_empty());
        assert!(labels.iter().all(|row| row["fontSize"] == 7.0));
    }

    #[test]
    fn default_ring_font_is_ten_pixels() {
        let tree = labelled_tree();

        let spec = ring_tree_vega(
            tree.as_object().unwrap(),
            WIDTH,
            HEIGHT,
            None,
            360.0,
            -90.0,
            TreeRenderOptions::default(),
        )
        .unwrap();

        let labels = spec["data"][1]["values"].as_array().unwrap();
        assert!(labels
            .iter()
            .all(|row| row["fontSize"] == DEFAULT_RING_FONT_PX));
    }

    #[test]
    fn sector_borders_are_half_a_pixel() {
        let tree = labelled_tree();

        let spec = ring_tree_vega(
            tree.as_object().unwrap(),
            WIDTH,
            HEIGHT,
            None,
            360.0,
            -90.0,
            TreeRenderOptions::default(),
        )
        .unwrap();

        assert_eq!(
            spec["marks"][0]["encode"]["enter"]["strokeWidth"]["value"],
            0.5
        );
    }

    #[test]
    fn ring_tree_vega_emits_one_arc_per_non_root_taxon() {
        let tree_nodes = json!({
            "r": {"scientific_name": "Root", "children": {"a": true, "b": true}},
            "a": {"scientific_name": "Alpha", "children": {"a1": true, "a2": true}},
            "a1": {"scientific_name": "Alpha one", "children": {}},
            "a2": {"scientific_name": "Alpha two", "children": {}},
            "b": {"scientific_name": "Beta", "children": {}}
        });
        let map = tree_nodes.as_object().unwrap();

        let spec = ring_tree_vega(
            map,
            WIDTH,
            HEIGHT,
            Some("Ring"),
            360.0,
            -90.0,
            TreeRenderOptions::default(),
        )
        .unwrap();

        assert_eq!(
            spec["$schema"],
            "https://vega.github.io/schema/vega/v6.json"
        );
        assert_eq!(spec["data"][0]["name"], "arcs");
        assert_eq!(spec["data"][0]["values"].as_array().unwrap().len(), 4);
        assert_eq!(spec["title"]["text"], "Ring");
    }

    #[test]
    fn ring_tree_vega_empty_tree_has_no_arcs() {
        let spec = ring_tree_vega(
            &Map::new(),
            WIDTH,
            HEIGHT,
            None,
            360.0,
            0.0,
            TreeRenderOptions::default(),
        )
        .unwrap();

        assert!(spec["data"][0]["values"].as_array().unwrap().is_empty());
        assert!(spec["data"][1]["values"].as_array().unwrap().is_empty());
    }

    proptest! {
        #[test]
        fn arcs_stay_inside_outer_ring_and_radii_are_ordered(leaf_count in 1usize..40) {
            let mut nodes = vec![node("root", None)];
            nodes.extend((0..leaf_count).map(|i| node(&format!("leaf{i:03}"), Some("root"))));

            let arcs = layout(&nodes, 0.0);
            let outer = ring_outer_radius(WIDTH as f64, HEIGHT as f64);

            prop_assert!(arcs.iter().all(|arc| arc.inner_radius >= -1e-9
                && arc.inner_radius <= arc.outer_radius + 1e-9
                && arc.outer_radius <= outer + 1e-9));
            prop_assert!(arcs.iter().all(|arc| arc.start_angle <= arc.end_angle));
        }
    }
}
