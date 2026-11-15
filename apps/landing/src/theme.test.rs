use telar::contrast_shortfalls;

use super::*;

#[test]
fn the_theme_reaches_every_minimum_of_the_contrast_table_at_either_contrast() {
    let shortfalls: Vec<String> = contrast_shortfalls(LandingTheme::light())
        .iter()
        .map(ToString::to_string)
        .collect();
    assert!(shortfalls.is_empty(), "{shortfalls:#?}");
}
