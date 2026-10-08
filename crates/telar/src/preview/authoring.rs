//! Previews written in Rust: [`preview!`](crate::preview::preview!) and what its body may return.

use crate::{LayoutError, LayoutItem, box_item};

/// What a preview's body returns: the `Result` a component call or a widget constructor returns, boxed as the canvas's root.
#[diagnostic::on_unimplemented(
    message = "a preview's body returns `{Self}`, which is not a tree",
    label = "expected `Result<impl LayoutItem, LayoutError>`",
    note = "a component call returns one: `tag(TagProps::props().build(), Children::default())`"
)]
pub trait IntoPreviewRoot {
    fn into_preview_root(self) -> Result<Box<dyn LayoutItem>, LayoutError>;
}

impl<T: LayoutItem + 'static> IntoPreviewRoot for Result<T, LayoutError> {
    fn into_preview_root(self) -> Result<Box<dyn LayoutItem>, LayoutError> {
        self.map(box_item)
    }
}

/// A preview written in Rust: one variant of a component, as a [`Preview`](crate::preview::Preview) to go on configuring with its setters.
///
/// ```ignore
/// use telar::preview::{Layout, Preview, preview};
///
/// pub(crate) fn previews() -> Vec<Preview> {
///     vec![
///         preview!(button: ButtonProps, "Primary", |p| {
///             button(
///                 ButtonProps::props()
///                     .label(p.arg("label", "Save"))
///                     .ghost(p.arg("ghost", false))
///                     .build(),
///                 Children::default(),
///             )
///         })
///         .title("Inputs/Button")
///         .layout(Layout::Centered),
///         preview!(button: ButtonProps, "Counting presses", |p| {
///             let pressed = p.signal("pressed", 0u32);
///             button(
///                 ButtonProps::props()
///                     .label("Press me")
///                     .on_press(Rc::new(move || pressed.update(|n| *n += 1)))
///                     .build(),
///                 Children::default(),
///             )
///         }),
///     ]
/// }
/// ```
///
/// - **`button: ButtonProps`** names the component under preview and its props, whose [`PropsSchema`](crate::preview::PropsSchema) feeds the docs and refines the control of each arg named after a prop. `: ButtonProps` may be left out for a component with no derived props.
/// - **`"Primary"`** names the variant. The id is `<crate>--button--primary`, slugged as a `.rsx` preview's is, so a name with no letter or digit is a compile error.
/// - **`|p| body`** builds the tree. `p` is the canvas's [`PreviewCtx`](crate::preview::PreviewCtx): [`p.arg`](crate::preview::PreviewCtx::arg) reads a value its control can change, building the preview again when it does, and [`p.signal`](crate::preview::PreviewCtx::signal) shares a signal with its control. The body returns `Result<impl LayoutItem, LayoutError>`, as a component call does, and may use `?`; it is compiled as a fn, so it cannot capture anything.
///
/// The entry also records the absolute path of the file it is written in, its line and the body's source text.
///
/// Write previews, and the `pub fn telar_all_previews() -> Vec<Preview>` that lists a crate's, inside [`crate::__previews!`], so a build without the `previews` feature compiles none of them.
#[macro_export]
#[doc(hidden)]
macro_rules! __preview_entry {
    ($tag:ident $(: $props:ty)?, $name:literal, |$ctx:pat_param| $body:expr $(,)?) => {{
        let (id, component) = $crate::preview::__preview_names!($tag, $name);
        $crate::preview::PreviewEntry::new(id, component, $name, {
            fn __telar_preview_build(
                ctx: &$crate::preview::PreviewCtx,
            ) -> ::core::result::Result<
                ::std::boxed::Box<dyn $crate::LayoutItem>,
                $crate::LayoutError,
            > {
                let $ctx = ctx;
                $crate::preview::IntoPreviewRoot::into_preview_root($body)
            }
            __telar_preview_build
        })
        .location($crate::preview::__preview_file_of!($name), ::core::line!())
        .source($crate::preview::__preview_source!($body), &[])
        $(.props(<$props as $crate::preview::HasPropsSchema>::schema))?
    }};
}
