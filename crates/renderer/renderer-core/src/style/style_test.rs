use super::*;
use crate::Color;

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
