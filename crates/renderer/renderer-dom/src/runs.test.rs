use renderer_core::{Color, Declared, Destination, Span, TextStyle};

use super::*;

#[test]
fn a_paragraph_is_cut_at_its_spans_with_plain_text_between() {
    let spans = [
        Span::new(0..4, Declared::default().with_font_weight(700)),
        Span::new(
            9..13,
            Declared::default().with_color(Color::rgb(1.0, 0.0, 0.0)),
        )
        .linking_to(Destination::external("https://example.com").unwrap()),
    ];
    let runs = runs("Meet the team today", &spans);
    let texts: Vec<&str> = runs.iter().map(|run| run.text.as_str()).collect();
    assert_eq!(texts, ["Meet", " the ", "team", " today"]);
    assert_eq!(runs[0].css, "font-weight:700;");
    assert!(runs[1].css.is_empty() && runs[1].link.is_none());
    let link = runs[2].link.as_ref().expect("the span links");
    assert_eq!(link.href, "https://example.com");
    assert!(link.external && link.opens_beside);
    assert_eq!(
        link.run, 1,
        "the span's position among the spans, not among the runs"
    );
}

#[test]
fn a_span_that_does_not_fit_the_text_is_skipped() {
    let spans = [
        Span::new(2..40, Declared::default()),
        Span::new(1..2, Declared::default()),
    ];
    let runs = runs("ñandú", &spans);
    assert_eq!(
        runs.iter().map(|run| run.text.as_str()).collect::<String>(),
        "ñandú"
    );
}

#[test]
fn a_restyled_run_changes_the_signature() {
    let plain = runs(
        "ab",
        &[Span::new(0..1, Declared::default().with_font_weight(400))],
    );
    let bold = runs(
        "ab",
        &[Span::new(0..1, Declared::default().with_font_weight(700))],
    );
    assert_ne!(signature(&plain), signature(&bold));
}

fn runs(text: &str, spans: &[Span]) -> Vec<Run> {
    runs_with(text, spans, &mut |_, _| 0.0)
}

fn runs_with(
    text: &str,
    spans: &[Span],
    width_of: &mut dyn FnMut(&str, &TextStyle) -> f32,
) -> Vec<Run> {
    runs_of(
        text,
        spans,
        &TextStyle::new(20.0, Color::BLACK),
        crate::document::DEFAULT_ROOT_FONT_SIZE,
        width_of,
    )
}

/// Every letter as wide as its size, so a test can say how wide a run is.
fn square(text: &str, style: &TextStyle) -> f32 {
    text.chars().count() as f32 * style.font_size
}

#[test]
fn a_reading_is_written_as_ruby_over_its_run_with_parentheses_for_what_sets_none() {
    let spans = [Span::new(0..6, Declared::default()).with_ruby("かんじ")];
    let runs = runs_with("漢字を", &spans, &mut square);
    let ruby = runs[0].ruby.as_ref().expect("the span has a reading");
    assert_eq!(ruby.reading, "かんじ");
    assert_eq!(ruby.css, "font-size:0.5em;line-height:1;");
    let mut html = String::new();
    crate::runs::ruby_markup(&mut html, &runs[0].text, ruby);
    assert_eq!(
        html,
        "<ruby style=\"white-space:nowrap;\">漢字<rp>(</rp><rt style=\"font-size:0.5em;line-height:1;\">かんじ</rt><rp>)</rp></ruby>"
    );
}

#[test]
fn a_reading_longer_than_its_base_is_written_smaller_and_tighter() {
    let spans = [Span::new(0..3, Declared::default()).with_ruby("うけたまわ")];
    let runs = runs_with("承る", &spans, &mut square);
    let css = &runs[0].ruby.as_ref().expect("a reading").css;
    assert!(css.starts_with("font-size:0.46em;"), "{css}");
    assert!(css.contains("letter-spacing:-"), "{css}");
}

#[test]
fn a_changed_reading_changes_the_signature() {
    let read = |reading: &str| {
        signature(&runs(
            "ab",
            &[Span::new(0..1, Declared::default()).with_ruby(reading)],
        ))
    };
    assert_ne!(read("x"), read("y"));
}
