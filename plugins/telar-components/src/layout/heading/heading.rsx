[logic]
/// A title, and the heading of the given level to assistive technology and in a document. It takes the ink and family of the text around it, in the theme's strong weight and at a size set by its level alone, so a class on the call has only that to override. It is text, not a control, so it keeps its size whatever the control size in force.
pub struct Props {
    #[props(into, default)]
    pub text: Reactive<String>,
    /// Where the title sits in the page's outline: 1 for the page's own title, down to 6.
    #[props(control = range(1, 6))]
    pub level: u8 = 2,
}

fn scale(level: u8) -> f32 {
    match level {
        1 => 2.0,
        2 => 1.5,
        3 => 1.25,
        4 => 1.0,
        5 => 0.875,
        _ => 0.75,
    }
}

let level = props.level.clamp(1, 6);

[view]
text "{$props.text}" role:(Role::Heading(level)) font_size:(scale(level))em font_weight:$theme.font_weight_strong
