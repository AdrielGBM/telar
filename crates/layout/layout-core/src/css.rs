//! A layout style, as the CSS that means the same thing.
//!
//! Not a translation between two vocabularies: `taffy::Style` **is** CSS's box model, field for field, and
//! this only writes it down. What makes that worth having is the target where the browser is the layout
//! engine — there, this is the whole of what Telar tells it, and Taffy stays as the oracle a test compares
//! against rather than the thing that positions anything.

use taffy::{
    AlignContentKeyword, AlignItemsKeyword, CompactLength, Dimension, Display, FlexDirection,
    FlexWrap, LengthPercentage, LengthPercentageAuto, Position, Style,
};

use crate::direction::Direction;
use crate::style::LayoutStyle;

/// Declarations, in the order a stylesheet would read best. Kept as one string rather than a map because
/// what consumes it writes one `style` attribute: a per-property call into the DOM for each of twenty
/// declarations costs more than the string it avoids.
#[derive(Default, Debug, PartialEq, Eq, Clone)]
pub struct Css(String);

impl Css {
    fn push(&mut self, property: &str, value: &str) {
        self.0.push_str(property);
        self.0.push(':');
        self.0.push_str(value);
        self.0.push(';');
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

impl std::fmt::Display for Css {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl LayoutStyle {
    /// This style as CSS declarations, resolved for `direction`.
    ///
    /// Resolved rather than logical: the same `resolve` every layout pass runs, so what a browser is told
    /// and what Taffy computed came from one function and cannot drift apart in an RTL locale.
    pub fn to_css(&self, direction: Direction) -> Css {
        css_of(&self.resolve(direction))
    }
}

fn css_of(style: &Style) -> Css {
    let mut css = Css::default();

    css.push("display", display_of(style.display));
    if style.display == Display::None {
        // Nothing else about a box that is not in the flow can be observed, and saying it anyway is twenty
        // declarations the browser parses to reach the same nothing.
        return css;
    }

    // Taffy's `size` is the border box, padding and border inside it; CSS's default `content-box` adds them
    // around it instead. Left out, a `pad:20` box came out 40px wider than Taffy said, its children were
    // measured against the wrong width, and every box below it in the column drifted further down the page
    // than the rects hit-testing reads — an interface that draws correctly and cannot be clicked.
    css.push("box-sizing", "border-box");

    if style.position == Position::Absolute {
        css.push("position", "absolute");
    }
    for (property, value) in [
        ("top", style.inset.top),
        ("right", style.inset.right),
        ("bottom", style.inset.bottom),
        ("left", style.inset.left),
    ] {
        if let Some(value) = auto_length(value) {
            css.push(property, &value);
        }
    }

    if style.display == Display::Flex {
        css.push("flex-direction", flex_direction_of(style.flex_direction));
        if style.flex_wrap == FlexWrap::Wrap {
            css.push("flex-wrap", "wrap");
        } else if style.flex_wrap == FlexWrap::WrapReverse {
            css.push("flex-wrap", "wrap-reverse");
        }
    }

    for (property, value) in [
        ("width", style.size.width),
        ("height", style.size.height),
        ("min-width", style.min_size.width),
        ("min-height", style.min_size.height),
        ("max-width", style.max_size.width),
        ("max-height", style.max_size.height),
    ] {
        if let Some(value) = dimension(value) {
            css.push(property, &value);
        }
    }
    if let Some(ratio) = style.aspect_ratio {
        css.push("aspect-ratio", &format_number(ratio));
    }

    if let Some(padding) = edges(
        style.padding.top,
        style.padding.right,
        style.padding.bottom,
        style.padding.left,
    ) {
        css.push("padding", &padding);
    }
    if let Some(margin) = auto_edges(
        style.margin.top,
        style.margin.right,
        style.margin.bottom,
        style.margin.left,
    ) {
        css.push("margin", &margin);
    }
    if let Some(gap) = gap_of(style.gap.height, style.gap.width) {
        css.push("gap", &gap);
    }

    if style.flex_grow != 0.0 {
        css.push("flex-grow", &format_number(style.flex_grow));
    }
    if style.flex_shrink != 1.0 {
        css.push("flex-shrink", &format_number(style.flex_shrink));
    }
    if let Some(basis) = dimension(style.flex_basis) {
        css.push("flex-basis", &basis);
    }

    if let Some(align) = style.align_items.map(|a| align_items_of(a.keyword)) {
        css.push("align-items", align);
    }
    if let Some(align) = style.align_self.map(|a| align_items_of(a.keyword)) {
        css.push("align-self", align);
    }
    if let Some(justify) = style.justify_content.map(|j| align_content_of(j.keyword)) {
        css.push("justify-content", justify);
    }
    if let Some(align) = style.align_content.map(|a| align_content_of(a.keyword)) {
        css.push("align-content", align);
    }

    if style.display == Display::Grid {
        if let Some(tracks) = template(&style.grid_template_columns) {
            css.push("grid-template-columns", &tracks);
        }
        if let Some(tracks) = template(&style.grid_template_rows) {
            css.push("grid-template-rows", &tracks);
        }
    }
    if let Some(span) = placement_span(&style.grid_column) {
        css.push("grid-column", &span);
    }
    if let Some(span) = placement_span(&style.grid_row) {
        css.push("grid-row", &span);
    }

    css
}

/// A track list, or nothing when there are no tracks to describe.
fn template(tracks: &[taffy::GridTemplateComponent<String>]) -> Option<String> {
    if tracks.is_empty() {
        return None;
    }
    let written: Vec<String> = tracks.iter().map(component).collect();
    Some(written.join(" "))
}

fn component(track: &taffy::GridTemplateComponent<String>) -> String {
    match track {
        taffy::GridTemplateComponent::Single(sizing) => sizing_of(*sizing),
        taffy::GridTemplateComponent::Repeat(repetition) => {
            let count = match repetition.count {
                taffy::RepetitionCount::AutoFill => "auto-fill".to_string(),
                taffy::RepetitionCount::AutoFit => "auto-fit".to_string(),
                taffy::RepetitionCount::Count(n) => n.to_string(),
            };
            let tracks: Vec<String> = repetition.tracks.iter().copied().map(sizing_of).collect();
            format!("repeat({count},{})", tracks.join(" "))
        }
    }
}

/// One track's size. A track whose min and max agree is written once, as CSS does.
fn sizing_of(sizing: taffy::TrackSizingFunction) -> String {
    let min_raw = sizing.min.into_raw();
    let min = track_length(min_raw);
    let max = track_length(sizing.max.into_raw());
    // A flexible track is written as the bare `fr`. `1fr` *is* `minmax(auto, 1fr)` in CSS, so the long form
    // would be right and unidiomatic — and this string is compared every frame, so shorter is also cheaper.
    if min_raw.tag() == CompactLength::AUTO_TAG
        && sizing.max.into_raw().tag() == CompactLength::FR_TAG
        && let Some(max) = max.clone()
    {
        return max;
    }
    match (min, max) {
        (Some(min), Some(max)) if min == max => min,
        (Some(min), Some(max)) => format!("minmax({min},{max})"),
        (None, Some(max)) => max,
        (Some(min), None) => min,
        (None, None) => "auto".to_string(),
    }
}

/// A track length, which has three spellings CSS has and a plain length does not: `fr`, the content
/// keywords, and `fit-content`.
fn track_length(compact: CompactLength) -> Option<String> {
    match compact.tag() {
        CompactLength::FR_TAG => Some(format!("{}fr", format_number(compact.value()))),
        CompactLength::MIN_CONTENT_TAG => Some("min-content".to_string()),
        CompactLength::MAX_CONTENT_TAG => Some("max-content".to_string()),
        CompactLength::FIT_CONTENT_PX_TAG => {
            Some(format!("fit-content({}px)", format_number(compact.value())))
        }
        CompactLength::FIT_CONTENT_PERCENT_TAG => Some(format!(
            "fit-content({}%)",
            format_number(compact.value() * 100.0)
        )),
        _ => length_of(compact),
    }
}

/// `span N`, which is the only placement this vocabulary can express.
fn placement_span(line: &taffy::Line<taffy::GridPlacement<String>>) -> Option<String> {
    match line.start {
        taffy::GridPlacement::Span(n) if n > 1 => Some(format!("span {n}")),
        _ => None,
    }
}

fn display_of(display: Display) -> &'static str {
    match display {
        Display::Block => "block",
        Display::Flex => "flex",
        Display::Grid => "grid",
        Display::FlowRoot => "flow-root",
        Display::None => "none",
    }
}

fn flex_direction_of(direction: FlexDirection) -> &'static str {
    match direction {
        FlexDirection::Row => "row",
        FlexDirection::Column => "column",
        FlexDirection::RowReverse => "row-reverse",
        FlexDirection::ColumnReverse => "column-reverse",
    }
}

fn align_items_of(keyword: AlignItemsKeyword) -> &'static str {
    match keyword {
        AlignItemsKeyword::Start => "start",
        AlignItemsKeyword::End => "end",
        AlignItemsKeyword::FlexStart => "flex-start",
        AlignItemsKeyword::FlexEnd => "flex-end",
        AlignItemsKeyword::SelfStart => "self-start",
        AlignItemsKeyword::SelfEnd => "self-end",
        AlignItemsKeyword::Center => "center",
        AlignItemsKeyword::Baseline => "baseline",
        AlignItemsKeyword::Stretch => "stretch",
    }
}

