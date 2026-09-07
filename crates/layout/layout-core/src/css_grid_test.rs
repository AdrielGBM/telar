use super::*;
use crate::track::TemplateTrack;

fn css(style: LayoutStyle) -> String {
    style.to_css(Direction::Ltr).into_string()
}

#[test]
fn a_grid_says_its_columns() {
    let out = css(LayoutStyle::new()
        .display_grid()
        .grid_template_columns(vec![TemplateTrack::fr(1.0), TemplateTrack::px(200.0)]));
    assert!(out.contains("display:grid;"), "got {out}");
    assert!(
        out.contains("grid-template-columns:1fr 200px;"),
        "got {out}"
    );
}

/// What `grid cols:"fit 150"` means, and the case that made the sandbox stack: a grid with no track list is one implicit column, so every card came out full width.
#[test]
fn an_auto_fitting_repeat_keeps_its_keyword() {
    let out = css(LayoutStyle::new()
        .display_grid()
        .grid_template_columns(vec![TemplateTrack::fit(TemplateTrack::minmax(
            TemplateTrack::px(150.0),
            TemplateTrack::fr(1.0),
        ))]));
    assert!(
        out.contains("grid-template-columns:repeat(auto-fit,minmax(150px,1fr));"),
        "got {out}"
    );
}

#[test]
fn a_filling_repeat_says_auto_fill() {
    let out = css(LayoutStyle::new()
        .display_grid()
        .grid_template_columns(vec![TemplateTrack::fill(TemplateTrack::px(100.0))]));
    assert!(
        out.contains("grid-template-columns:repeat(auto-fill,100px);"),
        "got {out}"
    );
}

#[test]
fn a_counted_repeat_says_the_count() {
    let out = css(LayoutStyle::new()
        .display_grid()
        .grid_template_columns(vec![TemplateTrack::repeat(3, TemplateTrack::fr(1.0))]));
    assert!(
        out.contains("grid-template-columns:repeat(3,1fr);"),
        "got {out}"
    );
}

#[test]
fn a_span_is_stated_and_a_single_track_is_not() {
    assert!(
        css(LayoutStyle::new().grid_column_span(2)).contains("grid-column:span 2;"),
        "{}",
        css(LayoutStyle::new().grid_column_span(2))
    );
    assert!(
        !css(LayoutStyle::new().grid_column_span(1)).contains("grid-column"),
        "a single-track span is the default, so it needs no declaration: {}",
        css(LayoutStyle::new().grid_column_span(1))
    );
}

/// A track list only means anything on a grid, and saying it elsewhere is noise the browser parses.
#[test]
fn a_flex_box_does_not_describe_tracks() {
    let out = css(LayoutStyle::new()
        .flex_row()
        .grid_template_columns(vec![TemplateTrack::fr(1.0)]));
    assert!(!out.contains("grid-template-columns"), "got {out}");
}
