use std::sync::Arc;

use rsx::{
    Canvas, Color, Container, LayoutError, LayoutStyle, Rect, RectStyle, RenderNode, TemplateTrack,
    TextStyle, WidgetCtx, box_item, children,
};

use crate::theme::{heading, section, theme};

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
            RenderNode::rect(
                Rect {
                    x: 0.0,
                    y: 0.0,
                    width: w,
                    height: h,
                },
                RectStyle::filled(color_fn(), rsx::BorderRadius::all(6.0)),
            ),
            RenderNode::text(
                label_arc.clone(),
                Rect {
                    x: 0.0,
                    y: 0.0,
                    width: w,
                    height: h,
                },
                TextStyle::new(13.0, theme().on_color),
            ),
        ])
    })
}

pub fn grid_section(ctx: &mut WidgetCtx) -> Result<Container, LayoutError> {
    let gc1 = grid_cell(ctx, || theme().primary, "1")?;
    let gc2 = grid_cell(ctx, || theme().success, "2")?;
    let gc3 = grid_cell(ctx, || theme().danger, "3")?;
    let gc4 = grid_cell(ctx, || theme().warning, "4")?;
    let gc5 = grid_cell(ctx, || theme().purple, "5")?;
    let gc6 = grid_cell(ctx, || theme().dark, "6")?;
    let auto_grid = Container::new(
        ctx,
        LayoutStyle::new()
            .display_grid()
            .grid_template_columns(vec![TemplateTrack::repeat(3, TemplateTrack::fr(1.0))])
            .gap(12.0),
        children![gc1, gc2, gc3, gc4, gc5, gc6],
    )?;

    let header = Canvas::new(
        ctx,
        LayoutStyle::new().height(48.0).grid_column_span(3),
        |rect| {
            let w = rect.width;
            let h = rect.height;
            let t = theme();
            RenderNode::group([
                RenderNode::rect(
                    Rect {
                        x: 0.0,
                        y: 0.0,
                        width: w,
                        height: h,
                    },
                    RectStyle::filled(t.dark, rsx::BorderRadius::all(6.0)),
                ),
                RenderNode::text(
                    crate::static_rc_str!("header — span 3"),
                    Rect {
                        x: 0.0,
                        y: 0.0,
                        width: w,
                        height: h,
                    },
                    TextStyle::new(13.0, t.on_color),
                ),
            ])
        },
    )?;
    let gca = grid_cell(ctx, || theme().success, "A")?;
    let gcb = grid_cell(ctx, || theme().danger, "B")?;
    let explicit_grid = Container::new(
        ctx,
        LayoutStyle::new()
            .display_grid()
            .grid_template_columns(vec![TemplateTrack::repeat(3, TemplateTrack::fr(1.0))])
            .gap(12.0),
        children![header, gca, gcb],
    )?;

    let gcg1 = grid_cell(ctx, || theme().primary, "G1")?;
    let gcg2 = grid_cell(ctx, || theme().success, "G2")?;
    let gcg3 = grid_cell(ctx, || theme().danger, "G3")?;
    let gcg4 = grid_cell(ctx, || theme().warning, "G4")?;
    let inner_grid = Container::new(
        ctx,
        LayoutStyle::new()
            .display_grid()
            .grid_template_columns(vec![TemplateTrack::fr(1.0), TemplateTrack::fr(1.0)])
            .flex_grow(1.0)
            .gap(8.0),
        children![gcg1, gcg2, gcg3, gcg4],
    )?;
    let side_label = Canvas::new(ctx, LayoutStyle::new().width(180.0), |rect| {
        RenderNode::text(
            crate::static_rc_str!("Grid nested\ninside flex →"),
            Rect {
                x: 0.0,
                y: 0.0,
                width: rect.width,
                height: rect.height,
            },
            TextStyle::new(13.0, theme().muted),
        )
    })?;
    let nested_row = Container::new(
        ctx,
        LayoutStyle::new().flex_row().gap(16.0),
        children![side_label, inner_grid],
    )?;

    let h2 = heading(ctx, "Auto-placed (repeat(3, 1fr))")?;
    let h3 = heading(ctx, "Explicit placement (grid_column_span)")?;
    let h4 = heading(ctx, "Nested in Container")?;
    let content = Container::new(
        ctx,
        LayoutStyle::new().flex_column().gap(16.0),
        vec![
            h2,
            box_item(auto_grid),
            h3,
            box_item(explicit_grid),
            h4,
            box_item(nested_row),
        ],
    )?;
    section(ctx, "Grid", content)
}
