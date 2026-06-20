use rsx::{Container, LayoutError, LayoutItem, LayoutStyle, WidgetCtx};

/// Returns a per-call-site cached `Arc<str>` for a string literal, allocating at most once per thread.
#[macro_export]
macro_rules! static_rc_str {
    ($s:literal) => {{
        thread_local! {
            static V: std::sync::Arc<str> = std::sync::Arc::from($s as &str);
        }
        V.with(std::sync::Arc::clone)
    }};
}

pub fn row_gap(
    ctx: &mut WidgetCtx,
    gap: f32,
    children: Vec<Box<dyn LayoutItem>>,
) -> Result<Container, LayoutError> {
    Container::new(ctx, LayoutStyle::new().flex_row().gap(gap), children)
}

pub fn col_gap(
    ctx: &mut WidgetCtx,
    gap: f32,
    children: Vec<Box<dyn LayoutItem>>,
) -> Result<Container, LayoutError> {
    Container::new(ctx, LayoutStyle::new().flex_column().gap(gap), children)
}
