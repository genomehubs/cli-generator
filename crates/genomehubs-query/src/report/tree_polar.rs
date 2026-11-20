//! Circular and radial tree layouts for tree reports.
//!
//! Radius is the cumulative branch length scaled to the outer ring, so the root
//! sits at the centre hole (circular) or the centre (radial). Each leaf takes an
//! equal share of the arc, and an internal
//! node sits at the mid-angle of its first and last child. Coordinates are pixels.

use std::collections::HashMap;
use std::f64::consts::PI;

use serde_json::{json, Map, Value};

use super::tree_layout::{LabelOrientation, TipAlignment, TreeLayoutNode, TreeRenderOptions};
use super::tree_rect::{
    layout_nodes_from_tree_nodes, leader_path, spread_positions, tip_font_for_row, tip_label,
    topology, tree_vega_spec, LayoutNode, Topology, LABEL_OFFSET_PX, PADDING_PX,
};

/// Share of the half-size reserved for tip labels outside the outer ring.
const LABEL_BAND_FRACTION: f64 = 0.25;

/// Radius of the empty centre in circular and ring layouts, in pixels.
pub(super) const CENTRE_HOLE_PX: f64 = 12.0;

/// How a node connects to its parent.
#[derive(Clone, Copy)]
enum Connector {
    /// Radial stub at the node's angle plus an arc at the parent's radius (circular).
    Arc,
    /// Straight line from the parent's point to the node's point (radial).
    Ray,
}

/// Angular extent, start angle, and connector shape of a polar tree.
#[derive(Clone, Copy)]
struct PolarSweep {
    total_angle: f64,
    start_angle: f64,
    connector: Connector,
    /// Radius of the empty centre in pixels; the root sits here.
    hole_px: f64,
}

/// Build a Vega v6 spec for a circular tree whose leaves span `total_angle` degrees from `start_angle`.
///
/// Angles are in degrees, measured clockwise from the positive x axis. Returns an
/// error when `total_angle` is outside `(0, 360]` or `start_angle` is not finite.
pub fn circ_tree_vega(
    tree_nodes: &Map<String, Value>,
    width: u64,
    height: u64,
    title: Option<&str>,
    total_angle: f64,
    start_angle: f64,
    options: TreeRenderOptions,
) -> Result<Value, String> {
    let sweep = PolarSweep {
        total_angle,
        start_angle,
        connector: Connector::Arc,
        hole_px: CENTRE_HOLE_PX,
    };
    polar_tree_vega(tree_nodes, width, height, title, sweep, options)
}

/// Build a Vega v6 spec for a radial tree that fills the full circle from `start_angle`.
///
/// Returns an error when `start_angle` is not finite.
pub fn radial_tree_vega(
    tree_nodes: &Map<String, Value>,
    width: u64,
    height: u64,
    title: Option<&str>,
    start_angle: f64,
    options: TreeRenderOptions,
) -> Result<Value, String> {
    let sweep = PolarSweep {
        total_angle: 360.0,
        start_angle,
        connector: Connector::Ray,
        hole_px: 0.0,
    };
    polar_tree_vega(tree_nodes, width, height, title, sweep, options)
}

/// Lay out a polar tree from a `treeNodes` object and assemble its Vega v6 spec.
fn polar_tree_vega(
    tree_nodes: &Map<String, Value>,
    width: u64,
    height: u64,
    title: Option<&str>,
    sweep: PolarSweep,
    options: TreeRenderOptions,
) -> Result<Value, String> {
    let (plot_width, plot_height) = (width as f64, height as f64);
    let polar_nodes = layout_polar(
        &layout_nodes_from_tree_nodes(tree_nodes),
        plot_width,
        plot_height,
        sweep,
        options.alignment,
    )?;
    let leaf_count = polar_nodes.iter().filter(|node| node.is_leaf).count();
    let arc_row = outer_radius(plot_width, plot_height) * sweep.total_angle.to_radians()
        / leaf_count.max(1) as f64;
    let font_size = options
        .font_size
        .unwrap_or_else(|| tip_font_for_row(arc_row));
    let centre = (plot_width / 2.0, plot_height / 2.0);
    let label_radius = outer_radius(plot_width, plot_height) + LABEL_OFFSET_PX;
    let links: Vec<Value> = polar_nodes
        .iter()
        .flat_map(|node| [node.arm_path.as_deref(), node.bar_path.as_deref()])
        .flatten()
        .map(|path| json!({ "d": path }))
        .collect();
    let (tips, leaders) = polar_tip_marks(tree_nodes, &polar_nodes, centre, label_radius, options);
    Ok(tree_vega_spec(
        links, leaders, tips, width, height, title, font_size,
    ))
}

