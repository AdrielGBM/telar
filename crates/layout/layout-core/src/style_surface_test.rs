use super::*;

const PHONE: Size = Size::new(400.0, 800.0);

#[test]
fn each_unit_is_a_fraction_of_its_own_side() {
    let style = LayoutStyle::new()
        .width(SizeDimension::SurfaceWidth(0.5))
        .height(SizeDimension::SurfaceHeight(0.25))
        .min_width(SizeDimension::SurfaceMin(0.1))
        .max_width(SizeDimension::SurfaceMax(0.1));
    let resolved = style.resolve(Direction::Ltr, PHONE);
    assert_eq!(resolved.size.width, Dimension::length(200.0));
    assert_eq!(resolved.size.height, Dimension::length(200.0));
    assert_eq!(resolved.min_size.width, LengthPercentageAuto::length(40.0));
    assert_eq!(resolved.max_size.width, LengthPercentageAuto::length(80.0));
}

#[test]
fn resolving_again_on_another_surface_does_not_remember_the_first() {
    let style = LayoutStyle::new().padding_all(SizeDimension::SurfaceWidth(0.05));
    let _ = style.resolve(Direction::Ltr, PHONE);
    let wide = style.resolve(Direction::Ltr, Size::new(1000.0, 800.0));
    assert_eq!(wide.padding.left, LengthPercentage::length(50.0));
    assert_eq!(wide.padding.top, LengthPercentage::length(50.0));
}

#[test]
fn logical_edges_resolve_against_the_surface_and_the_direction_together() {
    let style = LayoutStyle::new()
        .padding_start(SizeDimension::SurfaceWidth(0.1))
        .margin_inline_end(SizeDimension::SurfaceWidth(0.05))
        .inset_start(SizeDimension::SurfaceHeight(0.01));
    let rtl = style.resolve(Direction::Rtl, PHONE);
    assert_eq!(rtl.padding.right, LengthPercentage::length(40.0));
    assert_eq!(rtl.margin.left, LengthPercentageAuto::length(20.0));
    assert_eq!(rtl.inset.right, LengthPercentageAuto::length(8.0));
}

#[test]
fn a_later_pixel_value_replaces_an_earlier_fraction() {
    let style = LayoutStyle::new()
        .width(SizeDimension::SurfaceWidth(0.5))
        .width(120.0)
        .padding_all(SizeDimension::SurfaceWidth(0.1))
        .padding_left(3.0);
    assert!(style.is_surface_relative(), "three padding edges still are");
    let resolved = style.resolve(Direction::Ltr, PHONE);
    assert_eq!(resolved.size.width, Dimension::length(120.0));
    assert_eq!(resolved.padding.left, LengthPercentage::length(3.0));
    assert_eq!(resolved.padding.right, LengthPercentage::length(40.0));
}

#[test]
fn a_style_without_fractions_is_not_surface_relative() {
    let style = LayoutStyle::new()
        .width(SizeDimension::Percent(0.5))
        .padding_start(8.0);
    assert!(!style.is_surface_relative());
}

#[test]
fn a_surface_width_is_not_auto_even_before_it_has_a_value() {
    let style = LayoutStyle::new().width(SizeDimension::SurfaceWidth(1.0));
    assert!(!style.is_width_auto());
    assert_eq!(style.width_px(), None);
}

#[test]
fn a_grid_track_is_a_fraction_of_the_surface_until_resolved_against_one() {
    let style = LayoutStyle::new()
        .display_grid()
        .grid_template_columns(vec![
            TemplateTrack::length(SizeDimension::SurfaceWidth(0.25)),
            TemplateTrack::minmax(
                TemplateTrack::length(SizeDimension::SurfaceMin(0.1)),
                TemplateTrack::fr(1.0),
            ),
        ]);
    assert!(style.is_surface_relative());
    let resolved = style.resolve(Direction::Ltr, PHONE);
    let expected: Vec<taffy::GridTemplateComponent<String>> = vec![
        taffy::GridTemplateComponent::Single(taffy::style_helpers::length(100.0)),
        taffy::GridTemplateComponent::Single(taffy::style_helpers::minmax(
            taffy::MinTrackSizingFunction::length(40.0),
            taffy::MaxTrackSizingFunction::fr(1.0),
        )),
    ];
    assert_eq!(resolved.grid_template_columns, expected);
}

#[test]
fn pixel_tracks_leave_the_style_independent_of_the_surface() {
    let style = LayoutStyle::new()
        .display_grid()
        .grid_template_columns(vec![TemplateTrack::px(120.0), TemplateTrack::fr(1.0)])
        .grid_auto_rows(vec![TemplateTrack::auto()]);
    assert!(!style.is_surface_relative());
}

#[test]
fn replacing_surface_tracks_with_pixel_ones_forgets_the_fraction() {
    let style = LayoutStyle::new()
        .grid_template_rows(vec![TemplateTrack::length(SizeDimension::SurfaceHeight(
            0.5,
        ))])
        .grid_template_rows(vec![TemplateTrack::px(30.0)]);
    assert!(!style.is_surface_relative());
}
