use super::*;
use renderer_core::BlendMode;

fn emitted(mode: BlendMode) -> String {
    format!("BlendMode::{mode:?}")
}

#[test]
fn every_blend_mode_the_renderer_has_can_be_written_by_its_css_name() {
    for mode in BlendMode::ALL {
        assert_eq!(
            keyword(BLEND_VALUES, mode.css_name()),
            Some(emitted(mode).as_str()),
            "{mode:?}"
        );
    }
}

#[test]
fn every_blend_spelling_offered_is_emitted_as_a_mode_the_renderer_has() {
    for (name, variant) in BLEND_VALUES {
        let named = BlendMode::ALL
            .into_iter()
            .find(|mode| emitted(*mode) == *variant)
            .unwrap_or_else(|| {
                panic!("`{name}` is emitted as `{variant}`, which is no `BlendMode`")
            });
        let is_css_name = named.css_name() == *name;
        let is_bare_plus = named == BlendMode::Plus && *name == "plus";
        assert!(
            is_css_name || is_bare_plus,
            "`{name}` does not spell {named:?}"
        );
    }
}
