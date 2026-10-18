//! The component inspector: what is mounted, as a tree, over the selected component's box.

use telar::{
    Accessible, AlignItems, Container, Insets, LayoutError, LayoutItem, LayoutStyle, Reactive,
    ReactiveList, Rect, Role, RwSignal, Size, Text, box_item, children, memo, signal, track_layout,
};
use telar_components::{
    SizedPane, SplitDirection, SplitPaneProps, TreeNode, TreeViewProps, split_pane, tree_view,
};

use super::tree_menu::TreeMenu;
use super::{count_mounted, mounted_while, number, panes};
use crate::inspect::{InspectedNode, Inspection};
use crate::strings::{
    self, BORDER, COMPONENTS, DEPTH, DETAILS, GAP, MARGIN, NOTHING_SELECTED, PADDING, POSITION,
    RESIZE_DETAILS, SIZE,
};
use crate::workbench::{WORKBENCH_GRID, workbench_fill, workbench_mono, workbench_muted};

const DETAILS_MIN_HEIGHT: f32 = 96.0;
const DETAILS_MIN_WIDTH: f32 = 200.0;
const DETAILS_MAX_FRACTION: f32 = 0.7;
const LABEL_WIDTH: f32 = 72.0;

/// The components of what `inspection` reads, nested as they are mounted, over the selected one's details: its name, depth, position, size and box model.
pub fn inspector_panel(inspection: Inspection) -> Result<Box<dyn LayoutItem>, LayoutError> {
    count_mounted(inspection.inspecting);
    let laid = signal(None::<RwSignal<Rect>>);
    let panel = ReactiveList::with_style(
        workbench_fill(),
        move || vec![facing(laid.get().map(|rect| rect.get()))],
        |direction: &SplitDirection| *direction,
        move |direction| split(inspection, direction),
    )?;
    laid.set(track_layout(panel.layout_node()));
    Ok(box_item(panel))
}

/// Beside the tree in a panel wider than it is tall, and under it in one that is not: the details are a short column, so put across a wide panel they leave most of it empty.
fn facing(panel: Option<Rect>) -> SplitDirection {
    match panel {
        Some(panel) if panel.width > panel.height => SplitDirection::Row,
        _ => SplitDirection::Column,
    }
}

fn split(
    inspection: Inspection,
    direction: SplitDirection,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let (size, min) = match direction {
        SplitDirection::Row => (inspection.details_width, DETAILS_MIN_WIDTH),
        SplitDirection::Column => (inspection.details_height, DETAILS_MIN_HEIGHT),
    };
    split_pane(
        SplitPaneProps::props()
            .direction(direction)
            .sized(SizedPane::Second)
            .size(size)
            .min(min)
            .max_fraction(DETAILS_MAX_FRACTION)
            .label(Reactive::of(|| strings::text(RESIZE_DETAILS)))
            .build(),
        panes([components(inspection)?, details(inspection)?]),
    )
}

fn components(inspection: Inspection) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let items = memo(move || inspection.nodes.with(|nodes| tree_items(nodes)));
    let menu = TreeMenu::new(items, inspection.expanded);
    let tree = tree_view(
        TreeViewProps::props()
            .items(items)
            .selected(inspection.selected)
            .expanded(inspection.expanded)
            .on_context(menu.on_context())
            .label(Reactive::of(|| strings::text(COMPONENTS)))
            .build(),
        telar::Children::default(),
    )?;
    menu.around(tree)
}

/// Nests a pre-order walk by depth. Only what the tree shows goes in, so a walk whose rects moved and whose shape did not leaves the rows alone.
pub(crate) fn tree_items(nodes: &[InspectedNode]) -> Vec<TreeNode> {
    let mut at = 0;
    let mut roots = Vec::new();
    while at < nodes.len() {
        let depth = nodes[at].depth;
        roots.extend(level(nodes, &mut at, depth));
    }
    roots
}

fn level(nodes: &[InspectedNode], at: &mut usize, depth: usize) -> Vec<TreeNode> {
    let mut siblings = Vec::new();
    while let Some(node) = nodes.get(*at).filter(|node| node.depth == depth) {
        *at += 1;
        let children = match nodes.get(*at) {
            Some(next) if next.depth > depth => level(nodes, at, next.depth),
            _ => Vec::new(),
        };
        siblings
            .push(TreeNode::new(node.id.to_string(), node.name.clone()).with_children(children));
    }
    siblings
}

fn details(inspection: Inspection) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let chosen = mounted_while(
        LayoutStyle::column().gap(WORKBENCH_GRID / 2.0),
        move || inspection.selected_node().is_some(),
        move || box_model(inspection),
    )?;
    let placeholder = mounted_while(
        LayoutStyle::new(),
        move || inspection.selected_node().is_none(),
        || {
            Ok(box_item(Text::declaring(
                || strings::text(NOTHING_SELECTED),
                LayoutStyle::new(),
                workbench_muted,
            )?))
        },
    )?;
    let details = Container::new(
        workbench_fill()
            .gap(WORKBENCH_GRID)
            .padding_all(WORKBENCH_GRID * 1.5),
        vec![placeholder, chosen],
    )?
    .role(Role::Group)
    .a11y_label(|| strings::text(DETAILS));
    Ok(box_item(details))
}

fn box_model(inspection: Inspection) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let read = move |describe: fn(&InspectedNode) -> String| {
        move || {
            inspection
                .selected_node()
                .as_ref()
                .map(describe)
                .unwrap_or_default()
        }
    };
    let name = Text::declaring(
        read(|node| node.name.to_string()),
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
        LayoutStyle::column().gap(WORKBENCH_GRID / 2.0),
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
        LayoutStyle::row()
            .align_items(AlignItems::CENTER)
            .gap(WORKBENCH_GRID),
        children![label, value],
    )?))
}

pub(crate) fn size(size: Size) -> String {
    format!("{} × {}", number(size.width), number(size.height))
}

/// Edges in the order a stylesheet writes them, top, right, bottom, left, collapsed to one value when all four agree.
pub(crate) fn edges(insets: Insets) -> String {
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
