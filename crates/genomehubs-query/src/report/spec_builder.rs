//! Shared axis-display resolution logic used by both the API-side builder and
//! the local-report builder.
//!
//! This module lives in `genomehubs-query` (WASM-compatible) so that
//! `local_report` can call it without depending on `genomehubs-api`.

use super::display::{AxisOptions, TickLabelPlacement};
use super::plot_spec::AxisMeta;

/// Set the resolved display fields on an [`AxisMeta`] from an optional
/// [`AxisOptions`] hint.
///
/// Resolution rules (single source of truth shared by API and local builds):
///
/// - `tick_label_placement`: user hint → auto (`between_ticks` for keywords,
///   `on_tick` for everything else).
/// - `tick_label_stride`: user hint → `1`.
/// - `tick_label_max_length`: passed through from the hint, or `None`.
/// - `label`: user hint takes precedence over the field-name default.
pub fn resolve_axis_display(meta: &mut AxisMeta, opts: Option<&AxisOptions>) {
    meta.tick_label_placement =
        opts.and_then(|o| o.tick_label_placement)
            .unwrap_or(match meta.value_type.as_str() {
                "keyword" => TickLabelPlacement::BetweenTicks,
                _ => TickLabelPlacement::OnTick,
            });
    meta.tick_label_stride = opts.and_then(|o| o.tick_label_stride).unwrap_or(1);
    meta.tick_label_max_length = opts.and_then(|o| o.tick_label_max_length);

    if let Some(label) = opts.and_then(|o| o.label.clone()) {
        meta.label = Some(label);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::display::AxisOptions;
    use crate::report::plot_spec::AxisMeta;

    fn make_meta(value_type: &str) -> AxisMeta {
        AxisMeta {
            field: "test_field".to_string(),
            label: None,
            scale: "linear".to_string(),
            domain: [0.0, 1.0],
            tick_values: vec![],
            tick_labels: vec![],
            value_type: value_type.to_string(),
            tick_label_placement: TickLabelPlacement::OnTick,
            tick_label_stride: 1,
            tick_label_max_length: None,
        }
    }

    #[test]
    fn keyword_axis_defaults_to_between_ticks() {
        let mut meta = make_meta("keyword");
        resolve_axis_display(&mut meta, None);
        assert_eq!(meta.tick_label_placement, TickLabelPlacement::BetweenTicks);
        assert_eq!(meta.tick_label_stride, 1);
        assert!(meta.tick_label_max_length.is_none());
    }

    #[test]
    fn float_axis_defaults_to_on_tick() {
        let mut meta = make_meta("float");
        resolve_axis_display(&mut meta, None);
        assert_eq!(meta.tick_label_placement, TickLabelPlacement::OnTick);
    }

    #[test]
    fn user_hint_overrides_auto_placement() {
        let mut meta = make_meta("keyword");
        let opts = AxisOptions {
            tick_label_placement: Some(TickLabelPlacement::OnTick),
            tick_label_stride: Some(3),
            tick_label_max_length: Some(8),
            label: Some("My Label".to_string()),
            tick_label_angle: None,
            show_tick_labels: None,
            number_format: None,
        };
        resolve_axis_display(&mut meta, Some(&opts));
        assert_eq!(meta.tick_label_placement, TickLabelPlacement::OnTick);
        assert_eq!(meta.tick_label_stride, 3);
        assert_eq!(meta.tick_label_max_length, Some(8));
        assert_eq!(meta.label.as_deref(), Some("My Label"));
    }
}
