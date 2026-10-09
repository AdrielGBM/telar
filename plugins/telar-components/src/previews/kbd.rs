use telar::Children;
use telar::preview::{Layout, Matrix, PreviewEntry, preview};

use crate::kbd::{KbdProps, kbd};

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(kbd: KbdProps, "Default", |p| {
            kbd(
                KbdProps::props()
                    .chord(p.arg("chord", "Mod+Shift+P"))
                    .build(),
                Children::default(),
            )
        })
        .title("Display/Kbd")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes")),
    ]
}
