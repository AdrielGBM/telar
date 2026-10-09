use telar::preview::{Layout, Matrix, PreviewEntry, preview};
use telar::{Children, Color, Container, LayoutError, LayoutStyle, Reactive};

use crate::badge::{BadgeProps, badge};

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(badge: BadgeProps, "Default", |p| {
            badge(
                BadgeProps::props()
                    .label(p.arg("label", "New"))
                    .color(p.arg("color", Color::TRANSPARENT))
                    .build(),
                Children::default(),
            )
        })
        .title("Display/Badge")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes")),
        preview!(badge: BadgeProps, "Semantic colours", |_p| {
            let badges = [
                ("Info", Reactive::of(|| telar::use_theme_tokens().info())),
                ("Success", Reactive::of(|| telar::use_theme_tokens().success())),
                ("Warning", Reactive::of(|| telar::use_theme_tokens().warning())),
                ("Error", Reactive::of(|| telar::use_theme_tokens().error())),
            ]
            .into_iter()
            .map(|(label, color)| {
                badge(
                    BadgeProps::props().label(label).color(color).build(),
                    Children::default(),
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
            Ok::<_, LayoutError>(Container::new(
                LayoutStyle::new().flex_row().gap(8.0),
                badges,
            )?)
        })
        .title("Display/Badge")
        .layout(Layout::Centered),
    ]
}
