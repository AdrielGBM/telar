//! Plays and accessibility checks against the catalogue's own previews, mounted the way the workshop mounts them.

use telar::preview::a11y::{Rule, Severity};
use telar::preview::{Play, PreviewEntry, by_role};
use telar::{Key, NamedKey, Role};

fn preview(id: &str) -> PreviewEntry {
    telar_components::telar_all_previews()
        .into_iter()
        .find(|entry| entry.id == id)
        .unwrap_or_else(|| panic!("the catalogue has no preview `{id}`"))
}

#[test]
fn a_play_presses_the_counting_button_and_sees_its_press_logged() {
    let entry = preview("telar_components--button--counting-presses").play(|canvas| {
        canvas.click(by_role(Role::Button).named("Pressed 0 times"))?;
        canvas.expect_text("Pressed 1 times")?;
        canvas.expect_action("on_press", 1)
    });
    let mut play = Play::mount(&entry).unwrap();
    let outcome = play.run();
    let log: Vec<String> = play
        .steps()
        .iter()
        .map(|step| format!("{} → {:?}", step.action, step.outcome))
        .collect();
    assert_eq!(outcome, Ok(()), "{log:#?}");
    assert_eq!(play.steps().len(), 3);
}

#[test]
fn a_play_types_into_the_bound_text_field() {
    let entry = preview("telar_components--text_field--bound-value");
    let mut play = Play::mount(&entry).unwrap();
    play.type_text(by_role(Role::TextInput), "Ada").unwrap();
    play.expect_text("Ada").unwrap();
    play.press(Key::Named(NamedKey::Backspace)).unwrap();
    play.expect_no_text("Ada").unwrap();
    play.expect_text("Ad").unwrap();
}

#[test]
fn the_keyboard_reaches_the_button_and_activates_it() {
    let mut play = Play::mount(&preview("telar_components--button--counting-presses")).unwrap();
    play.tab().unwrap();
    play.expect_focused(by_role(Role::Button)).unwrap();
    play.press(Key::Named(NamedKey::Enter)).unwrap();
    play.expect_text("Pressed 1 times").unwrap();
}

#[test]
fn the_default_button_preview_breaks_no_accessibility_rule_that_is_an_error() {
    let entry = preview("telar_components--button--default").locale("en");
    let report = Play::mount(&entry).unwrap().check_a11y();
    assert_eq!(report.at_least(Severity::Error).count(), 0, "{report}");
    assert_eq!(report.of(Rule::MissingLang).count(), 0, "{report}");
}

#[test]
fn no_preview_breaks_an_accessibility_rule_that_is_an_error() {
    let broken: Vec<String> = telar_components::telar_all_previews()
        .into_iter()
        .filter_map(|entry| {
            let entry = entry.locale("en");
            let report = Play::mount(&entry)
                .unwrap_or_else(|error| panic!("`{}` does not mount: {error}", entry.id))
                .check_a11y();
            let errors: Vec<String> = report
                .at_least(Severity::Error)
                .map(ToString::to_string)
                .collect();
            (!errors.is_empty()).then(|| format!("{}:\n  {}", entry.id, errors.join("\n  ")))
        })
        .collect();
    assert!(broken.is_empty(), "{}", broken.join("\n"));
}

#[cfg(feature = "overlays")]
#[test]
fn a_toast_puts_itself_away_once_its_time_has_passed() {
    use std::time::Duration;

    let mut play = Play::mount(&preview("telar_components--toaster--stack")).unwrap();
    play.click(by_role(Role::Button).named("Info")).unwrap();
    play.expect_text("A new version is available.").unwrap();
    play.advance(Duration::from_secs(4)).unwrap();
    play.expect_text("A new version is available.").unwrap();
    play.advance(Duration::from_secs(1)).unwrap();
    play.expect_no_text("A new version is available.").unwrap();
    play.expect_text("Your changes are saved.").unwrap();
}
