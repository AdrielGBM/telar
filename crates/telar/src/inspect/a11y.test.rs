use std::sync::Arc;

use super::*;
use crate::preview::{Play, PreviewCtx, PreviewEntry};
use crate::{
    Accessible, Border, Container, FixedLayer, LayoutError, LayoutItem, LayoutStyle, PathData,
    PathStyle, Point, RectStyle, ShapeStyle, StyledContainer, Text, TextStyle, box_item,
};

type Built = Result<Box<dyn LayoutItem>, LayoutError>;

const INK: Color = Color::BLACK;
const PAPER: Color = Color::WHITE;
/// 3.04:1 on white: enough for large text and not for body text.
const GREY: Color = Color::rgb(148.0 / 255.0, 148.0 / 255.0, 148.0 / 255.0);

fn label(text: &'static str, size: f32, color: Color) -> Box<dyn LayoutItem> {
    box_item(
        Text::new(
            move || text.to_string(),
            LayoutStyle::new(),
            move || TextStyle::new(size, color),
        )
        .unwrap(),
    )
}

fn button(text: Option<&'static str>, width: f32, height: f32) -> Box<dyn LayoutItem> {
    let children = text
        .map(|text| label(text, 12.0, INK))
        .into_iter()
        .collect();
    box_item(
        StyledContainer::new(
            LayoutStyle::new().width(width).height(height),
            |_| RectStyle::default(),
            children,
        )
        .unwrap()
        .control(Role::Button)
        .on_press(|| {}),
    )
}

fn square() -> crate::Path {
    let data = Arc::new(PathData::polygon(&[
        Point::new(0.0, 0.0),
        Point::new(32.0, 0.0),
        Point::new(32.0, 32.0),
        Point::new(0.0, 32.0),
    ]));
    crate::Path::new(LayoutStyle::new().width(32.0).height(32.0), move |_| {
        (Arc::clone(&data), PathStyle::default().with_fill(INK))
    })
    .unwrap()
}

fn column(children: Vec<Box<dyn LayoutItem>>) -> Built {
    Ok(box_item(Container::new(
        LayoutStyle::new().flex_column().gap(16.0),
        children,
    )?))
}

fn row(style: LayoutStyle, children: Vec<Box<dyn LayoutItem>>) -> Built {
    Ok(box_item(Container::new(style, children)?))
}

fn report(build: fn(&PreviewCtx) -> Built) -> Report {
    report_of(entry(build).locale(const { &crate::locale_tag!("en") }))
}

fn entry(build: fn(&PreviewCtx) -> Built) -> PreviewEntry {
    PreviewEntry::new("a11y--case--default", "case", "Default", build).bg(PAPER)
}

fn report_of(entry: PreviewEntry) -> Report {
    Play::mount(&entry).unwrap().check_a11y()
}

fn found(report: &Report, rule: Rule) -> usize {
    report.of(rule).count()
}

#[test]
fn a_control_that_draws_no_words_is_unnamed() {
    fn icon_only(_: &PreviewCtx) -> Built {
        column(vec![button(None, 40.0, 40.0)])
    }
    fn labelled(_: &PreviewCtx) -> Built {
        column(vec![button(Some("Save"), 40.0, 40.0)])
    }
    fn named_by_the_app(_: &PreviewCtx) -> Built {
        column(vec![button(None, 40.0, 40.0).a11y_label(|| "Save")])
    }
    assert_eq!(found(&report(icon_only), Rule::UnnamedControl), 1);
    assert_eq!(found(&report(labelled), Rule::UnnamedControl), 0);
    assert_eq!(found(&report(named_by_the_app), Rule::UnnamedControl), 0);
}

