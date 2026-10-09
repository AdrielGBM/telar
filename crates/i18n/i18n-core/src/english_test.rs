use crate::{Catalog, Message, Part, translate};

static CATALOG: Catalog = Catalog {
    locales: &["en"],
    default_locale: "en",
    entries: &[
        english!("close", "Close"),
        english!("fps", { count }, " fps"),
        english!("percent", { value }, "%"),
        english!("reset", "Reset ", { name }),
    ],
};

#[test]
fn a_plain_string_is_a_plain_english_message() {
    let entry = &CATALOG.entries[0];
    assert_eq!(entry.messages.len(), 1);
    assert_eq!(entry.messages[0].0, "en");
    assert!(matches!(entry.messages[0].1, Message::Plain("Close")));
}

#[test]
fn placeholders_and_literals_keep_their_order() {
    let Message::Format(parts) = &CATALOG.entries[3].messages[0].1 else {
        panic!("a placeholder makes a format message");
    };
    assert!(matches!(parts, [Part::Lit("Reset "), Part::Arg("name")]));
    assert_eq!(translate(&CATALOG, "fps", &[("count", "60")]), "60 fps");
    assert_eq!(translate(&CATALOG, "percent", &[("value", "50")]), "50%");
    assert_eq!(
        translate(&CATALOG, "reset", &[("name", "label")]),
        "Reset label"
    );
}

#[test]
fn the_entries_are_sorted() {
    CATALOG.assert_sorted();
}
