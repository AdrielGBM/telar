use super::*;
use platform_core::{
    Destination, HistoryStep, HistoryUpdate, Location, LocationFormat, WindowCommand, address_of,
    location_history, location_locale, push_location, receive_location_history,
    take_window_commands,
};

fn at(path: &str) -> Location {
    LocationFormat::root()
        .with_locales(["es", "en"])
        .parse(path)
        .unwrap()
}

fn replaced_with() -> Vec<Vec<Location>> {
    take_window_commands()
        .into_iter()
        .filter_map(|command| match command {
            WindowCommand::Navigate(HistoryUpdate {
                step: HistoryStep::Replace,
                history,
            }) => Some(history),
            _ => None,
        })
        .collect()
}

#[test]
fn the_address_decides_the_locale_the_app_opens_in() {
    i18n_core::set_locale("es");
    follow_location_locale(["es", "en"], "es");
    receive_location_history(vec![at("/en/"), at("/en/projects")]);
    assert_eq!(i18n_core::current_locale().as_deref(), Some("en"));
    assert_eq!(location_locale().as_deref(), Some("en"));
    assert!(
        replaced_with().is_empty(),
        "an address already in its locale is left as it is"
    );
}

#[test]
fn an_address_naming_none_is_written_in_the_locale_already_set() {
    i18n_core::set_locale("en");
    follow_location_locale(["es", "en"], "es");
    receive_location_history(vec![at("/"), at("/projects")]);
    assert_eq!(replaced_with(), [vec![at("/en/"), at("/en/projects")]]);
    assert_eq!(location_history(), [at("/en/"), at("/en/projects")]);
}

#[test]
fn switching_rewrites_the_entry_shown_and_keeps_the_page_and_the_anchor() {
    follow_location_locale(["es", "en"], "es");
    receive_location_history(vec![at("/es/"), at("/es/#contact")]);
    take_window_commands();
    i18n_core::set_locale("en");
    assert_eq!(
        replaced_with(),
        [vec![at("/en/"), at("/en/#contact")]],
        "a language is how a place is shown, so it adds no entry"
    );
    i18n_core::set_locale("en-GB");
    assert!(
        replaced_with().is_empty(),
        "a regional variant is carried as the locale the app ships"
    );
    i18n_core::set_locale("fr");
    assert_eq!(replaced_with(), [vec![at("/es/"), at("/es/#contact")]]);
}

#[test]
fn links_are_written_in_the_locale_the_app_is_in() {
    follow_location_locale(["es", "en"], "es");
    receive_location_history(vec![at("/en/")]);
    assert_eq!(
        address_of(&Destination::Route(Location::from_segments(["projects"]))),
        "/en/projects"
    );
    assert_eq!(address_of(&Destination::anchor("contact")), "/en/#contact");
    assert_eq!(
        address_of(&Destination::Route(
            Location::from_segments(["projects"]).with_locale("es")
        )),
        "/es/projects",
        "a link to another language says so"
    );
}

#[test]
fn an_entry_returned_to_from_before_a_switch_is_shown_in_the_locale_the_app_is_in() {
    follow_location_locale(["es", "en"], "es");
    receive_location_history(vec![at("/es/a")]);
    i18n_core::set_locale("en");
    take_window_commands();
    receive_location_history(vec![at("/es/a")]);
    assert_eq!(i18n_core::current_locale().as_deref(), Some("en"));
    assert_eq!(replaced_with(), [vec![at("/en/a")]]);
}

#[test]
fn a_link_to_the_place_shown_in_another_locale_switches_to_it() {
    follow_location_locale(["es", "en"], "es");
    receive_location_history(vec![at("/es/a#team")]);
    take_window_commands();
    assert!(push_location(at("/en/a#team")));
    assert_eq!(i18n_core::current_locale().as_deref(), Some("en"));
    assert_eq!(
        take_window_commands()
            .into_iter()
            .filter_map(|command| match command {
                WindowCommand::Navigate(update) => Some(update),
                _ => None,
            })
            .collect::<Vec<_>>(),
        [HistoryUpdate {
            step: HistoryStep::Replace,
            history: vec![at("/en/a#team")],
        }]
    );
}

#[test]
fn a_renamed_anchor_takes_the_address_with_it() {
    follow_location_locale(["es", "en"], "es");
    receive_location_history(vec![at("/es/"), at("/es/#simulacion")]);
    take_window_commands();
    i18n_core::set_locale("en");
    platform_core::rename_anchor("simulacion", "simulation");
    assert_eq!(location_history(), [at("/en/"), at("/en/#simulation")]);
}
