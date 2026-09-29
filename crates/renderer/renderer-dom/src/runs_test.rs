use renderer_core::{Color, Declared, Destination, Span};

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
    let runs = runs_of("Meet the team today", &spans);
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
    let runs = runs_of("ñandú", &spans);
    assert_eq!(
        runs.iter().map(|run| run.text.as_str()).collect::<String>(),
        "ñandú"
    );
}

#[test]
fn a_restyled_run_changes_the_signature() {
    let plain = runs_of(
        "ab",
        &[Span::new(0..1, Declared::default().with_font_weight(400))],
    );
    let bold = runs_of(
        "ab",
        &[Span::new(0..1, Declared::default().with_font_weight(700))],
    );
    assert_ne!(signature(&plain), signature(&bold));
}
