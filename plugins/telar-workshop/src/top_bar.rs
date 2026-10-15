//! The top bar, which is also the title bar of a window the workshop frames itself: the sidebar's opener, the "Preview" label and the preview settings at the start, the trigger of the command palette in the middle, and the view switch, then the window's own controls, at the end.
//!
//! The trigger is centred on the window rather than between its neighbours: the two ends split the rest of the bar evenly whatever each holds. A window too narrow for that sets it between the two groups instead, as far from one as from the other, rather than letting the group that needs more push it towards the other and leave a gap on one side only.

use std::rc::Rc;

use telar::{
    Accessible, AlignItems, Border, BorderRadius, Children, Clip, ClippedItem, Container,
    JustifyContent, LayoutError, LayoutItem, LayoutStyle, Memo, NodeId, Reactive, ReactiveList,
    Rect, RectStyle, Role, RwSignal, ShapeStyle, Slots, StyledContainer, Text, TextWrap, box_item,
    memo, signal, style_follows, track_layout,
};
use telar_components::metrics::{control_height, control_pad_x, radius_control};
use telar_components::{
    IconButtonProps, TitleBarOptions, TooltipProps, WindowControlsProps, icon_button, title_bar,
    tooltip,
};
use telar_devtools::{WORKBENCH_GRID, WORKBENCH_RADIUS, use_workbench_tokens, workbench_muted};
use telar_icons::{IconProps, icon};

use crate::arrangement::{FIND_FIELD_COMFORT, FindPlacement};
use crate::keymap::{Action, Keymap};
use crate::state::WorkshopState;
use crate::strings::{self, FIND, FIND_IN, PREVIEW, PREVIEWS, WORKSHOP};
use crate::{canvas_toolbar, view_switch};

const HEIGHT: f32 = 40.0;
const GAP: f32 = WORKBENCH_GRID * 2.0;
/// The widest the palette's trigger grows, in a window with room to spare.
pub(crate) const FIND_WIDTH: f32 = WORKBENCH_GRID * 60.0;

/// Who frames the window the workshop is shown in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Frame {
    /// A desktop window without decorations, framed by the workshop: the top bar moves and maximizes it and holds its controls, and its borders resize it.
    Own,
    /// A browser page or a phone's screen, framed by whatever it is shown in.
    Host,
}

impl Frame {
    pub(crate) const CURRENT: Self = if cfg!(any(target_arch = "wasm32", target_os = "android")) {
        Self::Host
    } else {
        Self::Own
    };
}

pub(crate) fn top_bar(
    state: &WorkshopState,
    keymap: &Keymap,
    project_name: Option<String>,
    frame: Frame,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let project: Rc<dyn Fn() -> String> = Rc::new(move || {
        project_name
            .clone()
            .unwrap_or_else(|| strings::text(WORKSHOP))
    });
    let placement = FindPlacement::follow();
    let start = start(state)?;
    let end = end(state, keymap, &project, placement, frame)?;
    let laid_out = signal(None::<RwSignal<Rect>>);
    let centred = centring(start.layout_node(), end.layout_node(), laid_out);
    let bar = StyledContainer::new(
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .flex_shrink(0.0)
            .height(HEIGHT)
            .padding_horizontal(WORKBENCH_GRID)
            .gap(GAP),
        |_| RectStyle::default(),
        vec![
            side(start, JustifyContent::START, centred)?,
            middle(keymap, &project, placement, centred)?,
            side(end, JustifyContent::END, centred)?,
        ],
    )?
    .role(Role::Banner);
    laid_out.set(track_layout(bar.layout_node()));
    Ok(box_item(match frame {
        Frame::Own => title_bar(
            bar,
            TitleBarOptions {
                replace_decorations: true,
                ..TitleBarOptions::default()
            },
        ),
        Frame::Host => bar,
    }))
}

/// Whether the bar centres the palette's field on itself: only while the wider of its two groups still fits in an even share of what the field leaves.
fn centring(start: NodeId, end: NodeId, bar: RwSignal<Option<RwSignal<Rect>>>) -> Memo<bool> {
    let start = track_layout(start).expect("the start group is laid out");
    let end = track_layout(end).expect("the end group is laid out");
    memo(move || {
        let Some(bar) = bar.get() else {
            return true;
        };
        let room = bar.get().width - 2.0 * WORKBENCH_GRID;
        let widest = start.get().width.max(end.get().width);
        room >= 2.0 * (widest + GAP) + FIND_WIDTH
    })
}

