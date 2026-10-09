//! The inspector drawer: the app's mounted components as a tree, the selected one's box beneath it, and its bounds marked over the app.

use std::rc::Rc;

use telar::{
    Accessible, AlignItems, Border, Children, Container, Insets, LayoutError, LayoutItem,
    LayoutStyle, Reactive, RectStyle, RenderNode, Role, SegmentNodeInfo, ShapeStyle, Size, Slots,
    StyledContainer, Text, box_item, memo, use_theme_tokens,
};
use telar_components::{
    ButtonProps, SizedPane, SplitDirection, SplitPaneProps, TreeNode, TreeViewProps, button,
    split_pane, tree_view,
};

use super::{Model, banner, mounted_while, stats};
use crate::strings::{
    self, BORDER, CLOSE, COMPONENTS, DEPTH, DETAILS, GAP, INSPECTOR, MARGIN, NODES,
    NOTHING_SELECTED, PADDING, POSITION, RESIZE_DETAILS, RESIZE_INSPECTOR, SIZE,
};
use crate::workbench::{
    WORKBENCH_GRID, use_workbench_tokens, workbench_fill, workbench_mono, workbench_muted,
};

const DRAWER_MIN_WIDTH: f32 = 220.0;
const DRAWER_MAX_FRACTION: f32 = 0.6;
const DETAILS_MIN_HEIGHT: f32 = 96.0;
const DETAILS_MAX_FRACTION: f32 = 0.7;
const LABEL_WIDTH: f32 = 72.0;
const HIGHLIGHT_FILL_ALPHA: f32 = 0.12;
const HIGHLIGHT_STROKE: f32 = 1.5;

/// The selected component's bounds, drawn over the app while the inspector is open. It answers no pointer, so the app keeps every press on it.
pub(super) fn highlight(model: Model) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let marks = telar::Canvas::declaring(LayoutStyle::new().absolute_fill(), move |_, _| {
        let node = model
            .inspector_open
            .get()
            .then(|| model.selected_node())
            .flatten()
            .filter(|node| node.rect.width > 0.0 && node.rect.height > 0.0);
        let Some(node) = node else {
            return RenderNode::group([]);
        };
        let accent = use_theme_tokens().primary();
        RenderNode::rect(
            node.rect,
            RectStyle::default()
                .with_fill(accent.with_alpha(HIGHLIGHT_FILL_ALPHA))
                .with_border(Border::uniform(accent, HIGHLIGHT_STROKE)),
        )
    })?
    .a11y_hidden();
    Ok(box_item(marks))
}

pub(super) fn drawer(model: Model) -> Result<Box<dyn LayoutItem>, LayoutError> {
    mounted_while(
        LayoutStyle::new().absolute_fill().flex_row(),
        move || model.inspector_open.get(),
        move || {
            split_pane(
                SplitPaneProps::props()
                    .size(model.drawer_width)
                    .min(DRAWER_MIN_WIDTH)
                    .max_fraction(DRAWER_MAX_FRACTION)
                    .collapsible(false)
                    .label(Reactive::of(|| strings::text(RESIZE_INSPECTOR)))
                    .build(),
                panes([pane(model)?, uncovered(model)?]),
            )
        },
    )
}

/// Where the drawer leaves the app showing, with the build-error banner at its top and the stats at its bottom. It claims no pointer itself, so a press there is the app's.
fn uncovered(model: Model) -> Result<Box<dyn LayoutItem>, LayoutError> {
    Ok(box_item(Container::new(
        LayoutStyle::new()
            .flex_column()
            .flex_grow(1.0)
            .padding_all(WORKBENCH_GRID),
        vec![
            banner::beside_drawer(model)?,
            spacer()?,
            stats::beside_drawer(model)?,
        ],
    )?))
}

fn pane(model: Model) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let body = split_pane(
        SplitPaneProps::props()
            .direction(SplitDirection::Column)
            .sized(SizedPane::Second)
            .size(model.details_height)
            .min(DETAILS_MIN_HEIGHT)
            .max_fraction(DETAILS_MAX_FRACTION)
            .label(Reactive::of(|| strings::text(RESIZE_DETAILS)))
            .build(),
        panes([components(model)?, details(model)?]),
    )?;
    let pane = StyledContainer::new(
        workbench_fill(),
        |_| {
            let tokens = use_workbench_tokens();
            RectStyle::default()
                .with_fill(tokens.panel_background)
                .with_border(Border::uniform(tokens.border_subtle, 1.0))
        },
        vec![header(model)?, body],
    )?
    .input_opaque()
    .role(Role::Complementary)
    .a11y_label(|| strings::text(INSPECTOR));
    Ok(box_item(pane))
}

fn header(model: Model) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let title = Text::declaring(
        || strings::text(INSPECTOR),
        LayoutStyle::new(),
        |text| text.with_font_weight(600),
    )?;
    let count = Text::declaring(
        move || strings::text_with(NODES, &model.nodes.with(|nodes| nodes.len()).to_string()),
        LayoutStyle::new().flex_grow(1.0),
        workbench_muted,
    )?;
    let close = button(
        ButtonProps::props()
            .label(Reactive::of(|| strings::text(CLOSE)))
            .ghost(true)
            .on_press(Rc::new(move || model.inspector_open.set(false)))
            .build(),
        Children::default(),
    )?;
    let header = Container::new(
        LayoutStyle::new()
            .flex_row()
            .flex_shrink(0.0)
            .align_items(AlignItems::CENTER)
            .gap(WORKBENCH_GRID)
            .padding_horizontal(WORKBENCH_GRID * 1.5)
            .padding_vertical(WORKBENCH_GRID / 2.0),
        vec![box_item(title), box_item(count), close],
    )?;
    Ok(box_item(header))
}

