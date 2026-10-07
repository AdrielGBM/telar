//! [`icon`]: one icon, sized like text, coloured like the text around it, and silent to a screen reader unless it is given a name.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::{Arc, LazyLock};

use telar::{
    Accessible, Children, Color, DrawCommand, LayoutError, LayoutItem, LayoutStyle, NodeId,
    ObjectFit, Paint, Props, Reactive, RectStyle, SizeDimension, StyledContainer, Svg, SvgData,
    box_item, inherited_text_style, use_theme_tokens,
};

use crate::IconName;

/// Draws one icon in a square box.
#[derive(Props)]
pub struct IconProps {
    /// The icon, `set:name`: `name:"mdi:home"`.
    #[props(into)]
    pub name: IconName,
    /// The side of the box in px. Unset (`0`), the theme's `icon_size`.
    #[props(into, default)]
    pub size: Reactive<f32>,
    /// The colour the icon is drawn in. Unset (`Color::TRANSPARENT`), an icon drawn in one colour takes the colour of the text around it, and a multicolour one keeps its own.
    #[props(into, default = Reactive::of(|| Color::TRANSPARENT))]
    pub color: Reactive<Color>,
    /// What the icon means, for a screen reader. Unset, the icon is decoration and assistive technology skips it; set, it is a picture by that name. Give it one when the icon is the only thing saying something, such as a button with no text.
    #[props(into, default)]
    pub label: Reactive<String>,
}

/// Draws the icon `name` in a `size`-px square, tinted like the text around it.
///
/// An id that was not baked is resolved by the runtime source the application installed (the `runtime` feature); until it arrives, and when nothing can answer for it, the box is drawn empty at its full size, so nothing around it moves when it lands.
pub fn icon(props: IconProps, _children: Children) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let IconProps {
        name,
        size,
        color,
        label,
    } = props;

    let glyph = glyph(name);
    let node: Rc<Cell<Option<NodeId>>> = Rc::new(Cell::new(None));
    let tint = {
        let glyph = Rc::clone(&glyph);
        let node = Rc::clone(&node);
        let one_ink = OneInk::default();
        move || {
            let chosen = color.get();
            if chosen != Color::TRANSPARENT {
                return Some(chosen);
            }
            let svg = glyph()?;
            if !one_ink.of(&svg) {
                return None;
            }
            node.get()
                .map(|node| inherited_text_style(node).color.solid_color())
        }
    };
    let artwork = Svg::new(
        LayoutStyle::new()
            .width(SizeDimension::Percent(1.0))
            .height(SizeDimension::Percent(1.0)),
        move || glyph().unwrap_or_else(|| Arc::clone(&EMPTY)),
        tint,
        || ObjectFit::Contain,
    )?;
    node.set(Some(artwork.layout_node()));

    let side = move || match size.get() {
        side if side > 0.0 => side,
        _ => use_theme_tokens().icon_size(),
    };
    let frame = StyledContainer::new(
        square(side()),
        |_| RectStyle::default(),
        vec![box_item(artwork)],
    )?
    .styled_by(move || square(side()));

    let frame = match label {
        Reactive::Const(text) if text.is_empty() => frame.a11y_hidden(),
        label => frame.a11y_label(move || label.get()),
    };
    Ok(box_item(frame))
}

fn square(side: f32) -> LayoutStyle {
    LayoutStyle::new().width(side).height(side).flex_shrink(0.0)
}

/// What an icon with nothing to draw yet draws: nothing, in a box of any size.
static EMPTY: LazyLock<Arc<SvgData>> =
    LazyLock::new(|| Arc::new(SvgData::from_baked_vector((1.0, 1.0), Vec::new())));

/// The artwork for `name`, as it reads now.
fn glyph(name: IconName) -> Rc<dyn Fn() -> Option<Arc<SvgData>>> {
    match name {
        IconName::Baked { svg, .. } => Rc::new(move || Some(Arc::clone(&svg))),
        IconName::Named(id) => Rc::new(move || crate::runtime::resolve(&id.get())),
    }
}

/// Whether an icon is drawn in a single colour, remembered for the last icon asked about so a frame does not walk its paths again.
///
/// That is the test for tinting it, rather than whether its set calls itself monochrome: an Iconify set's own icons are `currentColor` throughout, the application's own may be drawn in any one colour, and a palette icon — a flag, a logo — is the one whose colours are the point.
#[derive(Default)]
struct OneInk {
    last: Cell<Option<(u64, bool)>>,
}

impl OneInk {
    fn of(&self, svg: &SvgData) -> bool {
        if let Some((id, answer)) = self.last.get()
            && id == svg.id()
        {
            return answer;
        }
        let (width, height) = svg.intrinsic_size();
        let answer =
            draws_in_one_ink(&svg.commands_for(width, height, None, None, ObjectFit::Contain));
        self.last.set(Some((svg.id(), answer)));
        answer
    }
}

fn draws_in_one_ink(commands: &[DrawCommand]) -> bool {
    let mut ink: Option<Color> = None;
    for command in commands {
        let paints = match command {
            DrawCommand::Path { style, .. } => style
                .fill
                .iter()
                .chain(style.stroke.iter().map(|stroke| &stroke.paint))
                .collect::<Vec<&Paint>>(),
            DrawCommand::Image { .. } => return false,
            _ => continue,
        };
        for paint in paints {
            let Paint::Solid(color) = paint else {
                return false;
            };
            let opaque = color.with_alpha(1.0);
            match ink {
                None => ink = Some(opaque),
                Some(seen) if seen == opaque => {}
                Some(_) => return false,
            }
        }
    }
    true
}

#[cfg(test)]
#[path = "icon_test.rs"]
mod tests;
