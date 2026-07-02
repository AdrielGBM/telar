use crate::demo_images::{make_checker, make_gradient, make_radial_alpha};
use crate::demo_svgs::{make_blurred, make_icon, make_logo};
use crate::theme::theme;
use rsx::{
    App, AvailableSpace, Color, Component, Container, Event, EventResult, ImageData, LayoutError,
    LayoutItem, LayoutScrollArea, LayoutStyle, NodeId, RenderNode, SizeDimension, SvgData,
    WidgetCtx, compute_layout, mark_dirty, new_container,
};
use std::sync::{Arc, OnceLock};

fn build_content(
    ctx: &mut WidgetCtx,
    gradient: Arc<ImageData>,
    checker: Arc<ImageData>,
    alpha: Arc<ImageData>,
    icon: Arc<SvgData>,
    logo: Arc<SvgData>,
    blurred: Arc<SvgData>,
) -> Result<Container, LayoutError> {
    let s_theme = crate::sections_theme_section(ctx)?;
    let s_shapes = crate::sections_shapes_section(ctx)?;
    let s_colors = crate::sections_colors_section(ctx)?;
    let s_typography = crate::sections_typography_section(ctx)?;
    let s_cards = crate::sections_cards_section(ctx)?;
    let s_images = crate::sections_images_section(
        ctx,
        crate::SectionsImagesSectionProps {
            gradient,
            checker,
            alpha,
        },
    )?;
    let s_svg = crate::sections_svg_section(
        ctx,
        crate::SectionsSvgSectionProps {
            icon,
            logo,
            blurred,
        },
    )?;
    let s_lines = crate::sections_lines_section(ctx)?;
    let s_paths = crate::sections_paths_section(ctx)?;
    let s_gradients = crate::sections_gradients_section(ctx)?;
    let s_layers = crate::sections_layers_section(ctx)?;
    let s_shadows = crate::sections_shadows_section(ctx)?;
    let s_grid = crate::sections_grid_section(ctx)?;
    let s_transforms = crate::sections_transforms_section(ctx)?;
    let mut sections: Vec<Box<dyn LayoutItem>> = Vec::new();
    sections.push(s_theme);
    sections.push(s_shapes);
    sections.push(s_colors);
    sections.push(s_typography);
    sections.push(s_cards);
    sections.push(s_images);
    sections.push(s_svg);
    sections.push(s_lines);
    sections.push(s_paths);
    sections.push(s_gradients);
    sections.push(s_layers);
    sections.push(s_shadows);
    sections.push(s_grid);
    sections.push(s_transforms);
    sections.push(crate::counter(ctx)?);
    Container::new(
        ctx,
        LayoutStyle::new().flex_column().padding_all(24.0).gap(24.0),
        sections,
    )
}

/// Full-window scrolling page: a window-sized root holding a LayoutScrollArea whose viewport is recomputed on resize so content scrolls against the current window dimensions.
struct ScrollPage {
    ctx: WidgetCtx,
    root: NodeId,
    content_node: NodeId,
    scroll_area: LayoutScrollArea,
}

impl ScrollPage {
    fn new(mut ctx: WidgetCtx, content: Box<dyn LayoutItem>) -> Result<Self, LayoutError> {
        let content_node = content.layout_node();
        let scroll_area = LayoutScrollArea::new(
            &mut ctx,
            LayoutStyle::new().flex_grow(1.0).align_self_stretch(),
            content,
        )?;
        // Root fills the window via percent sizing so compute_layout against Definite(w, h) yields a full-window viewport for the scroll-area leaf.
        let root = new_container(
            &mut ctx,
            LayoutStyle::new()
                .flex_column()
                .width(SizeDimension::Percent(1.0))
                .height(SizeDimension::Percent(1.0)),
            &[scroll_area.layout_node()],
        )?;
        Ok(Self {
            ctx,
            root,
            content_node,
            scroll_area,
        })
    }

    fn relayout(&mut self, width: f32, height: f32) {
        mark_dirty(&mut self.ctx, self.root).ok();
        compute_layout(
            &mut self.ctx,
            self.root,
            AvailableSpace::Definite(width),
            AvailableSpace::Definite(height),
        )
        .ok();
        compute_layout(
            &mut self.ctx,
            self.content_node,
            AvailableSpace::Definite(width),
            AvailableSpace::MaxContent,
        )
        .ok();
        self.scroll_area.clamp_scroll();
    }
}

impl Component for ScrollPage {
    fn view(&self) -> RenderNode {
        self.scroll_area.view()
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        if let Event::WindowResized { width, height } = event {
            self.relayout(*width as f32, *height as f32);
            return EventResult::Handled;
        }
        self.scroll_area.on_event(event)
    }
}

pub struct SandboxRoot;

impl App for SandboxRoot {
    fn root(&self) -> Box<dyn rsx::Component> {
        static IMAGES: OnceLock<(Arc<ImageData>, Arc<ImageData>, Arc<ImageData>)> = OnceLock::new();
        let images = IMAGES.get_or_init(|| {
            (
                Arc::new(make_gradient(128, 128)),
                Arc::new(make_checker(128, 128, 16)),
                Arc::new(make_radial_alpha(128, 128)),
            )
        });

        static SVGS: OnceLock<(Arc<SvgData>, Arc<SvgData>, Arc<SvgData>)> = OnceLock::new();
        let svgs = SVGS.get_or_init(|| (make_icon(), make_logo(), make_blurred()));

        let mut ctx = WidgetCtx::new();
        let content = build_content(
            &mut ctx,
            images.0.clone(),
            images.1.clone(),
            images.2.clone(),
            svgs.0.clone(),
            svgs.1.clone(),
            svgs.2.clone(),
        )
        .expect("layout failed");

        let page = ScrollPage::new(ctx, Box::new(content)).expect("page layout failed");
        Box::new(page)
    }

    fn clear_color(&self) -> Option<Color> {
        Some(theme().background)
    }
}