#[test]
fn a_picture_needs_a_name_or_to_be_hidden() {
    fn bare(_: &PreviewCtx) -> Built {
        column(vec![box_item(square())])
    }
    fn named(_: &PreviewCtx) -> Built {
        column(vec![box_item(square().a11y_label(|| "Logo"))])
    }
    fn hidden(_: &PreviewCtx) -> Built {
        column(vec![box_item(square().a11y_hidden())])
    }
    fn hidden_with_its_group(_: &PreviewCtx) -> Built {
        column(vec![box_item(
            Container::new(LayoutStyle::new(), vec![box_item(square())])?.a11y_hidden(),
        )])
    }
    fn inside_a_control(_: &PreviewCtx) -> Built {
        column(vec![box_item(
            StyledContainer::new(
                LayoutStyle::new().width(48.0).height(48.0),
                |_| RectStyle::default(),
                vec![box_item(square()), label("Save", 12.0, INK)],
            )?
            .control(Role::Button)
            .on_press(|| {}),
        )])
    }
    fn inside_a_control_under_a_dialog(_: &PreviewCtx) -> Built {
        let control = StyledContainer::new(
            LayoutStyle::new().width(48.0).height(48.0),
            |_| RectStyle::default(),
            vec![box_item(square()), label("Save", 12.0, INK)],
        )?
        .control(Role::Button)
        .on_press(|| {});
        let dialog = crate::Overlay::new(
            LayoutStyle::new().flex_column(),
            vec![button(Some("Close"), 48.0, 48.0)],
        )?;
        column(vec![box_item(control), box_item(dialog)])
    }
    assert_eq!(found(&report(bare), Rule::UnnamedPicture), 1);
    assert_eq!(found(&report(named), Rule::UnnamedPicture), 0);
    assert_eq!(found(&report(hidden), Rule::UnnamedPicture), 0);
    assert_eq!(
        found(&report(hidden_with_its_group), Rule::UnnamedPicture),
        0
    );
    assert_eq!(found(&report(inside_a_control), Rule::UnnamedPicture), 0);
    assert_eq!(
        found(
            &report(inside_a_control_under_a_dialog),
            Rule::UnnamedPicture
        ),
        0,
        "a picture stays its control's while a dialog keeps a reader away from both"
    );
}

#[test]
fn faint_body_text_fails_contrast_where_the_same_ink_passes_as_large_text() {
    fn faint_body(_: &PreviewCtx) -> Built {
        column(vec![label("Fine print", 14.0, GREY)])
    }
    fn faint_large(_: &PreviewCtx) -> Built {
        column(vec![label("Heading", 24.0, GREY)])
    }
    fn dark_body(_: &PreviewCtx) -> Built {
        column(vec![label("Body", 14.0, INK)])
    }
    let report_of_faint = report(faint_body);
    let violations: Vec<&Violation> = report_of_faint.of(Rule::Contrast).collect();
    assert_eq!(violations.len(), 1, "{report_of_faint}");
    assert!(violations[0].message.contains("Fine print"));
    assert_eq!(found(&report(faint_large), Rule::Contrast), 0);
    assert_eq!(found(&report(dark_body), Rule::Contrast), 0);
}

#[test]
fn contrast_is_measured_against_the_fill_behind_the_text() {
    fn light_on_dark(_: &PreviewCtx) -> Built {
        column(vec![box_item(StyledContainer::new(
            LayoutStyle::new().padding_all(8.0),
            |_| RectStyle::default().with_fill(INK),
            vec![label("Inverted", 14.0, PAPER)],
        )?)])
    }
    fn grey_on_a_translucent_veil(_: &PreviewCtx) -> Built {
        column(vec![box_item(StyledContainer::new(
            LayoutStyle::new().padding_all(8.0),
            |_| RectStyle::default().with_fill(Color::rgba(0.0, 0.0, 0.0, 0.5)),
            vec![label("Veiled", 24.0, GREY)],
        )?)])
    }
    assert_eq!(found(&report(light_on_dark), Rule::Contrast), 0);
    assert_eq!(
        found(&report(grey_on_a_translucent_veil), Rule::Contrast),
        1,
        "grey on white darkened by half no longer reaches 3:1"
    );
}

#[test]
fn small_targets_fail_only_when_something_else_is_within_reach() {
    fn crowded(_: &PreviewCtx) -> Built {
        column(vec![row(
            LayoutStyle::new().flex_row().gap(2.0),
            vec![button(Some("A"), 16.0, 16.0), button(Some("B"), 16.0, 16.0)],
        )?])
    }
    fn spaced(_: &PreviewCtx) -> Built {
        column(vec![row(
            LayoutStyle::new().flex_row().gap(40.0),
            vec![button(Some("A"), 16.0, 16.0), button(Some("B"), 16.0, 16.0)],
        )?])
    }
    fn large(_: &PreviewCtx) -> Built {
        column(vec![row(
            LayoutStyle::new().flex_row(),
            vec![button(Some("A"), 32.0, 32.0), button(Some("B"), 32.0, 32.0)],
        )?])
    }
    assert_eq!(found(&report(crowded), Rule::TargetSize), 2);
    assert_eq!(found(&report(spaced), Rule::TargetSize), 0);
    assert_eq!(found(&report(large), Rule::TargetSize), 0);
}

