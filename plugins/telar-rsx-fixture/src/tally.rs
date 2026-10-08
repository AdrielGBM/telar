//! [`tally`]: a component written in Rust beside the `.rsx` one, so the fixture carries previews written in Rust.

use telar::{
    Children, Color, LayoutError, LayoutItem, LayoutStyle, Props, Reactive, Text, TextStyle,
    box_item,
};

/// A label beside how many of it there are.
#[derive(Props)]
pub struct TallyProps {
    /// What is counted.
    #[props(into)]
    pub label: Reactive<String>,
    /// How many there are.
    #[props(into, default)]
    pub count: Reactive<u32>,
}

/// A label beside how many of it there are, as `label · count`.
pub fn tally(props: TallyProps, _children: Children) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let TallyProps { label, count } = props;
    let text = Text::new(
        move || format!("{} · {}", label.get(), count.get()),
        LayoutStyle::new(),
        || TextStyle::new(14.0, Color::BLACK),
    )?;
    Ok(box_item(text))
}
