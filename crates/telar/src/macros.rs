//! The macros an application writes: `app!`, `rsx_modules!` and the tree shims they expand to.

/// Collects widgets of different types into the `Vec<Box<dyn LayoutItem>>` a container takes, boxing each one.
#[macro_export]
macro_rules! children {
    ($($item:expr),* $(,)?) => {
        vec![$($crate::box_item($item)),*]
    }
}

/// Caches an `Arc<str>` per call site in thread-local storage so a string literal allocates at most once per thread instead of once per frame.
#[macro_export]
macro_rules! static_rc_str {
    ($s:literal) => {{
        thread_local! {
            static V: ::std::sync::Arc<str> = ::std::sync::Arc::from($s as &str);
        }
        V.with(::std::sync::Arc::clone)
    }};
}
