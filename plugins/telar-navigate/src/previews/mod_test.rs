use std::collections::HashSet;

use telar::preview::a11y::Severity;
use telar::preview::{Play, PreviewEntry};

use super::previews;

fn mount(entry: &PreviewEntry) -> Play {
    Play::mount(&entry.locale("en"))
        .unwrap_or_else(|error| panic!("`{}` does not mount: {error}", entry.id))
}

#[test]
fn every_preview_has_its_own_id() {
    let entries = previews();
    let ids: HashSet<&str> = entries.iter().map(|entry| entry.id).collect();
    assert_eq!(ids.len(), entries.len());
}

#[test]
fn every_preview_mounts_and_breaks_no_accessibility_rule_that_is_an_error() {
    for entry in previews() {
        let report = mount(&entry).check_a11y();
        assert_eq!(
            report.at_least(Severity::Error).count(),
            0,
            "`{}`:\n{report}",
            entry.id
        );
    }
}

#[test]
fn the_stack_follows_its_screen_arg() {
    let entry = previews()
        .into_iter()
        .find(|entry| entry.component == "nav_host")
        .expect("a nav host preview");
    let mut play = mount(&entry);
    play.expect_text("Inbox").unwrap();
    play.ctx()
        .signal("screen", super::nav_host::Screen::Inbox)
        .set(super::nav_host::Screen::Reply);
    play.settle().unwrap();
    play.expect_text("Friday works for me.").unwrap();
    play.expect_no_text("Three unread messages.").unwrap();
}
