//! [`SurfaceStyle`]: the per-instance amendment a component accepts for its principal surface, and [`amend_surface`] to apply it.

use std::rc::Rc;

use renderer_core::RectStyle;

/// An amendment to the paint a component worked out for its **principal surface** — the one a caller means when they point at the control: a button's box, a menu's trigger, a tooltip's bubble.
///
/// It takes the finished [`RectStyle`] and hands back another, rather than being a `radius` or a `fill` prop, and that is the whole point. A component resolves its surface *per state* — hovered, pressed, bordered, themed — so a prop naming one property would have to be threaded through every one of those branches and would still only cover the property it named. Amending the result composes with the states instead of competing with them: `|s| s.with_radius(BorderRadius::all(2.0))` re-rounds the hovered style too, and nothing about the component's own logic has to know it happened.
///
/// This is the per-instance half of styling. The other two halves already exist and are not this: **theme tokens** for what a whole application should agree on (how round anything is), and **props** for what changes the shape rather than the paint (whether a menu wears a field's border). Reaching for this to say something every menu should say is how a design system comes apart one call site at a time.
///
/// A component crate accepts `@class` the way the catalogue does by taking this type for its surface prop and running its resolved style through [`amend_surface`].
pub type SurfaceStyle = Option<Rc<dyn Fn(RectStyle) -> RectStyle>>;

/// Applies a caller's amendment, if there is one.
pub fn amend_surface(style: RectStyle, over: &SurfaceStyle) -> RectStyle {
    match over {
        Some(f) => f(style),
        None => style,
    }
}
