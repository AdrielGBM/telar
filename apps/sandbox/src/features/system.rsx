[logic]
use crate::shared::components::card::{card, CardProps};
use crate::shared::components::code_line::{code_line, CodeLineProps};
use crate::shared::components::doc_header::{doc_header, DocHeaderProps};
use crate::shared::components::example::{example, ExampleProps};

fn reading<T: std::fmt::Debug>(value: Option<T>) -> String {
    value.map_or_else(|| "unknown".to_string(), |v| format!("{v:?}"))
}

let scheme = memo(|| reading(telar::use_color_scheme()));
let reduced = memo(|| reading(telar::use_reduced_motion()));
let contrast = memo(|| reading(telar::use_high_contrast()));
let locales = memo(|| {
    let locales = telar::use_preferred_locales();
    if locales.is_empty() { "none reported".to_string() } else { locales.join(", ") }
});
let negotiated = memo(|| telar::negotiate_locale(&telar::use_preferred_locales(), &["es", "en"], "es").to_string());
let pulse = signal(0.0f32);
let chosen = memo(|| telar::use_scheme_preference().as_str().to_string());
let safe_area = memo(|| {
    let insets = telar::use_safe_area_insets();
    format!("top {} · right {} · bottom {} · left {}", insets.top, insets.right, insets.bottom, insets.left)
});
let resolved = memo(|| format!("{:?}", telar::use_resolved_scheme()));

[view]
col gap:20
    doc_header kicker:"FOUNDATIONS" title:"System preferences" desc:"What the platform says the user prefers — colour scheme, reduced motion, more contrast and languages — read live. Change them in the system settings and this page follows without a restart; the theme follows the scheme and every animation stops under reduced motion."
    example title:"As the platform reports them right now"
        card gap:6
            text "color scheme · {$scheme}" font_size:15 color:$theme.ink
            text "reduced motion · {$reduced}" font_size:15 color:$theme.ink
            text "high contrast · {$contrast}" font_size:15 color:$theme.ink
            text "languages · {$locales}" font_size:15 color:$theme.ink
            text "negotiated against es, en · {$negotiated}" font_size:15 color:$theme.primary
        code_line code:"use_color_scheme()   ·   use_reduced_motion()   ·   use_high_contrast()   ·   use_preferred_locales()"
    example title:"The safe area — what the system keeps of the surface"
        card gap:6
            text "{$safe_area}" font_size:13 color:$theme.ink
            text "Status and navigation bars, a notch, rounded corners: Android's window insets, a page's env(safe-area-inset-*) when drawn to the edges, zero on a desktop window and a terminal." font_size:12 color:$theme.muted
        code_line code:"use_safe_area_insets()   ·   ScrollPage::new(content).keep_to_safe_area()"
    example title:"A scheme the person chooses, kept between runs"
        card gap:8
            text "Chosen: {$chosen} · in use: {$resolved}" font_size:13 color:$theme.ink
            row gap:8
                button label:"System" ghost on_press:(|| telar::set_scheme_preference(telar::SchemePreference::System))
                button label:"Light" ghost on_press:(|| telar::set_scheme_preference(telar::SchemePreference::Light))
                button label:"Dark" ghost on_press:(|| telar::set_scheme_preference(telar::SchemePreference::Dark))
            text "A fixed choice holds whatever the system does until it is changed here, and is kept in the target's preference store: a file beside prefs.toml, or localStorage on the web." font_size:12 color:$theme.muted
        code_line code:"set_scheme_preference(SchemePreference::Dark)   ·   use_resolved_scheme()   ·   store_preference(key, value)"
    example title:"Reduced motion — the bar slides, or jumps straight to its end"
        card gap:12
            button label:"Slide" fill:$theme.primary on_press:(|| $pulse.set(if $pulse.get() < 0.5 { 1.0 } else { 0.0 }))
            box fill:$theme.surface_alt radius:6 height:12 width:260
                box fill:$theme.primary radius:6 height:12 width:40 translate_x:($pulse * 220.0) transition(translate_x 1200ms ease-in-out)
        code_line code:"follow_reduced_motion(false)   // opt out"
