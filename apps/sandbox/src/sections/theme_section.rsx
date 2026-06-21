[logic]
use crate::theme::{SandboxTheme, theme};

let theme_name = create_memo(move || theme().name.to_string());

[view]
col gap:8
    text "Theme" size:12 color:muted
    row gap:8
        btn "Modern" fill:primary on_press:|| set_theme_with_widgets(SandboxTheme::modern())
        btn "Pastel" fill:primary on_press:|| set_theme_with_widgets(SandboxTheme::pastel())
    text "Active: {theme_name}" size:13 color:muted
