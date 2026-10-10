use telar::preview::{Layout, Matrix, PreviewEntry, by_role, by_text, preview};
use telar::{Color, Role};

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
        .tags(&["stateful"])
        .play(|canvas| {
            let details = by_role(Role::Disclosure).containing("Shipping details");
            canvas.expect_text("Orders leave the warehouse within two working days.")?;
            canvas.click(details.clone())?;
            canvas.expect_toggled(details.clone(), false)?;
            canvas.expect_absent(by_text("Orders leave the warehouse within two working days."))?;
            canvas.click(details.clone())?;
            canvas.expect_toggled(details, true)?;
            canvas.expect_text("Orders leave the warehouse")
        }),
    ]
}