fn align_content_of(keyword: AlignContentKeyword) -> &'static str {
    match keyword {
        AlignContentKeyword::Start => "start",
        AlignContentKeyword::End => "end",
        AlignContentKeyword::FlexStart => "flex-start",
        AlignContentKeyword::FlexEnd => "flex-end",
        AlignContentKeyword::Center => "center",
        AlignContentKeyword::Stretch => "stretch",
        AlignContentKeyword::SpaceBetween => "space-between",
        AlignContentKeyword::SpaceEvenly => "space-evenly",
        AlignContentKeyword::SpaceAround => "space-around",
    }
}

/// A number without a trailing `.0`, because CSS reads better and the string is compared every frame.
fn format_number(value: f32) -> String {
    if value.fract() == 0.0 {
        format!("{}", value as i64)
    } else {
        format!("{value}")
    }
}

fn length_of(compact: CompactLength) -> Option<String> {
    match compact.tag() {
        CompactLength::LENGTH_TAG => Some(format!("{}px", format_number(compact.value()))),
        CompactLength::PERCENT_TAG => Some(format!("{}%", format_number(compact.value() * 100.0))),
        CompactLength::AUTO_TAG => Some("auto".to_string()),
        _ => None,
    }
}

/// A size, or nothing when it is `auto` — which is what CSS starts every one of these at, and what
/// `max-width` does not even accept as a value.
fn dimension(value: Dimension) -> Option<String> {
    let compact = value.into_raw();
    if compact.tag() == CompactLength::AUTO_TAG {
        return None;
    }
    length_of(compact)
}

