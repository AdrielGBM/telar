use platform_core::{WindowCommand, take_window_commands};
use reactive_core::{effect, signal};

use super::*;
use crate::Surface;

fn titles_announced() -> Vec<String> {
    take_window_commands()
        .into_iter()
        .filter_map(|command| match command {
            WindowCommand::SetTitle(title) => Some(title),
            _ => None,
        })
        .collect()
}

fn parts<'a>(app: &'a str, page: Option<&'a str>) -> TitleParts<'a> {
    TitleParts { app, page }
}

#[test]
fn the_default_rule_puts_the_page_before_the_app() {
    assert_eq!(
        compose_title(&parts("Portfolio", Some("Credits"))),
        "Credits — Portfolio"
    );
    assert_eq!(compose_title(&parts("Portfolio", None)), "Portfolio");
    assert_eq!(compose_title(&parts("", Some("Credits"))), "Credits");
    assert_eq!(
        compose_title(&parts("Portfolio", Some("Portfolio"))),
        "Portfolio"
    );
}

#[test]
fn opening_on_the_title_the_platform_already_shows_tells_it_nothing() {
    open_surface_title("Portfolio", "Portfolio");
    assert_eq!(surface_title(), "Portfolio");
    assert!(titles_announced().is_empty());
}

#[test]
fn a_page_title_is_announced_once_per_change() {
    open_surface_title("Portfolio", "Portfolio");
    set_page_title(Some("Credits".into()));
    set_page_title(Some("Credits".into()));
    assert_eq!(surface_title(), "Credits — Portfolio");
    assert_eq!(titles_announced(), ["Credits — Portfolio"]);

    set_page_title(None);
    assert_eq!(titles_announced(), ["Portfolio"]);
}

#[test]
fn an_empty_page_title_is_no_page_title() {
    open_surface_title("Portfolio", "Portfolio");
    set_page_title(Some(String::new()));
    assert_eq!(surface_title(), "Portfolio");
    assert!(titles_announced().is_empty());
}

#[test]
fn a_title_the_platform_does_not_show_yet_is_announced_on_opening() {
    open_surface_title("Portfolio", "Credits — Portfolio");
    assert_eq!(titles_announced(), ["Portfolio"]);
}

#[test]
fn reading_the_title_follows_it() {
    open_surface_title("Portfolio", "Portfolio");
    let seen = signal(String::new());
    effect(move || seen.set(use_surface_title()));
    set_page_title(Some("Act I".into()));
    assert_eq!(seen.peek(), "Act I — Portfolio");
}

#[test]
fn a_page_title_derived_from_a_signal_follows_it() {
    open_surface_title("Portfolio", "Portfolio");
    let locale = signal("es");
    effect(move || {
        let page = if locale.get() == "es" {
            "Créditos"
        } else {
            "Credits"
        };
        set_page_title(Some(page.to_owned()));
    });
    assert_eq!(surface_title(), "Créditos — Portfolio");
    locale.set("en");
    assert_eq!(surface_title(), "Credits — Portfolio");
    assert_eq!(
        titles_announced(),
        ["Créditos — Portfolio", "Credits — Portfolio"]
    );
}

#[test]
fn a_rule_that_reads_a_signal_is_followed() {
    open_surface_title("Portfolio", "Portfolio");
    let separator = signal(" | ");
    set_title_format(move |parts| {
        let page = parts.page.unwrap_or("Home");
        format!("{page}{}{}", separator.get(), parts.app)
    });
    assert_eq!(surface_title(), "Home | Portfolio");
    separator.set(" · ");
    assert_eq!(surface_title(), "Home · Portfolio");
}

#[test]
fn the_app_can_rename_itself_and_a_second_opening_keeps_the_name() {
    open_surface_title("RSX App", "RSX App");
    set_app_title("Portfolio");
    assert_eq!(surface_title(), "Portfolio");
    open_surface_title("RSX App", "Portfolio");
    assert_eq!(surface_title(), "Portfolio");
    assert_eq!(titles_announced(), ["Portfolio"]);
}

#[test]
fn clearing_a_page_title_nobody_set_creates_nothing() {
    set_page_title(None);
    assert!(existing().is_none());
    assert!(titles_announced().is_empty());
}

#[test]
fn every_surface_has_its_own_title_and_announces_it_to_its_own_queue() {
    let a = Surface::new();
    let b = Surface::new();
    {
        let _a = a.enter();
        open_surface_title("Editor", "Editor");
    }
    {
        let _b = b.enter();
        open_surface_title("Inspector", "Inspector");
    }
    let page = signal(None::<String>);
    {
        let _a = a.enter();
        effect(move || set_page_title(page.get()));
    }
    page.set(Some("notes.txt".into()));
    {
        let _b = b.enter();
        assert_eq!(surface_title(), "Inspector");
        assert!(titles_announced().is_empty());
    }
    let _a = a.enter();
    assert_eq!(surface_title(), "notes.txt — Editor");
    assert_eq!(titles_announced(), ["notes.txt — Editor"]);
}