#[test]
fn a_tab_stop_hidden_from_readers_is_reported() {
    fn hidden(_: &PreviewCtx) -> Built {
        column(vec![box_item(
            Container::new(LayoutStyle::new(), vec![button(Some("Ghost"), 40.0, 40.0)])?
                .a11y_hidden(),
        )])
    }
    fn shown(_: &PreviewCtx) -> Built {
        column(vec![button(Some("Shown"), 40.0, 40.0)])
    }
    assert_eq!(found(&report(hidden), Rule::HiddenFocusable), 1);
    assert_eq!(found(&report(shown), Rule::HiddenFocusable), 0);
}

#[test]
fn tab_moving_back_against_the_reading_order_is_reported() {
    fn reversed(_: &PreviewCtx) -> Built {
        column(vec![row(
            LayoutStyle::new().flex_row_reverse(),
            vec![
                button(Some("Second"), 40.0, 40.0),
                button(Some("First"), 40.0, 40.0),
            ],
        )?])
    }
    fn in_order(_: &PreviewCtx) -> Built {
        column(vec![
            row(
                LayoutStyle::new().flex_row(),
                vec![
                    button(Some("One"), 40.0, 40.0),
                    button(Some("Two"), 40.0, 40.0),
                ],
            )?,
            button(Some("Three"), 40.0, 40.0),
        ])
    }
    let report_of_reversed = report(reversed);
    let violations: Vec<&Violation> = report_of_reversed.of(Rule::FocusOrder).collect();
    assert_eq!(violations.len(), 1, "{report_of_reversed}");
    assert!(violations[0].message.contains("\"First\""));
    assert_eq!(found(&report(in_order), Rule::FocusOrder), 0);
}

#[test]
fn right_to_left_reading_order_is_not_reported_as_backwards() {
    fn row_of_two(_: &PreviewCtx) -> Built {
        column(vec![row(
            LayoutStyle::new().flex_row(),
            vec![
                button(Some("One"), 40.0, 40.0),
                button(Some("Two"), 40.0, 40.0),
            ],
        )?])
    }
    let rtl = entry(row_of_two)
        .locale(const { &crate::locale_tag!("ar") })
        .dir(crate::Direction::Rtl);
    assert_eq!(found(&report_of(rtl), Rule::FocusOrder), 0);
}

#[test]
fn a_canvas_with_text_and_no_language_is_reported() {
    fn text(_: &PreviewCtx) -> Built {
        column(vec![label("Hello", 14.0, INK)])
    }
    assert_eq!(found(&report_of(entry(text)), Rule::MissingLang), 1);
    assert_eq!(found(&report(text), Rule::MissingLang), 0);
}

#[test]
fn a_clean_canvas_reports_nothing() {
    fn form(_: &PreviewCtx) -> Built {
        column(vec![
            label("Sign in", 24.0, INK),
            row(
                LayoutStyle::new().flex_row().gap(16.0),
                vec![
                    button(Some("Cancel"), 80.0, 32.0),
                    button(Some("Continue"), 80.0, 32.0),
                ],
            )?,
        ])
    }
    let report = report(form);
    assert!(report.is_clean(), "{report}");
}

#[test]
fn ignoring_a_rule_drops_its_findings_and_keeps_the_rest() {
    fn both(_: &PreviewCtx) -> Built {
        column(vec![
            button(None, 40.0, 40.0),
            label("Fine print", 14.0, GREY),
        ])
    }
    let ignored = report_of(
        entry(both)
            .locale(const { &crate::locale_tag!("en") })
            .a11y_ignore(&[Rule::UnnamedControl]),
    );
    assert_eq!(found(&ignored, Rule::UnnamedControl), 0);
    assert_eq!(found(&ignored, Rule::Contrast), 1);
    assert_eq!(ignored.worst(), Some(Severity::Error));
}

