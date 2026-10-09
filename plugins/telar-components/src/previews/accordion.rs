use telar::Color;
use telar::preview::{Layout, Matrix, PreviewEntry, preview};

use super::sample;
use crate::accordion::{AccordionProps, accordion};

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(accordion: AccordionProps, "Default", |p| {
            accordion(
                AccordionProps::props()
                    .title(p.arg("title", "Shipping details"))
                    .open(p.signal("open", true))
                    .color(p.arg("color", Color::TRANSPARENT))
                    .build(),
                sample::paragraph("Orders leave the warehouse within two working days."),
            )
        })
        .title("Layout/Accordion")
        .layout(Layout::Padded)
        .matrix(Matrix::Named("themes"))
        .tags(&["stateful"]),
    ]
}
