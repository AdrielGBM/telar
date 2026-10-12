[logic]

// Reads through use_theme, so switching the theme re-runs this memo and updates the "Active" label.
let theme_name = memo(move || theme.get().name.to_string());

[view]
col gap:22
    col gap:2 role:banner
        row gap:8 align:center
            icon name:"lucide:triangle" size:20 color:$theme.ink
            text "rsx" font_size:20 color:$theme.ink
        text "Feature gallery" font_size:12 color:$theme.muted
    col gap:8
        text "THEME" font_size:11 color:$theme.muted
        col gap:6
            row gap:6 wrap
                button label:"Modern" color:$theme.primary on_press:(|| set_mode("modern"))
                button label:"Pastel" color:$theme.primary on_press:(|| set_mode("pastel"))
                button label:"Midnight" color:$theme.primary on_press:(|| set_mode("midnight"))
            text "Active · {$theme_name}" font_size:12 color:$theme.muted
