//! [`icon`]: one icon, sized like text, coloured like the text around it when it is monochrome, and silent to a screen reader unless it is given a name.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::{Arc, LazyLock};

use telar::{
    Accessible, Children, Color, LayoutError, LayoutItem, LayoutStyle, NodeId, ObjectFit, Props,
    Reactive, RectStyle, SizeDimension, StyledContainer, Svg, SvgData, box_item,
    inherited_text_style, use_theme_tokens,
};

use crate::IconName;

/// Draws one icon in a square box.
#[derive(Props)]
pub struct IconProps {
    /// The icon, `set:name`: `name:"mdi:home"`. A bare name, `name:"home"`, is read in `[telar.icons] default_set`.
    #[props(into)]
    pub name: IconName,
    /// The side of the box in px. Unset (`0`), the theme's `icon_size`.
    #[props(into, default)]
    pub size: Reactive<f32>,
    /// The colour the icon is drawn in, as a silhouette. Unset (`Color::TRANSPARENT`), a monochrome icon takes the colour of the text around it, and one of a palette set keeps its own colours.
    #[props(into, default = Reactive::of(|| Color::TRANSPARENT))]
    pub color: Reactive<Color>,
    /// What the icon means, for a screen reader. Unset, the icon is decoration and assistive technology skips it; set, it is a picture by that name. Give it one when the icon is the only thing saying something, such as a button with no text.
    #[props(into, default)]
    pub label: Reactive<String>,
}

/// Artwork ready to draw, and whether it takes the colour around it.
///
/// `monochrome` is decided where the icon was resolved, by [`icons_core::is_monochrome`]: from the set's `palette` when it declares one, else from whether the SVG paints in `currentColor`. The renderer tints flat — every paint of the artwork becomes the one colour — so only a monochrome icon is tinted unasked.
#[derive(Clone)]
pub(crate) struct Glyph {
    pub svg: Arc<SvgData>,
    pub monochrome: bool,
}

/// Draws the icon `name` in a `size`-px square, a monochrome one tinted like the text around it.
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
        move || {
            let chosen = color.get();
            if chosen != Color::TRANSPARENT {
                return Some(chosen);
            }
            if !glyph()?.monochrome {
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
        move || glyph().map_or_else(|| Arc::clone(&EMPTY), |glyph| glyph.svg),
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
fn glyph(name: IconName) -> Rc<dyn Fn() -> Option<Glyph>> {
    match name {
        IconName::Baked {
            svg, monochrome, ..
        } => {
            let glyph = Glyph { svg, monochrome };
            Rc::new(move || Some(glyph.clone()))
        }
        IconName::Named(id) => Rc::new(move || crate::runtime::resolve(&id.get())),
    }
}

#[cfg(test)]
#[path = "icon_test.rs"]
mod tests;
