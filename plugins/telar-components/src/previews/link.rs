use telar::preview::{Layout, Matrix, PreviewEntry, by_role, preview};
use telar::{Children, Container, LayoutError, LayoutStyle, Location, Role, external};

use crate::link::{LinkProps, link};

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(link: LinkProps, "Default", |p| {
            let links = vec![
                link(
                    LinkProps::props()
                        .label(p.arg("label", "Getting started"))
                        .to(Location::root().segment("guide"))
                        .current(p.arg("current", false))
                        .build(),
                    Children::default(),
                )?,
                link(
                    LinkProps::props()
                        .label("Source code")
                        .to(external("https://github.com/AdrielGBM/telar"))
                        .build(),
                    Children::default(),
                )?,
                link(
                    LinkProps::props()
                        .label("Home")
                        .to(Location::root())
                        .current(true)
                        .build(),
                    Children::default(),
                )?,
            ];
            Ok::<_, LayoutError>(Container::new(LayoutStyle::column().gap(12.0), links)?)
        })
        .title("Navigation/Link")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes"))
        .play(|canvas| {
            canvas.expect(by_role(Role::Link).named("Getting started"))?;
            canvas.expect(by_role(Role::Link).named("Source code (external link)"))?;
            canvas.tab()?;
            canvas.expect_focused(by_role(Role::Link).named("Getting started"))
        }),
    ]
}