/// Tip label anchors and dotted leaders for the leaves of a polar layout.
///
/// With `TipAlignment::Labels` every label sits on a shared ring just beyond the
/// outer ring, and a leader runs from its tip out to that ring. Otherwise labels
/// sit just past their tips.
fn polar_tip_marks(
    tree_nodes: &Map<String, Value>,
    polar_nodes: &[LayoutNode],
    centre: (f64, f64),
    label_radius: f64,
    options: TreeRenderOptions,
) -> (Vec<Value>, Vec<Value>) {
    let mut leaders: Vec<Value> = Vec::new();
    let tips: Vec<Value> = polar_nodes
        .iter()
        .filter(|node| node.is_leaf)
        .map(|node| {
            let angle = (node.y - centre.1).atan2(node.x - centre.0);
            let (rotation, align) = label_orientation(angle, options.label_orientation);
            let (x, y) = match options.alignment {
                TipAlignment::Labels => {
                    let anchor = (
                        centre.0 + label_radius * angle.cos(),
                        centre.1 + label_radius * angle.sin(),
                    );
                    let leader = leader_path((node.x, node.y), anchor);
                    leaders.push(json!({ "d": leader }));
                    anchor
                }
                TipAlignment::None | TipAlignment::Tips | TipAlignment::Ultrametric => (
                    node.x + LABEL_OFFSET_PX * angle.cos(),
                    node.y + LABEL_OFFSET_PX * angle.sin(),
                ),
            };
            json!({
                "x": x,
                "y": y,
                "label": tip_label(tree_nodes, &node.taxon_id),
                "angle": rotation,
                "align": align
            })
        })
        .collect();
    (tips, leaders)
}

/// Lay out every taxon on the polar grid described by `sweep`, in depth-first order.
///
/// Returns an error when `total_angle` is outside `(0, 360]`, `start_angle` is not
/// finite, or the tree is invalid.
fn layout_polar(
    nodes: &[TreeLayoutNode],
    width: f64,
    height: f64,
    sweep: PolarSweep,
    alignment: TipAlignment,
) -> Result<Vec<LayoutNode>, String> {
    validate_angles(sweep.total_angle, sweep.start_angle)?;
    if nodes.is_empty() {
        return Ok(Vec::new());
    }
    let topo = topology(nodes, alignment)?;
    let total = sweep.total_angle.to_radians();
    let start = sweep.start_angle.to_radians();
    let angles = spread_positions(&topo.order, &topo.children, |row, leaf_count| {
        start + total * (row as f64 + 0.5) / leaf_count as f64
    });
    let polar = Polar {
        centre: (width / 2.0, height / 2.0),
        radii: radii_by_id(
            &topo.order,
            &topo.depths,
            sweep.hole_px,
            outer_radius(width, height),
        ),
        angles,
    };
    Ok(topo
        .order
        .iter()
        .map(|id| polar_node(id, &topo, &polar, sweep.connector))
        .collect())
}

/// Reject angle settings that cannot produce a valid layout.
pub(super) fn validate_angles(total_angle: f64, start_angle: f64) -> Result<(), String> {
    if !(total_angle > 0.0 && total_angle <= 360.0) {
        return Err(format!(
            "total_angle must be in (0, 360], got {total_angle}"
        ));
    }
    if !start_angle.is_finite() {
        return Err(format!("start_angle must be finite, got {start_angle}"));
    }
    Ok(())
}

/// Radius of the outer ring, leaving a band for tip labels and the plot padding.
fn outer_radius(width: f64, height: f64) -> f64 {
    let half = width.min(height) / 2.0;
    (half * (1.0 - LABEL_BAND_FRACTION) - PADDING_PX).max(0.0)
}

/// Scale each node's cumulative depth onto `[hole, outer]`, so the root sits at `hole`.
fn radii_by_id<'a>(
    order: &[&'a str],
    depths: &HashMap<String, f64>,
    hole: f64,
    outer: f64,
) -> HashMap<&'a str, f64> {
    let hole = hole.min(outer);
    let max_depth = depths.values().copied().fold(0.0, f64::max);
    order
        .iter()
        .map(|&id| {
            let radius = if max_depth > 0.0 {
                hole + depths[id] / max_depth * (outer - hole)
            } else {
                hole
            };
            (id, radius)
        })
        .collect()
}