/// Either end of the bar, holding `group`. While the field is centred both ends take the same share of what it leaves; otherwise each is as wide as its group, and the field stands between them.
fn side(
    group: Box<dyn LayoutItem>,
    justify: JustifyContent,
    centred: Memo<bool>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let style = move || {
        let side = LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .justify_content(justify);
        match centred.get() {
            true => side.flex_grow(1.0).flex_shrink(1.0).flex_basis(0.0),
            false => side.flex_shrink(0.0),
        }
    };
    let side = Container::new(style(), vec![group])?.styled_by(style);
    Ok(box_item(side))
}

/// The controls of one end, as wide as they are whatever share of the bar the end has.
fn group() -> LayoutStyle {
    LayoutStyle::new()
        .flex_row()
        .align_items(AlignItems::CENTER)
        .flex_shrink(0.0)
        .gap(GAP)
}

/// A control of the bar, which keeps every stroke started on it rather than letting it move the window.
fn held(control: Box<dyn LayoutItem>) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let held = StyledContainer::new(
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .flex_shrink(0.0),
        |_| RectStyle::default(),
        vec![control],
    )?
    .holds_stroke();
    Ok(box_item(held))
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Start {
    Sidebar,
    Title,
    Settings,
}

/// The sidebar's opener while the sidebar is a drawer, then the bold "Preview" label, then the preview settings.
fn start(state: &WorkshopState) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let arrangement = state.arrangement();
    let state = state.clone();
    let start = ReactiveList::with_style(
        group(),
        move || {
            let opener = arrangement
                .get()
                .sidebar_in_drawer()
                .then_some(Start::Sidebar);
            opener
                .into_iter()
                .chain([Start::Title, Start::Settings])
                .collect()
        },
        |part: &Start| *part,
        move |part| match part {
            Start::Sidebar => held(sidebar_opener(&state)?),
            Start::Title => preview_label(),
            Start::Settings => held(canvas_toolbar::environment_menu(&state)?),
        },
    )?;
    Ok(box_item(start))
}

/// The bar's name for what the workshop shows, in bold, set in a box a control's size and padding so it stands level with the controls beside it and starts where a button's label would.
fn preview_label() -> Result<Box<dyn LayoutItem>, LayoutError> {
    let label = Text::declaring(
        || strings::text(PREVIEW),
        LayoutStyle::new().flex_shrink(0.0),
        |text| text.with_font_weight(600).with_text_wrap(TextWrap::NoWrap),
    )?;
    let boxed = Container::new(
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .flex_shrink(0.0)
            .height(control_height())
            .padding_horizontal(control_pad_x()),
        vec![box_item(label)],
    )?;
    Ok(box_item(boxed))
}

/// Opens the sidebar's drawer, as its shortcut does.
fn sidebar_opener(state: &WorkshopState) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let drawer = state.sidebar_drawer();
    icon_button(
        IconButtonProps::props()
            .icon(telar_icons::icon!("lucide:panel-left"))
            .label(strings::reactive(PREVIEWS))
            .shortcut(Action::ToggleSidebar.chords()[0].to_string())
            .on_press(Rc::new(move || drawer.set(true)))
            .build(),
        Children::default(),
    )
}

/// Drawn as the field the palette searches in, with the project's name in it, and pressed to open the palette, as the shortcut its tooltip names does from anywhere a field is not taking text.
fn find_trigger(
    keymap: &Keymap,
    project: Rc<dyn Fn() -> String>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let search = icon(
        IconProps::props()
            .name(telar_icons::icon!("lucide:search"))
            .size(WORKBENCH_GRID * 2.0)
            .color(Reactive::of(|| use_workbench_tokens().text_muted))
            .build(),
        Children::default(),
    )?;
    let shown = Rc::clone(&project);
    let name = Text::declaring(
        move || shown(),
        LayoutStyle::new().min_width(0.0),
        |text| workbench_muted(text).with_text_wrap(TextWrap::NoWrap),
    )?;
    let keymap = keymap.clone();
    let field = StyledContainer::new(
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::CENTER)
            .flex_grow(1.0)
            .min_width(0.0)
            .height(telar_components::metrics::control_height())
            .padding_horizontal(WORKBENCH_GRID)
            .gap(WORKBENCH_GRID),
        |_| field_paint(false),
        vec![
            search,
            box_item(ClippedItem::new(box_item(name), Clip::x())),
        ],
    )?
    .hover_style(|_| field_paint(true))
    .control(Role::Button)
    .a11y_label(move || strings::text_with(FIND_IN, &project()))
    .on_press(move || keymap.run(Action::Find))
    .holds_stroke();
    let mut slots = Slots::new();
    slots.push(None, box_item(field));
    let hinted = tooltip(
        TooltipProps::props()
            .text(Reactive::of(|| strings::text(FIND)))
            .shortcut(Action::Find.chords()[0].to_string())
            .stretch(true)
            .build(),
        Children::from(slots),
    )?;
    let middle = StyledContainer::new(
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .width(FIND_WIDTH)
            .flex_shrink(1.0)
            .min_width(FIND_FIELD_COMFORT),
        |_| RectStyle::default(),
        vec![hinted],
    )?;
    Ok(box_item(middle))
}

