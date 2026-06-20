use crate::counter;
use crate::sections::{
    cards_section, colors_section, gradients_section, grid_section, images_section, layers_section,
    lines_section, paths_section, shadows_section, shapes_section, theme_section,
    transforms_section, typography_section,
};
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
    let s_theme = theme_section(ctx)?;
    let s_shapes = shapes_section(ctx)?;
    let s_colors = colors_section(ctx)?;
    let s_typography = typography_section(ctx)?;
    let s_cards = cards_section(ctx)?;
    let s_images = images_section(ctx, gradient, checker, alpha)?;
    let s_lines = lines_section(ctx)?;
    let s_paths = paths_section(ctx)?;
    let s_gradients = gradients_section(ctx)?;
    let s_layers = layers_section(ctx)?;
    let s_shadows = shadows_section(ctx)?;
    let s_grid = grid_section(ctx)?;
    let s_transforms = transforms_section(ctx)?;
    let mut sections: Vec<Box<dyn LayoutItem>> = rsx::children![
        s_theme,
        s_shapes,
        s_colors,
        s_typography,
        s_cards,
        s_images,
        s_lines,
        s_paths,
        s_gradients,
        s_layers,
        s_shadows,
        s_grid,
        s_transforms,
    ];
    sections.push(counter(ctx)?);
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
