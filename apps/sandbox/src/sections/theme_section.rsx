[logic]
use crate::theme::{SandboxTheme, theme};

let theme_choice = signal(0i32);
if theme_choice.peek() == 1 { set_theme_with_widgets(SandboxTheme::pastel()); }
let theme_name = memo(move || theme().name.to_string());

[view]
col gap:8
    text "Theme" size:12 color:muted
    row gap:8
        btn "Modern" fill:primary on_press:|| { set_theme_with_widgets(SandboxTheme::modern()); $theme_choice.set(0) }
        btn "Pastel" fill:primary on_press:|| { set_theme_with_widgets(SandboxTheme::pastel()); $theme_choice.set(1) }
    text "Active: {$theme_name}" size:13 color:muted
