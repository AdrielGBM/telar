[logic]
use crate::core::theme::{SandboxTheme, theme};

// Reads through use_theme, so switching the theme re-runs this memo and updates the "Active" label.
let theme_name = memo(move || theme().name.to_string());

[view]
col gap:22
    col gap:2
        text "▲ rsx" size:20 color:ink
        text "Feature gallery" size:12 color:muted
    col gap:8
        text "THEME" size:11 color:muted
        col gap:6
            row gap:6 wrap
                button label:"Modern" fill:primary on_press:|| set_theme_with_widgets(SandboxTheme::modern())
                button label:"Pastel" fill:primary on_press:|| set_theme_with_widgets(SandboxTheme::pastel())
                button label:"Midnight" fill:primary on_press:|| set_theme_with_widgets(SandboxTheme::midnight())
            text "Active · {$theme_name}" size:12 color:muted
