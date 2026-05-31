use crate::images::{make_checker, make_gradient, make_radial_alpha};
use crate::sections::{
    cards_section, colors_section, gradients_section, grid_section, images_section, layers_section,
    lines_section, paths_section, shadows_section, shapes_section, theme_section,
    transforms_section, typography_section,
};
use crate::theme::SandboxTheme;
use rsx::{
    App, AvailableSpace, Color, Component, Container, Event, EventResult, ImageData, LayoutError,
    LayoutItem, LayoutStyle, NodeId, Rect, RenderNode, RwSignal, ScrollArea, WidgetCtx,
    compute_layout, create_rw_signal, use_theme, with_context,
};
use std::rc::Rc;

fn build_content(
    ctx: &mut WidgetCtx,
    gradient: Rc<ImageData>,
    checker: Rc<ImageData>,
    alpha: Rc<ImageData>,
) -> Result<Container, LayoutError> {
    let s0 = Box::new(theme_section(ctx)?) as Box<dyn LayoutItem>;
    let s1 = Box::new(shapes_section(ctx)?) as Box<dyn LayoutItem>;
    let s2 = Box::new(colors_section(ctx)?) as Box<dyn LayoutItem>;
    let s3 = Box::new(typography_section(ctx)?) as Box<dyn LayoutItem>;
    let s4 = Box::new(cards_section(ctx)?) as Box<dyn LayoutItem>;
    let s5 = Box::new(images_section(ctx, gradient, checker, alpha)?) as Box<dyn LayoutItem>;
    let s6 = Box::new(lines_section(ctx)?) as Box<dyn LayoutItem>;
    let s7 = Box::new(paths_section(ctx)?) as Box<dyn LayoutItem>;
    let s8 = Box::new(gradients_section(ctx)?) as Box<dyn LayoutItem>;
    let s9 = Box::new(layers_section(ctx)?) as Box<dyn LayoutItem>;
    let s10 = Box::new(shadows_section(ctx)?) as Box<dyn LayoutItem>;
    let s11 = Box::new(grid_section(ctx)?) as Box<dyn LayoutItem>;
    let s12 = Box::new(transforms_section(ctx)?) as Box<dyn LayoutItem>;
    let sections = vec![s0, s1, s2, s3, s4, s5, s6, s7, s8, s9, s10, s11, s12];

    Container::new(
        ctx,
        LayoutStyle::new().flex_column().padding_all(24.0).gap(24.0),
        sections,
    )
}

struct SandboxRootComponent {
    ctx: WidgetCtx,
    content_node: NodeId,
    window_width: RwSignal<f32>,
    window_height: RwSignal<f32>,
    scroll_area: ScrollArea,
}

impl Component for SandboxRootComponent {
    fn view(&self) -> RenderNode {
        self.scroll_area.view()
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        if let Event::WindowResized { width, height } = event {
            let w = *width as f32;
            self.window_width.set(w);
            self.window_height.set(*height as f32);
            self.ctx.mark_dirty_node(self.content_node).ok();
            compute_layout(
                &mut self.ctx,
                self.content_node,
                AvailableSpace::Definite(w.max(480.0)),
                AvailableSpace::MaxContent,
            )
            .ok();
            self.scroll_area.clamp_scroll();
            return EventResult::Handled;
        }
        self.scroll_area.on_event(event)
    }
}

pub struct SandboxRoot;

impl App for SandboxRoot {
    fn root(&self) -> Box<dyn Component> {
        let window_width = create_rw_signal(800.0f32);
        let window_height = create_rw_signal(600.0f32);

        let gradient_image = Rc::new(make_gradient(128, 128));
        let checker_image = Rc::new(make_checker(128, 128, 16));
        let alpha_image = Rc::new(make_radial_alpha(128, 128));

        let (build, ctx) = with_context(WidgetCtx::new(), |ctx| {
            let content = build_content(
                ctx,
                gradient_image.clone(),
                checker_image.clone(),
                alpha_image.clone(),
            )?;

            let content_node = content.layout_node();
            let ww = window_width.clone();
            let wh = window_height.clone();
            let scroll_area = ScrollArea::new(
                ctx,
                move || Rect::new(0.0, 0.0, ww.get(), wh.get()),
                Box::new(content),
            );

            compute_layout(
                ctx,
                content_node,
                AvailableSpace::Definite(window_width.get()),
                AvailableSpace::MaxContent,
            )?;

            Ok::<_, LayoutError>((scroll_area, content_node))
        });

        let (scroll_area, content_node) = build.expect("layout failed");

        Box::new(SandboxRootComponent {
            ctx,
            content_node,
            window_width,
            window_height,
            scroll_area,
        })
    }

    fn clear_color(&self) -> Option<Color> {
        Some(use_theme::<SandboxTheme>().background)
    }
}