#[test]
fn each_rule_has_a_severity_and_a_name_that_reads_back() {
    for rule in Rule::ALL {
        assert_eq!(Rule::from_id(rule.id()), Some(rule));
    }
    assert_eq!(Rule::Contrast.severity(), Severity::Error);
    assert_eq!(Rule::TargetSize.severity(), Severity::Warning);
    assert!(Severity::Error > Severity::Warning);
}

/// Before and After in a column 80 px apart, with a layer fixed over the page declared between them holding Inside at `inside_top`: the frame draws Inside last, and Tab reaches it where it was declared.
fn layered(inside_top: f32) -> Built {
    let layer = FixedLayer::new(
        LayoutStyle::new()
            .flex_column()
            .padding_top(inside_top)
            .padding_left(crate::preview::host::PAGE_PADDING),
        vec![button(Some("Inside"), 40.0, 40.0)],
    )?;
    Ok(box_item(Container::new(
        LayoutStyle::new().flex_column().gap(80.0),
        vec![
            button(Some("Before"), 40.0, 40.0),
            box_item(layer),
            button(Some("After"), 40.0, 40.0),
        ],
    )?))
}

#[test]
fn focus_order_follows_tab_and_not_the_order_the_frame_draws_in() {
    fn read_in_place(_: &PreviewCtx) -> Built {
        layered(76.0)
    }
    fn read_out_of_place(_: &PreviewCtx) -> Built {
        layered(300.0)
    }
    let play =
        Play::mount(&entry(read_in_place).locale(const { &crate::locale_tag!("en") })).unwrap();
    let frame = play.frame();
    let access = frame
        .access
        .as_deref()
        .expect("a captured frame keeps what Tab walks");
    let tabbed: Vec<&str> = access
        .tab_order
        .iter()
        .map(|stop| {
            let node = access
                .snapshot
                .iter()
                .find(|node| node.id == Some(crate::AccessId::own(stop.id)));
            node.map_or("", |node| node.name.as_str())
        })
        .collect();
    assert_eq!(tabbed, ["Before", "Inside", "After"]);
    let drawn: Vec<&str> = frame
        .texts()
        .filter(|text| ["Before", "Inside", "After"].contains(text))
        .collect();
    assert_eq!(
        drawn,
        ["Before", "After", "Inside"],
        "the layer is drawn over the page"
    );
    let rect_of = |name: &str| {
        access
            .snapshot
            .iter()
            .find(|node| node.name == name)
            .expect("read")
            .rect
    };
    assert!(
        reads_before(rect_of("Inside"), rect_of("After"), Direction::Ltr),
        "walked in draw order, Tab would seem to go back up from After to Inside"
    );
    let in_place = play.check_a11y();
    assert_eq!(found(&in_place, Rule::FocusOrder), 0, "{in_place}");

    let out_of_place = report(read_out_of_place);
    let violations: Vec<&Violation> = out_of_place.of(Rule::FocusOrder).collect();
    assert_eq!(violations.len(), 1, "{out_of_place}");
    assert!(
        violations[0]
            .message
            .contains("from button \"Inside\" to button \"After\"")
    );
}

fn troubled(_: &PreviewCtx) -> Built {
    column(vec![
        box_item(square()),
        button(None, 40.0, 40.0),
        box_item(
            Container::new(LayoutStyle::new(), vec![button(Some("Ghost"), 40.0, 40.0)])?
                .a11y_hidden(),
        ),
        row(
            LayoutStyle::new().flex_row_reverse(),
            vec![
                button(Some("Second"), 40.0, 40.0),
                button(Some("First"), 40.0, 40.0),
            ],
        )?,
    ])
}

#[test]
fn a_captured_frame_is_checked_the_same_once_its_canvas_is_gone() {
    let play = Play::mount(&entry(troubled).locale(const { &crate::locale_tag!("en") })).unwrap();
    let live = play.check_a11y();
    for rule in [
        Rule::UnnamedPicture,
        Rule::UnnamedControl,
        Rule::HiddenFocusable,
        Rule::FocusOrder,
    ] {
        assert_eq!(found(&live, rule), 1, "{rule}: {live}");
    }
    let frame = play.frame();
    drop(play);

    let snapshot = &frame.access.as_deref().expect("captured").snapshot;
    assert_eq!(check(&frame, snapshot), live);
}