/// Centre and per-taxon polar coordinates for one layout.
struct Polar<'a> {
    centre: (f64, f64),
    radii: HashMap<&'a str, f64>,
    angles: HashMap<&'a str, f64>,
}

impl Polar<'_> {
    /// Pixel position at `radius` along `angle` (radians) from the centre.
    fn at(&self, radius: f64, angle: f64) -> (f64, f64) {
        (
            self.centre.0 + radius * angle.cos(),
            self.centre.1 + radius * angle.sin(),
        )
    }

    /// Pixel position of a taxon.
    fn point(&self, taxon_id: &str) -> (f64, f64) {
        self.at(self.radii[taxon_id], self.angles[taxon_id])
    }
}

/// Build the pixel geometry and link paths for one taxon.
fn polar_node(
    taxon_id: &str,
    topo: &Topology<'_>,
    polar: &Polar<'_>,
    connector: Connector,
) -> LayoutNode {
    let (x, y) = polar.point(taxon_id);
    let kids = &topo.children[taxon_id];
    let bar_path = match (connector, kids.first(), kids.last()) {
        (Connector::Arc, Some(first), Some(last)) if polar.radii[taxon_id] > 0.0 => {
            Some(arc_path(taxon_id, first, last, polar))
        }
        _ => None,
    };
    LayoutNode {
        taxon_id: taxon_id.to_string(),
        x,
        y,
        is_leaf: kids.is_empty(),
        arm_path: topo
            .parent_of
            .get(taxon_id)
            .map(|parent| arm_path(taxon_id, parent, polar, connector)),
        bar_path,
    }
}

/// Path from the parent toward this taxon.
///
/// Arcs leave the parent's radius on this taxon's angle, so the stub is radial.
/// Rays join the parent's point and this taxon's point directly.
fn arm_path(taxon_id: &str, parent_id: &str, polar: &Polar<'_>, connector: Connector) -> String {
    let (x, y) = polar.point(taxon_id);
    let (start_x, start_y) = match connector {
        Connector::Arc => polar.at(polar.radii[parent_id], polar.angles[taxon_id]),
        Connector::Ray => polar.point(parent_id),
    };
    format!("M{start_x:.2},{start_y:.2}L{x:.2},{y:.2}")
}

/// Arc at this taxon's radius from its first child's angle to its last child's angle.
fn arc_path(taxon_id: &str, first_id: &str, last_id: &str, polar: &Polar<'_>) -> String {
    let radius = polar.radii[taxon_id];
    let (start_x, start_y) = polar.at(radius, polar.angles[first_id]);
    let (end_x, end_y) = polar.at(radius, polar.angles[last_id]);
    let large_arc = u8::from(polar.angles[last_id] - polar.angles[first_id] > PI);
    format!(
        "M{start_x:.2},{start_y:.2}A{radius:.2},{radius:.2} 0 {large_arc} 1 {end_x:.2},{end_y:.2}"
    )
}

