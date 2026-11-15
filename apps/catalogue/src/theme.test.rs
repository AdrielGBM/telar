use telar::{contrast_shortfalls, set_mode};

use super::*;

#[test]
fn both_variants_reach_every_minimum_of_the_contrast_table_at_either_contrast() {
    register_modes();
    let mut shortfalls = Vec::new();
    for (mode, theme) in [("light", AppTheme::light()), ("dark", AppTheme::dark())] {
        set_mode(mode);
        shortfalls.extend(
            contrast_shortfalls(theme)
                .iter()
                .map(|shortfall| format!("{mode}: {shortfall}")),
        );
    }
    assert!(shortfalls.is_empty(), "{shortfalls:#?}");
}