#[test]
fn a_frame_made_from_commands_alone_is_read_against_the_surface_entered() {
    let play = Play::mount(&entry(troubled).locale(const { &crate::locale_tag!("en") })).unwrap();
    let captured = play.frame();
    let bare = Frame::new(Arc::clone(&captured.commands), captured.size)
        .with_lang(captured.lang.clone())
        .with_direction(captured.direction);
    assert!(bare.access.is_none());
    let snapshot = &captured.access.as_deref().expect("captured").snapshot;

    play.canvas().with_entered(|| {
        assert_eq!(check(&bare, snapshot), check(&captured, snapshot));
    })
}

#[test]
fn a_pressable_named_only_by_symbols_is_reported() {
    fn close_glyph(_: &PreviewCtx) -> Built {
        column(vec![button(Some("\u{2715}"), 40.0, 40.0)])
    }
    fn ellipsis(_: &PreviewCtx) -> Built {
        column(vec![button(Some("\u{22ef}"), 40.0, 40.0)])
    }
    fn glyph_named_by_the_app(_: &PreviewCtx) -> Built {
        column(vec![
            button(Some("\u{2715}"), 40.0, 40.0).a11y_label(|| "Close"),
        ])
    }
    fn word(_: &PreviewCtx) -> Built {
        column(vec![button(Some("Close"), 40.0, 40.0)])
    }
    fn number(_: &PreviewCtx) -> Built {
        column(vec![button(Some("+1"), 40.0, 40.0)])
    }
    fn icon_only(_: &PreviewCtx) -> Built {
        column(vec![button(None, 40.0, 40.0)])
    }
    assert_eq!(found(&report(close_glyph), Rule::SymbolOnlyName), 1);
    assert_eq!(found(&report(ellipsis), Rule::SymbolOnlyName), 1);
    assert_eq!(
        found(&report(glyph_named_by_the_app), Rule::SymbolOnlyName),
        0
    );
    assert_eq!(found(&report(word), Rule::SymbolOnlyName), 0);
    assert_eq!(found(&report(number), Rule::SymbolOnlyName), 0);
    assert_eq!(found(&report(icon_only), Rule::SymbolOnlyName), 0);
    assert_eq!(
        report(close_glyph)
            .of(Rule::SymbolOnlyName)
            .next()
            .map(|violation| violation.severity),
        Some(Severity::Error)
    );
}

/// 1.25:1 on white: an edge nobody sees.
const FAINT: Color = Color::rgb(230.0 / 255.0, 230.0 / 255.0, 230.0 / 255.0);

fn progress_bar() -> StyledContainer {
    StyledContainer::new(
        LayoutStyle::new().width(80.0).height(8.0),
        |_| RectStyle::default().with_fill(INK),
        vec![],
    )
    .unwrap()
    .role(Role::ProgressBar)
}

fn tab_list() -> StyledContainer {
    let tab = StyledContainer::new(
        LayoutStyle::new().width(80.0).height(32.0),
        |_| RectStyle::default(),
        vec![label("Overview", 12.0, INK)],
    )
    .unwrap()
    .control(Role::Tab)
    .on_press(|| {});
    StyledContainer::new(
        LayoutStyle::new().flex_row(),
        |_| RectStyle::default(),
        vec![box_item(tab)],
    )
    .unwrap()
    .role(Role::TabList)
}

#[test]
fn a_progress_bar_and_a_tab_list_need_a_name() {
    fn unnamed(_: &PreviewCtx) -> Built {
        column(vec![box_item(progress_bar()), box_item(tab_list())])
    }
    fn named(_: &PreviewCtx) -> Built {
        column(vec![
            box_item(progress_bar().a11y_label(|| "Uploading")),
            box_item(tab_list().a11y_label(|| "Sections")),
        ])
    }
    let unnamed = report(unnamed);
    let roles: Vec<&str> = unnamed
        .of(Rule::UnnamedWidget)
        .map(|violation| violation.message.split_whitespace().nth(1).unwrap_or(""))
        .collect();
    assert_eq!(roles, ["progressbar", "tablist"], "{unnamed}");
    assert_eq!(found(&report(named), Rule::UnnamedWidget), 0);
    assert_eq!(Rule::UnnamedWidget.severity(), Severity::Error);
}

fn field(paint: fn() -> RectStyle) -> Box<dyn LayoutItem> {
    box_item(
        StyledContainer::new(
            LayoutStyle::new().width(160.0).height(32.0),
            move |_| paint(),
            vec![],
        )
        .unwrap()
        .control(Role::TextInput)
        .a11y_label(|| "Name"),
    )
}