/// Rotation in degrees and alignment for a tip label at `angle` radians.
///
/// `Flip` turns labels on the left half around, so the text stays upright and is
/// aligned to the anchor's inner side. `Continuous` keeps the radial rotation on
/// every half, so the left half reads upside down.
pub(super) fn label_orientation(angle: f64, orientation: LabelOrientation) -> (f64, &'static str) {
    let degrees = angle.to_degrees();
    match orientation {
        LabelOrientation::Continuous => (degrees.rem_euclid(360.0), "left"),
        LabelOrientation::Flip if angle.cos() < 0.0 => {
            ((degrees + 180.0).rem_euclid(360.0), "right")
        }
        LabelOrientation::Flip => (degrees.rem_euclid(360.0), "left"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const WIDTH: f64 = 800.0;
    const HEIGHT: f64 = 600.0;

    fn tree_node(
        taxon_id: &str,
        parent_id: Option<&str>,
        branch_length: Option<f64>,
    ) -> TreeLayoutNode {
        TreeLayoutNode {
            taxon_id: taxon_id.to_string(),
            parent_id: parent_id.map(str::to_string),
            branch_length,
        }
    }

    /// Root `r` with leaves `a` and `b`; `a` is internal with leaves `a1` and `a2`.
    fn nested_tree() -> Vec<TreeLayoutNode> {
        vec![
            tree_node("r", None, None),
            tree_node("a", Some("r"), Some(1.0)),
            tree_node("a1", Some("a"), Some(1.0)),
            tree_node("a2", Some("a"), Some(1.0)),
            tree_node("b", Some("r"), Some(1.0)),
        ]
    }

    /// Root `r` with four unit-length leaves `a`, `b`, `c`, `d`.
    fn four_leaves() -> Vec<TreeLayoutNode> {
        vec![
            tree_node("r", None, None),
            tree_node("a", Some("r"), Some(1.0)),
            tree_node("b", Some("r"), Some(1.0)),
            tree_node("c", Some("r"), Some(1.0)),
            tree_node("d", Some("r"), Some(1.0)),
        ]
    }

    fn by_id<'a>(laid_out: &'a [LayoutNode], taxon_id: &str) -> &'a LayoutNode {
        laid_out.iter().find(|n| n.taxon_id == taxon_id).unwrap()
    }

    fn centre() -> (f64, f64) {
        (WIDTH / 2.0, HEIGHT / 2.0)
    }

    /// Lay out with the default, unaligned tip placement.
    fn polar(
        nodes: &[TreeLayoutNode],
        total_angle: f64,
        start_angle: f64,
        connector: Connector,
    ) -> Result<Vec<LayoutNode>, String> {
        let sweep = PolarSweep {
            total_angle,
            start_angle,
            connector,
            hole_px: 0.0,
        };
        layout_polar(nodes, WIDTH, HEIGHT, sweep, TipAlignment::None)
    }

    fn radius_of(point: &LayoutNode) -> f64 {
        let (cx, cy) = centre();
        (point.x - cx).hypot(point.y - cy)
    }

    fn angle_of(point: &LayoutNode) -> f64 {
        let (cx, cy) = centre();
        (point.y - cy).atan2(point.x - cx)
    }

    #[test]
    fn root_sits_at_centre() {
        let laid_out = polar(&four_leaves(), 360.0, -90.0, Connector::Arc).unwrap();
        let root = by_id(&laid_out, "r");
        let (cx, cy) = centre();
        assert!((root.x - cx).abs() < 1e-9);
        assert!((root.y - cy).abs() < 1e-9);
    }

    #[test]
    fn centre_hole_keeps_root_off_the_centre() {
        let sweep = PolarSweep {
            total_angle: 360.0,
            start_angle: -90.0,
            connector: Connector::Arc,
            hole_px: CENTRE_HOLE_PX,
        };
        let laid_out =
            layout_polar(&four_leaves(), WIDTH, HEIGHT, sweep, TipAlignment::None).unwrap();
        assert!((radius_of(by_id(&laid_out, "r")) - CENTRE_HOLE_PX).abs() < 1e-9);
        let outer = outer_radius(WIDTH, HEIGHT);
        for leaf in laid_out.iter().filter(|n| n.is_leaf) {
            assert!((radius_of(leaf) - outer).abs() < 1e-9);
        }
    }

    #[test]
    fn radius_scales_with_cumulative_depth() {
        let nodes = vec![
            tree_node("r", None, None),
            tree_node("a", Some("r"), Some(1.0)),
            tree_node("a1", Some("a"), Some(1.0)),
            tree_node("b", Some("r"), Some(3.0)),
        ];
        let laid_out = polar(&nodes, 360.0, -90.0, Connector::Arc).unwrap();
        let outer = outer_radius(WIDTH, HEIGHT);
        assert!((radius_of(by_id(&laid_out, "b")) - outer).abs() < 1e-9);
        assert!((radius_of(by_id(&laid_out, "a1")) - outer * 2.0 / 3.0).abs() < 1e-9);
    }

    #[test]
    fn leaves_stay_within_total_angle_span() {
        let laid_out = polar(&four_leaves(), 90.0, -90.0, Connector::Arc).unwrap();
        let start = (-90.0_f64).to_radians();
        let span = 90.0_f64.to_radians();
        for leaf in laid_out.iter().filter(|n| n.is_leaf) {
            let offset = (angle_of(leaf) - start).rem_euclid(2.0 * PI);
            assert!(
                offset > 0.0 && offset < span,
                "offset {offset} outside span"
            );
        }
    }

    #[test]
    fn start_angle_places_first_leaf_at_three_oclock() {
        let nodes = vec![
            tree_node("r", None, None),
            tree_node("a", Some("r"), Some(1.0)),
            tree_node("b", Some("r"), Some(1.0)),
        ];
        let laid_out = polar(&nodes, 360.0, -90.0, Connector::Arc).unwrap();
        let first = by_id(&laid_out, "a");
        let second = by_id(&laid_out, "b");
        let (cx, cy) = centre();
        assert!((first.y - cy).abs() < 1e-9);
        assert!(first.x > cx);
        assert!(second.x < cx);
    }

    #[test]
    fn labels_on_left_half_are_flipped() {
        let (rotation, align) = label_orientation(0.0, LabelOrientation::Flip);
        assert_eq!(align, "left");
        assert!(rotation.abs() < 1e-9);

        let (rotation, align) = label_orientation(PI, LabelOrientation::Flip);
        assert_eq!(align, "right");
        let normalised = rotation.rem_euclid(360.0);
        assert!(normalised < 1e-9 || 360.0 - normalised < 1e-9);
    }

    #[test]
    fn continuous_labels_keep_radial_rotation_on_left_half() {
        let (rotation, align) = label_orientation(PI, LabelOrientation::Continuous);
        assert_eq!(align, "left");
        assert!((rotation - 180.0).abs() < 1e-9);
    }

    #[test]
    fn circular_uses_arcs_and_radial_uses_rays() {
        let circ = polar(&nested_tree(), 360.0, -90.0, Connector::Arc).unwrap();
        assert!(by_id(&circ, "a").bar_path.as_deref().unwrap().contains('A'));
        assert!(by_id(&circ, "r").bar_path.is_none());

        let rays = polar(&nested_tree(), 360.0, -90.0, Connector::Ray).unwrap();
        assert!(rays.iter().all(|n| n.bar_path.is_none()));
        assert!(rays
            .iter()
            .filter_map(|n| n.arm_path.as_deref())
            .all(|path| !path.contains('A')));
    }

    #[test]
    fn rejects_invalid_angles() {
        let nodes = four_leaves();
        assert!(polar(&nodes, 0.0, -90.0, Connector::Arc).is_err());
        assert!(polar(&nodes, 400.0, -90.0, Connector::Arc).is_err());
        assert!(polar(&nodes, 360.0, f64::NAN, Connector::Ray).is_err());
    }

    #[test]
    fn tips_alignment_puts_every_leaf_on_outer_ring() {
        let sweep = PolarSweep {
            total_angle: 360.0,
            start_angle: -90.0,
            connector: Connector::Arc,
            hole_px: 0.0,
        };
        let laid_out =
            layout_polar(&nested_tree(), WIDTH, HEIGHT, sweep, TipAlignment::Tips).unwrap();
        let outer = outer_radius(WIDTH, HEIGHT);
        for leaf in laid_out.iter().filter(|n| n.is_leaf) {
            assert!((radius_of(leaf) - outer).abs() < 1e-9);
        }
    }

    #[test]
    fn labels_alignment_places_labels_on_ring_with_leaders() {
        let tree_nodes = json!({
            "r": {"scientific_name": "Root", "children": {"a": true, "b": true}},
            "a": {"scientific_name": "Alpha", "children": {}, "branch_length": 1.0},
            "b": {"scientific_name": "Beta", "children": {}, "branch_length": 3.0}
        });
        let map = tree_nodes.as_object().unwrap();

        let spec = circ_tree_vega(
            map,
            800,
            600,
            None,
            360.0,
            -90.0,
            TreeRenderOptions::with_alignment(TipAlignment::Labels),
        )
        .unwrap();

        let tips = spec["data"][1]["values"].as_array().unwrap();
        let leaders = spec["data"][2]["values"].as_array().unwrap();
        let label_radius = outer_radius(WIDTH, HEIGHT) + LABEL_OFFSET_PX;
        let (cx, cy) = centre();
        assert_eq!(leaders.len(), 2);
        for tip in tips {
            let x = tip["x"].as_f64().unwrap();
            let y = tip["y"].as_f64().unwrap();
            assert!(((x - cx).hypot(y - cy) - label_radius).abs() < 1e-6);
        }
    }

    proptest! {
        #[test]
        fn leaves_stay_inside_outer_ring(
            leaf_count in 1usize..40,
            total in 1.0f64..=360.0,
            start in -720.0f64..720.0,
        ) {
            let mut nodes = vec![tree_node("root", None, None)];
            nodes.extend((0..leaf_count).map(|i| {
                tree_node(&format!("t{i:02}"), Some("root"), Some(1.0 + i as f64))
            }));
            let laid_out = polar(&nodes, total, start, Connector::Arc).unwrap();
            let outer = outer_radius(WIDTH, HEIGHT);
            for leaf in laid_out.iter().filter(|n| n.is_leaf) {
                prop_assert!(radius_of(leaf) <= outer + 1e-6);
            }
        }
    }
}
