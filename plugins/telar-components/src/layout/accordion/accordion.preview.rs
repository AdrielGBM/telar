use telar::preview::{Layout, Matrix, PreviewEntry, by_role, by_text, preview};
use super::sample::Given;
use telar::{Color, Role};

use super::sample;
use crate::layout::accordion::accordion::{AccordionProps, accordion};
use telar::ControlSize;

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(accordion: AccordionProps, "Default", |p| {
            accordion(
                AccordionProps::props()
                    .title(p.arg("title", "Shipping details"))
                    .size(p.arg("size", ControlSize::Regular))
                    .open(p.signal("open", true))
                    .given(p.arg("color", None::<Color>), |props, color| props.color(color))
                    .children(sample::paragraph(
                        "Orders leave the warehouse within two working days.",
                    ))
                    .build(),
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
            canvas.expect_absent(by_text(
                "Orders leave the warehouse within two working days.",
            ))?;
            canvas.click(details.clone())?;
            canvas.expect_toggled(details, true)?;
            canvas.expect_text("Orders leave the warehouse")
        }),
    ]
}
