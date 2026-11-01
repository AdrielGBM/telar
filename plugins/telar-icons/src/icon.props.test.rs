//! What an icon's props are, checked by compile-fail doctests.

/// The paths the compile-fail block below uses resolve, and a name is said by the call rather than by a prop.
///
/// ```
/// use telar::Accessible;
/// use telar_icons::{IconProps, icon};
/// let _ = icon(IconProps::props().name("demo:home").build()).map(|item| item.a11y_label(|| "Home"));
/// ```
pub struct NamedByTheCall;

/// An icon has no `label` of its own: `label` is always text that is seen, and an icon draws none.
///
/// ```compile_fail,E0599
/// use telar_icons::{IconProps, icon};
/// let _ = icon(IconProps::props().name("demo:home").label("Home").build());
/// ```
pub struct HasNoLabelProp;
