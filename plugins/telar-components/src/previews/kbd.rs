use telar::preview::{Layout, Matrix, PreviewEntry, preview};

use crate::ControlSize;
use crate::kbd::{KbdProps, kbd};

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(kbd: KbdProps, "Default", |p| {
            kbd(KbdProps::props()
                .chord(p.arg("chord", "Mod+Shift+P"))
                .size(p.arg("size", ControlSize::Regular))
                .build())
        })
        .title("Display/Kbd")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes")),
    ]
}
