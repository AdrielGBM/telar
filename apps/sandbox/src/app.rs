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
use std::sync::{Arc, OnceLock};

fn build_content(
    ctx: &mut WidgetCtx,
    gradient: Arc<ImageData>,
    checker: Arc<ImageData>,
    alpha: Arc<ImageData>,
) -> Result<Container, LayoutError> {
    let sections: Vec<Box<dyn LayoutItem>> = vec![
        Box::new(theme_section(ctx)?),
        Box::new(shapes_section(ctx)?),
        Box::new(colors_section(ctx)?),
        Box::new(typography_section(ctx)?),
        Box::new(cards_section(ctx)?),
        Box::new(images_section(ctx, gradient, checker, alpha)?),
        Box::new(lines_section(ctx)?),
        Box::new(paths_section(ctx)?),
        Box::new(gradients_section(ctx)?),
        Box::new(layers_section(ctx)?),
        Box::new(shadows_section(ctx)?),
        Box::new(grid_section(ctx)?),
        Box::new(transforms_section(ctx)?),
    ];
    Container::new(
        ctx,
        LayoutStyle::new().flex_column().padding_all(24.0).gap(24.0),
        sections,
    )
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

        let mut ctx = WidgetCtx::new();
        let content = build_content(
            &mut ctx,
            images.0.clone(),
            images.1.clone(),
            images.2.clone(),
        )
        .expect("layout failed");

        let page = ScrollablePage::new(ctx, Box::new(content), 0.0, 0.0);
        Box::new(page)
    }

    fn clear_color(&self) -> Option<Color> {
        Some(use_theme::<SandboxTheme>().background)
    }
}
