use std::sync::Mutex;

use super::*;

/// A source holding exactly the names it was built with, recording every batch it was asked for.
struct Fixed {
    label: &'static str,
    names: &'static [&'static str],
    asked: Mutex<Vec<Vec<String>>>,
}

impl Fixed {
    fn new(label: &'static str, names: &'static [&'static str]) -> Self {
        Self {
            label,
            names,
            asked: Mutex::new(Vec::new()),
        }
    }
}

impl IconSource for Fixed {
    fn describe(&self) -> String {
        self.label.to_string()
    }

    fn icon(&self, id: &IconId) -> Result<Option<SourcedIcon>, IconError> {
        Ok(self.names.contains(&id.name()).then(|| SourcedIcon {
            svg: format!("<svg>{}</svg>", self.label),
            set: None,
            own: false,
            origin: self.label.to_string(),
        }))
    }

    fn icons(&self, ids: &[IconId]) -> Vec<Result<Option<SourcedIcon>, IconError>> {
        self.asked
            .lock()
            .unwrap()
            .push(ids.iter().map(IconId::to_string).collect());
        ids.iter().map(|id| self.icon(id)).collect()
    }
}

/// A source whose every answer is a failure.
struct Broken;

impl IconSource for Broken {
    fn describe(&self) -> String {
        "broken".to_string()
    }

    fn icon(&self, id: &IconId) -> Result<Option<SourcedIcon>, IconError> {
        Err(IconError::Fetch {
            url: id.path(),
            message: "down".to_string(),
        })
    }
}

fn id(text: &str) -> IconId {
    IconId::parse(text).unwrap()
}

#[test]
fn the_first_source_that_has_an_icon_answers() {
    let sources = Sources::new()
        .with(Fixed::new("own", &["logo"]))
        .with(Fixed::new("sets", &["logo", "home"]));
    assert_eq!(sources.icon(&id("x:logo")).unwrap().unwrap().origin, "own");
    assert_eq!(sources.icon(&id("x:home")).unwrap().unwrap().origin, "sets");
    assert!(sources.icon(&id("x:none")).unwrap().is_none());
}

#[test]
fn a_broken_source_is_reported_rather_than_skipped() {
    let sources = Sources::new()
        .with(Broken)
        .with(Fixed::new("sets", &["home"]));
    assert!(sources.icon(&id("x:home")).is_err());
}

#[test]
fn a_batch_reaches_each_source_with_only_what_earlier_ones_lacked() {
    let first = Fixed::new("own", &["logo"]);
    let second = Fixed::new("sets", &["home"]);
    let sources = Sources::new().with(first).with(second);
    let answers = sources.icons(&[id("x:logo"), id("x:home"), id("x:none")]);
    let origins: Vec<Option<String>> = answers
        .into_iter()
        .map(|answer| answer.unwrap().map(|icon| icon.origin))
        .collect();
    assert_eq!(
        origins,
        vec![Some("own".to_string()), Some("sets".to_string()), None]
    );
}

#[test]
fn a_chain_describes_every_source_in_order() {
    let sources = Sources::new()
        .with(Fixed::new("one", &[]))
        .with(Fixed::new("two", &[]));
    assert_eq!(sources.describe(), "one, then two");
    assert_eq!(Sources::new().describe(), "no icon source");
}