/// A padding-style edge: no `auto`, and zero is the initial value, so it is left unsaid.
fn edge(value: LengthPercentage) -> Option<String> {
    let compact = value.into_raw();
    if compact.tag() == CompactLength::LENGTH_TAG && compact.value() == 0.0 {
        return None;
    }
    length_of(compact)
}

fn auto_edge(value: LengthPercentageAuto) -> Option<String> {
    let compact = value.into_raw();
    if compact.tag() == CompactLength::LENGTH_TAG && compact.value() == 0.0 {
        return None;
    }
    length_of(compact)
}

fn auto_length(value: LengthPercentageAuto) -> Option<String> {
    let compact = value.into_raw();
    if compact.tag() == CompactLength::AUTO_TAG {
        return None;
    }
    length_of(compact)
}

/// The four-value shorthand, or nothing when every edge is zero.
fn edges(
    top: LengthPercentage,
    right: LengthPercentage,
    bottom: LengthPercentage,
    left: LengthPercentage,
) -> Option<String> {
    let sides = [edge(top), edge(right), edge(bottom), edge(left)];
    shorthand(sides)
}

fn auto_edges(
    top: LengthPercentageAuto,
    right: LengthPercentageAuto,
    bottom: LengthPercentageAuto,
    left: LengthPercentageAuto,
) -> Option<String> {
    let sides = [
        auto_edge(top),
        auto_edge(right),
        auto_edge(bottom),
        auto_edge(left),
    ];
    shorthand(sides)
}

