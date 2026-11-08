use super::*;
use crate::Color;
use geometry_core::Size;

#[test]
fn text_style_new_stores_font_size() {
    let style = TextStyle::new(16.0, Color::BLACK);
    assert_eq!(style.font_size, 16.0);
}

#[test]
fn text_style_new_stores_color() {
    let style = TextStyle::new(12.0, Color::WHITE);
    assert_eq!(style.color, Paint::Solid(Color::WHITE));
}

#[test]
fn text_style_defaults_to_natural_spacing() {
    let style = TextStyle::new(16.0, Color::BLACK);
    assert_eq!(style.line_height, LineHeight::Natural);
    assert_eq!(style.letter_spacing, 0.0);
}

#[test]
fn text_style_builders_set_spacing() {
    let style = TextStyle::new(16.0, Color::BLACK)
        .with_line_height(1.5)
        .with_letter_spacing(2.0);
    assert_eq!(style.line_height, LineHeight::Times(1.5));
    assert_eq!(style.letter_spacing, 2.0);
}

#[test]
fn an_underline_cascades_one_longhand_at_a_time() {
    let base = TextStyle::new(20.0, Color::BLACK);
    let offset_above = Declared::default().with_underline_offset(TextLength::Em(0.2));
    let shown_on_hover = Declared::default()
        .with_underline(true)
        .with_underline_color(Color::WHITE);
    let resolved = shown_on_hover.over(&offset_above.over(&base, Size::ZERO), Size::ZERO);
    assert!(resolved.decoration.is_drawn());
    assert_eq!(resolved.decoration.offset, DecorationMetric::Px(4.0));
    assert_eq!(resolved.decoration.thickness, DecorationMetric::FromFont);
    assert_eq!(resolved.decoration.color, Some(Paint::Solid(Color::WHITE)));
    let taken_away = Declared::default()
        .with_underline(false)
        .over(&resolved, Size::ZERO);
    assert!(!taken_away.decoration.is_drawn());
    assert_eq!(taken_away.decoration.offset, DecorationMetric::Px(4.0));
}

#[test]
fn a_case_cascades_and_changes_the_extent_but_an_underline_does_not() {
    let base = TextStyle::new(14.0, Color::BLACK);
    let upper = Declared::default()
        .with_text_case(TextCase::Upper)
        .over(&base, Size::ZERO);
    assert_eq!(upper.text_case, TextCase::Upper);
    assert!(!upper.same_extent(&base));
    assert!(base.clone().with_underline(true).same_extent(&base));
}

#[test]
fn a_surface_fraction_offset_waits_for_the_surface() {
    let declared = Declared::default().with_underline_offset(TextLength::SurfaceWidth(0.01));
    assert!(declared.uses_surface());
    assert_eq!(
        declared
            .on_surface(Size::new(400.0, 300.0))
            .decoration_offset,
        Some(DecorationLength::Length(TextLength::Px(4.0)))
    );
}
