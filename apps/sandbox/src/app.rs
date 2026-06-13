use crate::images::{make_checker, make_gradient, make_radial_alpha};
use crate::sections::{
    cards_section, colors_section, gradients_section, grid_section, images_section, layers_section,
    lines_section, paths_section, shadows_section, shapes_section, theme_section,
    transforms_section, typography_section,
};
use crate::theme::SandboxTheme;
use rsx::{
    App, Color, Container, ImageData, LayoutError, LayoutItem, LayoutStyle, ScrollablePage,
    WidgetCtx, use_theme,
};
use std::sync::Arc;

fn build_content(
    ctx: &mut WidgetCtx,
    gradient: Arc<ImageData>,
    checker: Arc<ImageData>,
    alpha: Arc<ImageData>,
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

pub struct SandboxRoot;

impl App for SandboxRoot {
    fn root(&self) -> Box<dyn rsx::Component> {
        let gradient_image = Arc::new(make_gradient(128, 128));
        let checker_image = Arc::new(make_checker(128, 128, 16));
        let alpha_image = Arc::new(make_radial_alpha(128, 128));

        let mut ctx = WidgetCtx::new();
        let content = build_content(
            &mut ctx,
            gradient_image.clone(),
            checker_image.clone(),
            alpha_image.clone(),
        )
        .expect("layout failed");

        let mut page = ScrollablePage::new(ctx, Box::new(content), 0.0, 0.0);
        page.compute_layout();
        Box::new(page)
    }

    fn clear_color(&self) -> Option<Color> {
        Some(use_theme::<SandboxTheme>().background)
    }
}