fn field_paint(hovered: bool) -> RectStyle {
    let tokens = use_workbench_tokens();
    let fill = if hovered {
        tokens.canvas_backdrop
    } else {
        tokens.sidebar_background
    };
    RectStyle::default()
        .with_fill(fill)
        .with_border(Border::uniform(tokens.border_subtle, 1.0))
        .with_radius(BorderRadius::all(WORKBENCH_RADIUS))
}

/// The palette's field in the middle of the bar, while the bar has room for one: as wide as the field while the field is centred, and the whole room between the groups, the field in the middle of it, while it is not.
fn middle(
    keymap: &Keymap,
    project: &Rc<dyn Fn() -> String>,
    placement: Memo<FindPlacement>,
    centred: Memo<bool>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let keymap = keymap.clone();
    let project = Rc::clone(project);
    let style = move || {
        let middle = LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::CENTER)
            .flex_shrink(1.0)
            .min_width(0.0);
        match centred.get() {
            true => middle,
            false => middle.flex_grow(1.0).flex_basis(0.0),
        }
    };
    let middle = ReactiveList::with_style(
        style(),
        move || match placement.get() {
            FindPlacement::Field => vec![()],
            FindPlacement::Button => Vec::new(),
        },
        |_: &()| (),
        move |_| find_trigger(&keymap, Rc::clone(&project)),
    )?;
    style_follows(middle.layout_node(), style);
    Ok(box_item(middle))
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum End {
    Find,
    Views,
    Controls,
}

/// The palette's button where the bar has no room for its field, the view switch, then the window's controls where the workshop frames its window.
fn end(
    state: &WorkshopState,
    keymap: &Keymap,
    project: &Rc<dyn Fn() -> String>,
    placement: Memo<FindPlacement>,
    frame: Frame,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let state = state.clone();
    let keymap = keymap.clone();
    let project = Rc::clone(project);
    let end = ReactiveList::with_style(
        group(),
        move || {
            let find = (placement.get() == FindPlacement::Button).then_some(End::Find);
            let controls = (frame == Frame::Own).then_some(End::Controls);
            find.into_iter()
                .chain([End::Views])
                .chain(controls)
                .collect()
        },
        |part: &End| *part,
        move |part| {
            held(match part {
                End::Find => find_button(&keymap, Rc::clone(&project))?,
                End::Views => view_switch::view_switch(&state)?,
                End::Controls => window_controls()?,
            })
        },
    )?;
    Ok(box_item(end))
}

/// The palette's trigger as an icon button, with the name and the shortcut of the field it stands in for.
fn find_button(
    keymap: &Keymap,
    project: Rc<dyn Fn() -> String>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let keymap = keymap.clone();
    icon_button(
        IconButtonProps::props()
            .icon(telar_icons::icon!("lucide:search"))
            .label(Reactive::of(move || {
                strings::text_with(FIND_IN, &project())
            }))
            .shortcut(Action::Find.chords()[0].to_string())
            .on_press(Rc::new(move || keymap.run(Action::Find)))
            .build(),
        Children::default(),
    )
}

/// The window's minimize, maximize and close, as tall as the other controls of the bar and rounded as they are under the pointer.
fn window_controls() -> Result<Box<dyn LayoutItem>, LayoutError> {
    telar_components::window_controls(
        WindowControlsProps::props()
            .min_size(Reactive::of(control_height))
            .style(Rc::new(|paint: RectStyle| {
                paint.with_radius(BorderRadius::all(radius_control()))
            }))
            .build(),
        Children::default(),
    )
}

#[cfg(test)]
#[path = "top_bar_test.rs"]
mod tests;