fn shorthand(sides: [Option<String>; 4]) -> Option<String> {
    if sides.iter().all(Option::is_none) {
        return None;
    }
    let [top, right, bottom, left] = sides;
    let value = |side: Option<String>| side.unwrap_or_else(|| "0".to_string());
    let (top, right, bottom, left) = (value(top), value(right), value(bottom), value(left));
    Some(if top == bottom && right == left {
        if top == right {
            top
        } else {
            format!("{top} {right}")
        }
    } else {
        format!("{top} {right} {bottom} {left}")
    })
}

/// `gap` takes row then column, which is the opposite order from how a `Size` names them.
fn gap_of(row: LengthPercentage, column: LengthPercentage) -> Option<String> {
    let (row, column) = (edge(row), edge(column));
    match (row, column) {
        (None, None) => None,
        (row, column) => {
            let row = row.unwrap_or_else(|| "0".to_string());
            let column = column.unwrap_or_else(|| "0".to_string());
            Some(if row == column {
                row
            } else {
                format!("{row} {column}")
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::{AlignItems, JustifyContent, SizeDimension};

    fn css(style: LayoutStyle) -> String {
        style.to_css(Direction::Ltr).into_string()
    }

    /// Every property CSS already starts where taffy does is left unsaid, so what reaches the browser is
    /// what the app actually asked for.
    #[test]
    fn a_plain_style_says_only_what_it_is() {
        assert_eq!(
            css(LayoutStyle::new()),
            "display:block;box-sizing:border-box;"
        );
    }

    #[test]
    fn a_row_becomes_a_flex_row() {
        assert!(
            css(LayoutStyle::new().flex_row())
                .starts_with("display:flex;box-sizing:border-box;flex-direction:row;")
        );
    }

    #[test]
    fn a_row_reverses_under_rtl() {
        let style = LayoutStyle::new().flex_row();
        assert!(
            style
                .to_css(Direction::Rtl)
                .as_str()
                .contains("flex-direction:row-reverse"),
            "got {}",
            style.to_css(Direction::Rtl)
        );
    }

    #[test]
    fn sizes_carry_their_unit() {
        let out = css(LayoutStyle::new()
            .width(SizeDimension::Px(300.0))
            .height(SizeDimension::Percent(0.5)));
        assert!(out.contains("width:300px;"), "got {out}");
        assert!(out.contains("height:50%;"), "got {out}");
    }

    #[test]
    fn equal_edges_collapse_to_one_value() {
        let out = css(LayoutStyle::new().padding_all(24.0));
        assert!(out.contains("padding:24px;"), "got {out}");
    }

    #[test]
    fn a_vertical_and_horizontal_pair_collapses_to_two() {
        let out = css(LayoutStyle::new()
            .padding_vertical(8.0)
            .padding_horizontal(16.0));
        assert!(out.contains("padding:8px 16px;"), "got {out}");
    }

    #[test]
    fn four_different_edges_stay_four() {
        let out = css(LayoutStyle::new()
            .padding_top(1.0)
            .padding_right(2.0)
            .padding_bottom(3.0)
            .padding_left(4.0));
        assert!(out.contains("padding:1px 2px 3px 4px;"), "got {out}");
    }

    #[test]
    fn zero_padding_is_left_unsaid() {
        assert!(!css(LayoutStyle::new().padding_all(0.0)).contains("padding"));
    }

    #[test]
    fn a_gap_is_row_then_column() {
        let out = css(LayoutStyle::new().flex_row().gap_y(4.0).gap_x(12.0));
        assert!(out.contains("gap:4px 12px;"), "got {out}");
    }

    #[test]
    fn a_shrink_that_is_not_the_default_is_stated() {
        assert!(css(LayoutStyle::new().flex_shrink(0.0)).contains("flex-shrink:0;"));
        assert!(!css(LayoutStyle::new()).contains("flex-shrink"));
    }

    #[test]
    fn a_grown_item_says_so() {
        assert!(css(LayoutStyle::new().flex_grow(1.0)).contains("flex-grow:1;"));
    }

    #[test]
    fn a_hidden_box_says_nothing_else() {
        assert_eq!(
            css(LayoutStyle::new()
                .flex_row()
                .padding_all(8.0)
                .display_none()),
            "display:none;"
        );
    }

    #[test]
    fn an_absolute_fill_pins_every_edge() {
        let out = css(LayoutStyle::new().absolute_fill());
        assert!(out.contains("position:absolute;"), "got {out}");
        for edge in ["top:0px", "right:0px", "bottom:0px", "left:0px"] {
            assert!(out.contains(edge), "{edge} missing from {out}");
        }
    }

    #[test]
    fn an_unpinned_absolute_box_leaves_its_edges_alone() {
        let out = css(LayoutStyle::new().absolute());
        assert!(out.contains("position:absolute;"), "got {out}");
        assert!(
            !out.contains("top:"),
            "an auto inset is not a declaration: {out}"
        );
    }

    #[test]
    fn alignment_keeps_its_css_spelling() {
        let out = css(LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::SPACE_BETWEEN));
        assert!(out.contains("align-items:center;"), "got {out}");
        assert!(out.contains("justify-content:space-between;"), "got {out}");
    }

    #[test]
    fn an_aspect_ratio_needs_no_unit() {
        assert!(css(LayoutStyle::new().aspect_ratio(1.5)).contains("aspect-ratio:1.5;"));
    }

    /// A logical edge has to land on the physical one the direction chose, and land on the *same* one Taffy
    /// resolved it to — the whole point of going through `resolve` rather than reading the logical fields.
    #[test]
    fn a_logical_edge_follows_the_direction() {
        let style = LayoutStyle::new().padding_start(SizeDimension::Px(20.0));
        assert!(
            style
                .to_css(Direction::Ltr)
                .as_str()
                .contains("padding:0 0 0 20px;"),
            "ltr: {}",
            style.to_css(Direction::Ltr)
        );
        assert!(
            style
                .to_css(Direction::Rtl)
                .as_str()
                .contains("padding:0 20px 0 0;"),
            "rtl: {}",
            style.to_css(Direction::Rtl)
        );
    }
}

#[cfg(test)]
mod grid_tests {
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

    /// What `grid cols:"fit 150"` means, and the case that made the sandbox stack: a grid with no track
    /// list is one implicit column, so every card came out full width.
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
        assert!(css(LayoutStyle::new().grid_column_span(2)).contains("grid-column:span 2;"));
        assert!(!css(LayoutStyle::new().grid_column_span(1)).contains("grid-column"));
    }

    /// A track list only means anything on a grid, and saying it elsewhere is noise the browser parses.
    #[test]
    fn a_flex_box_does_not_describe_tracks() {
        let out = css(LayoutStyle::new()
            .flex_row()
            .grid_template_columns(vec![TemplateTrack::fr(1.0)]));
        assert!(!out.contains("grid-template-columns"), "got {out}");
    }
}
