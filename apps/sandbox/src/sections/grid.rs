use std::sync::Arc;

use rsx::{
    BorderRadius, Canvas, Color, Container, DrawCommand, LayoutError, LayoutItem, LayoutStyle,
    Paint, Rect, RectPayload, RectStyle, RenderNode, TemplateTrack, TextPayload, TextStyle,
    WidgetCtx, use_theme,
};

use crate::theme::section;
use crate::theme::{SandboxTheme, heading};

pub fn grid_cell(
    ctx: &mut WidgetCtx,
    color_fn: impl Fn() -> Color + 'static,
    label: &'static str,
) -> Result<Canvas, LayoutError> {
    let label_arc: Arc<str> = Arc::from(label);
    Canvas::with_intrinsic_height(ctx, 72.0, move |rect| {
        let w = rect.width;
        let h = rect.height;
        RenderNode::group([
            RenderNode::Primitive(DrawCommand::Rect(Arc::new(RectPayload {
                rect: Rect {
                    x: 0.0,
                    y: 0.0,
                    width: w,
                    height: h,
                },
                style: RectStyle {
                    fill: Some(Paint::Solid(color_fn())),
                    stroke: None,
                    shadow: None,
                    radius: BorderRadius::all(6.0),
                },
            }))),
            RenderNode::Primitive(DrawCommand::Text(Arc::new(TextPayload {
                text: label_arc.clone(),
                rect: Rect {
                    x: 0.0,
                    y: 0.0,
                    width: w,
                    height: h,
                },
                style: TextStyle::new(13.0, use_theme::<SandboxTheme>().on_color),
            }))),
        ])
    })
}

pub fn grid_section(ctx: &mut WidgetCtx) -> Result<Container, LayoutError> {
    let gc1 = Box::new(grid_cell(ctx, || use_theme::<SandboxTheme>().primary, "1")?)
        as Box<dyn LayoutItem>;
    let gc2 = Box::new(grid_cell(ctx, || use_theme::<SandboxTheme>().success, "2")?)
        as Box<dyn LayoutItem>;
    let gc3 = Box::new(grid_cell(ctx, || use_theme::<SandboxTheme>().danger, "3")?)
        as Box<dyn LayoutItem>;
    let gc4 = Box::new(grid_cell(ctx, || use_theme::<SandboxTheme>().warning, "4")?)
        as Box<dyn LayoutItem>;
    let gc5 = Box::new(grid_cell(ctx, || use_theme::<SandboxTheme>().purple, "5")?)
        as Box<dyn LayoutItem>;
    let gc6 =
        Box::new(grid_cell(ctx, || use_theme::<SandboxTheme>().dark, "6")?) as Box<dyn LayoutItem>;
    let auto_grid = Container::new(
        ctx,
        LayoutStyle::new()
            .display_grid()
            .grid_template_columns(vec![TemplateTrack::repeat(3, TemplateTrack::fr(1.0))])
            .gap(12.0),
        vec![gc1, gc2, gc3, gc4, gc5, gc6],
    )?;

    let header = Canvas::new(
        ctx,
        LayoutStyle::new().height(48.0).grid_column_span(3),
        |rect| {
            let w = rect.width;
            let h = rect.height;
            let t = use_theme::<SandboxTheme>();
            RenderNode::group([
                RenderNode::Primitive(DrawCommand::Rect(Arc::new(RectPayload {
                    rect: Rect {
                        x: 0.0,
                        y: 0.0,
                        width: w,
                        height: h,
                    },
                    style: RectStyle {
                        fill: Some(Paint::Solid(t.dark)),
                        stroke: None,
                        shadow: None,
                        radius: BorderRadius::all(6.0),
                    },
                }))),
                RenderNode::Primitive(DrawCommand::Text(Arc::new(TextPayload {
                    text: crate::static_rc_str!("header — span 3"),
                    rect: Rect {
                        x: 0.0,
                        y: 0.0,
                        width: w,
                        height: h,
                    },
                    style: TextStyle::new(13.0, t.on_color),
                }))),
            ])
        },
    )?;
    let gca = Box::new(grid_cell(ctx, || use_theme::<SandboxTheme>().success, "A")?)
        as Box<dyn LayoutItem>;
    let gcb = Box::new(grid_cell(ctx, || use_theme::<SandboxTheme>().danger, "B")?)
        as Box<dyn LayoutItem>;
    let explicit_grid = Container::new(
        ctx,
        LayoutStyle::new()
            .display_grid()
            .grid_template_columns(vec![TemplateTrack::repeat(3, TemplateTrack::fr(1.0))])
            .gap(12.0),
        vec![Box::new(header) as Box<dyn LayoutItem>, gca, gcb],
    )?;

    let gcg1 = Box::new(grid_cell(
        ctx,
        || use_theme::<SandboxTheme>().primary,
        "G1",
    )?) as Box<dyn LayoutItem>;
    let gcg2 = Box::new(grid_cell(
        ctx,
        || use_theme::<SandboxTheme>().success,
        "G2",
    )?) as Box<dyn LayoutItem>;
    let gcg3 = Box::new(grid_cell(ctx, || use_theme::<SandboxTheme>().danger, "G3")?)
        as Box<dyn LayoutItem>;
    let gcg4 = Box::new(grid_cell(
        ctx,
        || use_theme::<SandboxTheme>().warning,
        "G4",
    )?) as Box<dyn LayoutItem>;
    let inner_grid = Container::new(
        ctx,
        LayoutStyle::new()
            .display_grid()
            .grid_template_columns(vec![TemplateTrack::fr(1.0), TemplateTrack::fr(1.0)])
            .flex_grow(1.0)
            .gap(8.0),
        vec![gcg1, gcg2, gcg3, gcg4],
    )?;
    let side_label = Canvas::new(ctx, LayoutStyle::new().width(180.0), |rect| {
        let w = rect.width;
        let h = rect.height;
        RenderNode::Primitive(DrawCommand::Text(Arc::new(TextPayload {
            text: crate::static_rc_str!("Grid nested\ninside flex →"),
            rect: Rect {
                x: 0.0,
                y: 0.0,
                width: w,
                height: h,
            },
            style: TextStyle::new(13.0, use_theme::<SandboxTheme>().muted),
        })))
    })?;
    let nested_row = Container::new(
        ctx,
        LayoutStyle::new().flex_row().gap(16.0),
        vec![
            Box::new(side_label) as Box<dyn LayoutItem>,
            Box::new(inner_grid) as Box<dyn LayoutItem>,
        ],
    )?;

    let h2 = heading(ctx, "Auto-placed (repeat(3, 1fr))")?;
    let h3 = heading(ctx, "Explicit placement (grid_column_span)")?;
    let h4 = heading(ctx, "Nested in Container")?;
    let content = Container::new(
        ctx,
        LayoutStyle::new().flex_column().gap(16.0),
        vec![
            h2,
            Box::new(auto_grid) as Box<dyn LayoutItem>,
            h3,
            Box::new(explicit_grid) as Box<dyn LayoutItem>,
            h4,
            Box::new(nested_row) as Box<dyn LayoutItem>,
        ],
    )?;
    section(ctx, "Grid", content)
}