#[test]
fn a_field_needs_an_edge_or_a_fill_that_stands_out_from_the_page() {
    fn faint_edge(_: &PreviewCtx) -> Built {
        column(vec![field(|| {
            RectStyle::default().with_border(Border::uniform(FAINT, 1.0))
        })])
    }
    fn no_edge(_: &PreviewCtx) -> Built {
        column(vec![field(RectStyle::default)])
    }
    fn dark_edge(_: &PreviewCtx) -> Built {
        column(vec![field(|| {
            RectStyle::default().with_border(Border::uniform(INK, 1.0))
        })])
    }
    fn dark_fill(_: &PreviewCtx) -> Built {
        column(vec![field(|| RectStyle::default().with_fill(INK))])
    }
    let faint = report(faint_edge);
    let violations: Vec<&Violation> = faint.of(Rule::BoundaryContrast).collect();
    assert_eq!(violations.len(), 1, "{faint}");
    assert!(violations[0].message.contains("1.25:1"), "{faint}");
    assert_eq!(found(&report(no_edge), Rule::BoundaryContrast), 1);
    assert_eq!(found(&report(dark_edge), Rule::BoundaryContrast), 0);
    assert_eq!(found(&report(dark_fill), Rule::BoundaryContrast), 0);
}

fn framed(edge: Color) -> Built {
    let frame = StyledContainer::new(
        LayoutStyle::new().padding_all(8.0),
        move |_| RectStyle::default().with_border(Border::uniform(edge, 1.0)),
        vec![field(RectStyle::default)],
    )?;
    column(vec![box_item(frame)])
}

#[test]
fn a_field_that_draws_nothing_itself_is_judged_by_the_frame_around_it() {
    fn faint(_: &PreviewCtx) -> Built {
        framed(FAINT)
    }
    fn dark(_: &PreviewCtx) -> Built {
        framed(INK)
    }
    assert_eq!(found(&report(faint), Rule::BoundaryContrast), 1);
    assert_eq!(found(&report(dark), Rule::BoundaryContrast), 0);
}

fn ringed(ring: fn(Rect) -> RectStyle) -> Box<dyn LayoutItem> {
    box_item(
        StyledContainer::new(
            LayoutStyle::new().width(80.0).height(32.0),
            |_| RectStyle::default().with_border(Border::uniform(INK, 1.0)),
            vec![label("Save", 12.0, INK)],
        )
        .unwrap()
        .control(Role::Button)
        .on_press(|| {})
        .focus_style(ring),
    )
}

#[test]
fn a_stop_has_to_show_where_the_keyboard_is() {
    fn unchanged(_: &PreviewCtx) -> Built {
        column(vec![ringed(|_| {
            RectStyle::default().with_border(Border::uniform(INK, 1.0))
        })])
    }
    fn faint_ring(_: &PreviewCtx) -> Built {
        column(vec![ringed(|_| {
            RectStyle::default().with_border(Border::uniform(FAINT, 2.0))
        })])
    }
    fn dark_ring(_: &PreviewCtx) -> Built {
        column(vec![ringed(|_| {
            RectStyle::default().with_border(Border::uniform(INK, 3.0))
        })])
    }
    fn theme_ring(_: &PreviewCtx) -> Built {
        column(vec![button(Some("Save"), 80.0, 32.0)])
    }
    let unchanged = report(unchanged);
    let violations: Vec<&Violation> = unchanged.of(Rule::FocusIndicator).collect();
    assert_eq!(violations.len(), 1, "{unchanged}");
    assert!(
        violations[0].message.contains("looks the same"),
        "{unchanged}"
    );
    let faint = report(faint_ring);
    let violations: Vec<&Violation> = faint.of(Rule::FocusIndicator).collect();
    assert_eq!(violations.len(), 1, "{faint}");
    assert!(violations[0].message.contains("1.25:1"), "{faint}");
    let dark = report(dark_ring);
    assert_eq!(found(&dark, Rule::FocusIndicator), 0, "{dark}");
    assert_eq!(found(&report(theme_ring), Rule::FocusIndicator), 0);
}

