//! The FPS badge in the corner, and the panel it opens: the frame rate, the frame time, how many components are mounted, which renderer is drawing, and the chords.

use telar::{
    Accessible, AlignItems, Children, Container, LayoutError, LayoutItem, LayoutStyle, Reactive,
    RectStyle, Role, ShapeStyle, SizeDimension, StyledContainer, Text, box_item, use_theme_tokens,
};
use telar_components::{BadgeProps, KbdProps, badge, kbd};

use super::{Model, mounted_while};
use crate::strings::{
    self, DEV, DEVTOOLS, FPS, FRAME_TIME, NODES, RENDERER, RENDERER_STARTING, SHOW_INSPECTOR,
    SHOW_PANEL, SWITCH_RENDERER, TOGGLE_PANEL,
};
use crate::workbench::{
    WORKBENCH_GRID, use_workbench_tokens, workbench_card, workbench_mono, workbench_muted,
};

const PANEL_WIDTH: f32 = 248.0;
const FPS_SIZE: f32 = 20.0;

/// Over the bottom-right of the app while the inspector is closed; with it open the stats live in the room the drawer leaves ([`beside_drawer`]), so neither covers the other.
pub(super) fn corner(model: Model) -> Result<Box<dyn LayoutItem>, LayoutError> {
    mounted_while(
        LayoutStyle::new()
            .absolute()
            .inset_bottom(WORKBENCH_GRID)
            .inset_end(WORKBENCH_GRID),
        move || !model.inspector_open.get(),
        move || cluster(model),
    )
}

/// The stats as the bottom of the area right of the inspector drawer, where they are laid out with the drawer rather than over it.
pub(super) fn beside_drawer(model: Model) -> Result<Box<dyn LayoutItem>, LayoutError> {
    mounted_while(
        LayoutStyle::new()
            .flex_column()
            .align_items(AlignItems::FLEX_END),
        move || model.inspector_open.get(),
        move || cluster(model),
    )
}

fn cluster(model: Model) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let panel = mounted_while(
        LayoutStyle::new(),
        move || model.panel_open.get(),
        move || panel(model),
    )?;
    let cluster = Container::new(
        LayoutStyle::new()
            .flex_column()
            .align_items(AlignItems::FLEX_END)
            .max_width(SizeDimension::Percent(1.0))
            .gap(WORKBENCH_GRID / 2.0),
        vec![panel, fps_badge(model)?],
    )?;
    Ok(box_item(cluster))
}

fn fps_badge(model: Model) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let tag = badge(
        BadgeProps::props()
            .label(Reactive::of(|| strings::text(DEV)))
            .color(Reactive::of(|| use_theme_tokens().success()))
            .build(),
        Children::default(),
    )?;
    let fps = Text::declaring(
        move || fps_label(model),
        LayoutStyle::new(),
        |text| workbench_mono(text).with_font_weight(600),
    )?;
    let pill = StyledContainer::new(
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .gap(WORKBENCH_GRID * 0.75)
            .padding_vertical(WORKBENCH_GRID / 2.0)
            .padding_horizontal(WORKBENCH_GRID * 0.75),
        |_| {
            let tokens = use_workbench_tokens();
            workbench_card().with_fill(tokens.panel_background)
        },
        vec![tag, box_item(fps)],
    )?
    .hover_style(|_| workbench_card().with_fill(use_theme_tokens().surface_alt()))
    .input_opaque()
    .control(Role::Button)
    .toggled(move || model.panel_open.get())
    .a11y_label(|| strings::text(TOGGLE_PANEL))
    .on_press(move || model.panel_open.toggle());
    Ok(box_item(pill))
}

fn fps_label(model: Model) -> String {
    strings::text_with(FPS, &model.reading.get().fps.to_string())
}

fn panel(model: Model) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let title = Text::declaring(
        || strings::text(DEVTOOLS),
        LayoutStyle::new(),
        |text| text.with_font_weight(600),
    )?;
    let fps = Text::declaring(
        move || fps_label(model),
        LayoutStyle::new(),
        |text| {
            text.with_font_size(FPS_SIZE)
                .with_font_weight(600)
                .with_color(use_theme_tokens().success())
        },
    )?;
    let timing = Text::declaring(
        move || {
            let millis = format!("{:.1}", model.reading.get().frame_millis);
            let nodes = model.nodes.with(|nodes| nodes.len()).to_string();
            format!(
                "{} · {}",
                strings::text_with(FRAME_TIME, &millis),
                strings::text_with(NODES, &nodes)
            )
        },
        LayoutStyle::new(),
        workbench_muted,
    )?;
    let renderer = row(vec![
        box_item(Text::declaring(
            || strings::text(RENDERER),
            LayoutStyle::new(),
            workbench_muted,
        )?),
        box_item(Text::declaring(
            move || {
                model
                    .renderer
                    .get()
                    .unwrap_or_else(|| strings::text(RENDERER_STARTING))
            },
            LayoutStyle::new(),
            workbench_mono,
        )?),
    ])?;
    let items = vec![
        box_item(title),
        box_item(fps),
        box_item(timing),
        renderer,
        divider()?,
        chord("Ctrl+Shift+B", SWITCH_RENDERER)?,
        chord("Ctrl+Shift+I", SHOW_INSPECTOR)?,
        chord("Ctrl+Shift+D", SHOW_PANEL)?,
    ];
    let panel = StyledContainer::new(
        LayoutStyle::new()
            .flex_column()
            .width(PANEL_WIDTH)
            .max_width(SizeDimension::Percent(1.0))
            .gap(WORKBENCH_GRID)
            .padding_all(WORKBENCH_GRID * 1.5),
        |_| workbench_card().with_fill(use_workbench_tokens().panel_background),
        items,
    )?
    .input_opaque()
    .role(Role::Group)
    .a11y_label(|| strings::text(DEVTOOLS));
    Ok(box_item(panel))
}

fn chord(keys: &'static str, action: &'static str) -> Result<Box<dyn LayoutItem>, LayoutError> {
    row(vec![
        kbd(KbdProps::props().chord(keys).build(), Children::default())?,
        box_item(Text::declaring(
            move || strings::text(action),
            LayoutStyle::new(),
            workbench_muted,
        )?),
    ])
}

fn row(items: Vec<Box<dyn LayoutItem>>) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let row = Container::new(
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .gap(WORKBENCH_GRID),
        items,
    )?;
    Ok(box_item(row))
}

fn divider() -> Result<Box<dyn LayoutItem>, LayoutError> {
    let line = StyledContainer::new(
        LayoutStyle::new().height(1.0).flex_shrink(0.0),
        |_| RectStyle::default().with_fill(use_workbench_tokens().border_subtle),
        Vec::new(),
    )?;
    Ok(box_item(line))
}
