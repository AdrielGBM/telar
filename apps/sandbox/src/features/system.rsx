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
    example title:"Reduced motion — the bar slides, or jumps straight to its end"
        card gap:12
            button label:"Slide" fill:$theme.primary on_press:(|| $pulse.set(if $pulse.get() < 0.5 { 1.0 } else { 0.0 }))
            box fill:$theme.surface_alt radius:6 height:12 width:260
                box fill:$theme.primary radius:6 height:12 width:40 translate_x:($pulse * 220.0) transition(translate_x 1200ms ease-in-out)
        code_line code:"follow_reduced_motion(false)   // opt out"