#[test]
fn checking_focus_leaves_it_where_the_play_put_it() {
    fn pair(_: &PreviewCtx) -> Built {
        column(vec![
            button(Some("One"), 80.0, 32.0),
            button(Some("Two"), 80.0, 32.0),
        ])
    }
    let mut play = Play::mount(&entry(pair).locale(const { &crate::locale_tag!("en") })).unwrap();
    play.tab().unwrap();
    play.tab().unwrap();
    let before = play.frame();
    let _ = play.check_a11y();
    let focused = |frame: &Frame| -> Vec<String> {
        frame
            .access
            .as_deref()
            .expect("captured")
            .snapshot
            .iter()
            .filter(|node| node.focused)
            .map(|node| node.name.clone())
            .collect()
    };
    assert_eq!(focused(&before), ["Two"]);
    assert_eq!(focused(&play.frame()), ["Two"]);
    assert_eq!(&*play.frame().commands, &*before.commands);
}

/// A path drawn under a word: a triangle covering the word's top-left corner of the box, or the opposite triangle, whose bounds still cover the word.
fn over_triangle(covering: bool) -> Built {
    let corners = match covering {
        true => [
            Point::new(0.0, 0.0),
            Point::new(200.0, 0.0),
            Point::new(0.0, 40.0),
        ],
        false => [
            Point::new(200.0, 0.0),
            Point::new(200.0, 40.0),
            Point::new(0.0, 40.0),
        ],
    };
    let data = Arc::new(PathData::polygon(&corners));
    let triangle = crate::Path::new(LayoutStyle::new().absolute_fill(), move |_| {
        (Arc::clone(&data), PathStyle::default().with_fill(INK))
    })?
    .a11y_hidden();
    column(vec![box_item(Container::new(
        LayoutStyle::new().width(200.0).height(40.0),
        vec![box_item(triangle), label("Inverted", 14.0, PAPER)],
    )?)])
}

#[test]
fn text_over_a_filled_path_is_measured_against_the_path_where_its_shape_covers_the_text() {
    fn covered(_: &PreviewCtx) -> Built {
        over_triangle(true)
    }
    fn beside(_: &PreviewCtx) -> Built {
        over_triangle(false)
    }
    assert_eq!(found(&report(covered), Rule::Contrast), 0);
    let beside = report(beside);
    let violations: Vec<&Violation> = beside.of(Rule::Contrast).collect();
    assert_eq!(
        violations.len(),
        1,
        "white beside the triangle is on the white page: {beside}"
    );
    assert!(violations[0].message.contains("1.00:1"), "{beside}");
}

#[test]
fn findings_of_the_frame_and_of_focus_read_in_the_order_of_the_rules() {
    fn both(_: &PreviewCtx) -> Built {
        column(vec![
            ringed(|_| RectStyle::default().with_border(Border::uniform(INK, 1.0))),
            label("Fine print", 14.0, GREY),
            button(None, 40.0, 40.0),
        ])
    }
    let report = report(both);
    let rules: Vec<Rule> = report
        .violations
        .iter()
        .map(|violation| violation.rule)
        .collect();
    assert_eq!(
        rules,
        [Rule::UnnamedControl, Rule::Contrast, Rule::FocusIndicator],
        "{report}"
    );
}

fn saves(_: &PreviewCtx) -> Built {
    column(vec![button(Some("Save"), 80.0, 40.0)])
}

fn presses_then_checks(canvas: &mut Play) -> crate::preview::PlayResult {
    canvas.press(crate::Key::Named(crate::NamedKey::Enter))?;
    canvas.expect_accessible()
}

#[test]
fn a_play_that_checks_itself_after_a_step_audits_once_and_ends() {
    let play_entry = entry(saves)
        .locale(const { &crate::locale_tag!("en") })
        .play(presses_then_checks);
    let mut play = Play::mount(&play_entry).unwrap();
    assert_eq!(play.run(), Ok(()));
    assert_eq!(
        play.steps().len(),
        2,
        "the press and the check, each once: the audit's replay is a copy"
    );
}

#[test]
fn a_play_that_checks_itself_still_fails_on_what_the_audit_finds() {
    let play_entry = entry(troubled)
        .locale(const { &crate::locale_tag!("en") })
        .play(presses_then_checks);
    let mut play = Play::mount(&play_entry).unwrap();
    let error = play.run().expect_err("the canvas has unnamed controls");
    assert!(error.message.contains("not accessible"), "{error}");
}