fn components(model: Model) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let items = memo(move || model.nodes.with(|nodes| tree_items(nodes)));
    let tree = tree_view(
        TreeViewProps::props()
            .items(items)
            .selected(model.selected)
            .expanded(model.expanded)
            .label(Reactive::of(|| strings::text(COMPONENTS)))
            .build(),
        Children::default(),
    )?;
    Ok(box_item(Container::new(workbench_fill(), vec![tree])?))
}

/// Nests a pre-order walk by depth. Only what the tree shows goes in, so a walk whose rects moved and whose shape did not leaves the rows alone.
fn tree_items(nodes: &[SegmentNodeInfo]) -> Vec<TreeNode> {
    let mut at = 0;
    let mut roots = Vec::new();
    while at < nodes.len() {
        let depth = nodes[at].depth;
        roots.extend(level(nodes, &mut at, depth));
    }
    roots
}

fn level(nodes: &[SegmentNodeInfo], at: &mut usize, depth: usize) -> Vec<TreeNode> {
    let mut siblings = Vec::new();
    while let Some(node) = nodes.get(*at).filter(|node| node.depth == depth) {
        *at += 1;
        let children = match nodes.get(*at) {
            Some(next) if next.depth > depth => level(nodes, at, next.depth),
            _ => Vec::new(),
        };
        siblings.push(TreeNode::new(node.id.to_string(), node.name).with_children(children));
    }
    siblings
}

fn details(model: Model) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let chosen = mounted_while(
        LayoutStyle::new().flex_column().gap(WORKBENCH_GRID / 2.0),
        move || model.selected_node().is_some(),
        move || box_model(model),
    )?;
    let placeholder = mounted_while(
        LayoutStyle::new(),
        move || model.selected_node().is_none(),
        || {
            Ok(box_item(Text::declaring(
                || strings::text(NOTHING_SELECTED),
                LayoutStyle::new(),
                workbench_muted,
            )?))
        },
    )?;
    let details = StyledContainer::new(
        workbench_fill()
            .gap(WORKBENCH_GRID)
            .padding_all(WORKBENCH_GRID * 1.5),
        |_| RectStyle::default(),
        vec![placeholder, chosen],
    )?
    .role(Role::Group)
    .a11y_label(|| strings::text(DETAILS));
    Ok(box_item(details))
}

fn box_model(model: Model) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let read = move |describe: fn(&SegmentNodeInfo) -> String| {
        move || {
            model
                .selected_node()
                .as_ref()
                .map(describe)
                .unwrap_or_default()
        }
    };
    let name = Text::declaring(
        read(|node| node.name.to_owned()),
        LayoutStyle::new(),
        |text| workbench_mono(text).with_font_weight(600),
    )?;
    let rows = vec![
        box_item(name),
        fact(DEPTH, read(|node| format!("{} · #{}", node.depth, node.id)))?,
        fact(
            POSITION,
            read(|node| format!("{}, {}", number(node.rect.x), number(node.rect.y))),
        )?,
        fact(
            SIZE,
            read(|node| size(Size::new(node.rect.width, node.rect.height))),
        )?,
        fact(PADDING, read(|node| edges(node.padding)))?,
        fact(MARGIN, read(|node| edges(node.margin)))?,
        fact(BORDER, read(|node| edges(node.border)))?,
        fact(GAP, read(|node| size(node.gap)))?,
    ];
    Ok(box_item(Container::new(
        LayoutStyle::new().flex_column().gap(WORKBENCH_GRID / 2.0),
        rows,
    )?))
}

fn fact(
    label: &'static str,
    value: impl Fn() -> String + 'static,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let label = Text::declaring(
        move || strings::text(label),
        LayoutStyle::new().width(LABEL_WIDTH).flex_shrink(0.0),
        workbench_muted,
    )?;
    let value = Text::declaring(value, LayoutStyle::new(), workbench_mono)?;
    Ok(box_item(Container::new(
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .gap(WORKBENCH_GRID),
        vec![box_item(label), box_item(value)],
    )?))
}

fn number(value: f32) -> String {
    if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        format!("{value:.1}")
    }
}

fn size(size: Size) -> String {
    format!("{} × {}", number(size.width), number(size.height))
}

/// Edges in the order a stylesheet writes them, top, right, bottom, left, collapsed to one value when all four agree.
fn edges(insets: Insets) -> String {
    let Insets {
        top,
        right,
        bottom,
        left,
    } = insets;
    if top == right && right == bottom && bottom == left {
        return number(top);
    }
    [top, right, bottom, left].map(number).join(" ")
}

fn panes(items: [Box<dyn LayoutItem>; 2]) -> Children {
    let mut slots = Slots::new();
    for item in items {
        slots.push(None, item);
    }
    Children::from(slots)
}

fn spacer() -> Result<Box<dyn LayoutItem>, LayoutError> {
    Ok(box_item(Container::new(
        LayoutStyle::new().flex_grow(1.0),
        Vec::new(),
    )?))
}
