[logic]
/// A title, and the heading of the given level to assistive technology and in a document. It takes the ink and family of the text around it, in the theme's strong weight and at a size set by its level alone, so a class on the call has only that to override. It is text, not a control, so it keeps its size whatever the control size in force.
pub struct Props {
    #[props(into, default)]
    pub text: Reactive<String>,
    /// Where the title sits in the page's outline: 1 for the page's own title, down to 6; a number outside is the nearest of the two.
    #[props(control = range(1, 6))]
    pub level: u8 = 2,
}

fn scale(level: HeadingLevel) -> f32 {
    match level {
        HeadingLevel::One => 2.0,
        HeadingLevel::Two => 1.5,
        HeadingLevel::Three => 1.25,
        HeadingLevel::Four => 1.0,
        HeadingLevel::Five => 0.875,
        HeadingLevel::Six => 0.75,
    }
}

let level = HeadingLevel::nearest(props.level);

[view]
text "{$props.text}" role:(Role::Heading(level)) font_size:(scale(level))em font_weight:$theme.font_weight_strong
