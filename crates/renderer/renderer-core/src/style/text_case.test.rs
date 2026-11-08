use super::*;
use crate::{Color, Declared};

fn style(case: TextCase) -> TextStyle {
    TextStyle::new(14.0, Color::BLACK).with_text_case(case)
}

#[test]
fn each_case_maps_the_whole_string() {
    assert_eq!(TextCase::Upper.apply("straße menu", None), "STRASSE MENU");
    assert_eq!(TextCase::Lower.apply("ABOUT Me", None), "about me");
    assert_eq!(
        TextCase::Capitalize.apply("don't stop-me now", None),
        "Don't Stop-Me Now"
    );
    assert_eq!(TextCase::AsWritten.apply("Mixed", None), "Mixed");
}

#[test]
fn capitalize_leaves_the_rest_of_each_word_as_written() {
    assert_eq!(
        TextCase::Capitalize.apply("iPhone mcDonald", None),
        "IPhone McDonald"
    );
}

#[test]
fn the_language_picks_the_rules() {
    assert_eq!(TextCase::Upper.apply("istanbul", Some("tr")), "İSTANBUL");
    assert_eq!(TextCase::Upper.apply("istanbul", Some("en")), "ISTANBUL");
    assert_eq!(
        TextCase::Lower.apply("DİYARBAKIR", Some("tr")),
        "diyarbakır"
    );
    assert_eq!(TextCase::Upper.apply("Καλημέρα", Some("el")), "ΚΑΛΗΜΕΡΑ");
    assert_eq!(
        TextCase::Upper.apply("istanbul", Some("not a tag!")),
        "ISTANBUL",
        "a tag that does not parse takes the language-neutral rules"
    );
}

#[test]
fn text_with_nothing_to_recase_is_borrowed_untouched() {
    let cased = case_text("hello", None, &style(TextCase::AsWritten));
    assert!(matches!(cased.text, Cow::Borrowed("hello")));
    assert_eq!(cased.source_index(3), 3);

    let already = case_text("HELLO", None, &style(TextCase::Upper));
    assert!(matches!(already.text, Cow::Borrowed("HELLO")));
}

#[test]
fn spans_follow_their_words_when_casing_changes_the_length() {
    let text = "\u{fb01}x \u{fb01}x";
    let spans = [Span::new(5..9, Declared::default().with_font_weight(700))];
    let cased = case_text(text, Some(&spans), &style(TextCase::Upper));
    assert_eq!(cased.text, "FIX FIX");
    let moved = cased.spans.clone().expect("spans survive");
    assert_eq!(moved[0].range, 4..7);
    assert_eq!(
        cased.source_index(5),
        5,
        "a shown byte maps back to the word it came from"
    );
    assert_eq!(cased.source_index(2), 0);
}

#[test]
fn a_span_can_ask_for_a_case_of_its_own() {
    let text = "read MORE";
    let spans = [Span::new(
        5..9,
        Declared::default().with_text_case(TextCase::Lower),
    )];
    let cased = case_text(text, Some(&spans), &style(TextCase::Upper));
    assert_eq!(cased.text, "READ more");

    let upper = [Span::new(
        5..9,
        Declared::default().with_text_case(TextCase::Upper),
    )];
    let only_span = case_text("read more", Some(&upper), &style(TextCase::AsWritten));
    assert_eq!(only_span.text, "read MORE");
}

#[test]
fn the_style_language_reaches_the_mapping() {
    let cased = case_text("izmir", None, &style(TextCase::Upper).with_lang("tr"));
    assert_eq!(cased.text, "İZMİR");
}
