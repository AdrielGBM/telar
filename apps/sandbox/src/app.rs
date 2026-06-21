use crate::test_assets::{make_checker, make_gradient, make_radial_alpha};
use crate::theme::theme;
use rsx::{
    App, Color, Container, ImageData, LayoutError, LayoutItem, LayoutStyle, ScrollablePage,
    WidgetCtx,
};
use std::sync::{Arc, OnceLock};

fn build_content(
    ctx: &mut WidgetCtx,
    gradient: Arc<ImageData>,
    checker: Arc<ImageData>,
    alpha: Arc<ImageData>,
) -> Result<Container, LayoutError> {
    let s_theme = crate::theme_section(ctx)?;
    let s_shapes = crate::shapes_section(ctx)?;
    let s_colors = crate::colors_section(ctx)?;
    let s_typography = crate::typography_section(ctx)?;
    let s_cards = crate::cards_section(ctx)?;
    let s_images = crate::images_section(ctx, gradient, checker, alpha)?;
    let s_lines = crate::lines_section(ctx)?;
    let s_paths = crate::paths_section(ctx)?;
    let s_gradients = crate::gradients_section(ctx)?;
    let s_layers = crate::layers_section(ctx)?;
    let s_shadows = crate::shadows_section(ctx)?;
    let s_grid = crate::grid_section(ctx)?;
    let s_transforms = crate::transforms_section(ctx)?;
    let mut sections: Vec<Box<dyn LayoutItem>> = Vec::new();
    sections.push(s_theme);
    sections.push(s_shapes);
    sections.push(s_colors);
    sections.push(s_typography);
    sections.push(s_cards);
    sections.push(s_images);
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
        Some(theme().background)
    }
}
