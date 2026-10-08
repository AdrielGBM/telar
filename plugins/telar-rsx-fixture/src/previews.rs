//! The fixture's previews written in Rust, compiled only under `telar/previews`.

use telar::preview::{Layout, Matrix, PreviewEntry, preview};
use telar::{Children, Container, LayoutError, LayoutItem, LayoutStyle, box_item};

use crate::tally::{TallyProps, tally};

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(tally: TallyProps, "Default", |p| {
            tally(
                TallyProps::props()
                    .label(p.arg("label", "Apples"))
                    .count(p.arg("count", 3u32))
                    .build(),
                Children::default(),
            )
        })
        .title("Fixture/Tally")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes")),
        preview!(tally: TallyProps, "Counting", |p| {
            let count = p.signal("count", 0u32);
            tally(
                TallyProps::props().label("Presses").count(count).build(),
                Children::default(),
            )
        })
        .title("Fixture/Tally")
        .tags(&["stateful"])
        .decorate(framed)
        .play(|_| Ok(())),
    ]
}

fn framed(children: Children) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let content = children.build()?.take_default();
    Ok(box_item(Container::new(
        LayoutStyle::new().padding_all(12.0),
        content,
    )?))
}
