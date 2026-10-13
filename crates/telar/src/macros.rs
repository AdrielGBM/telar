//! The macros an application writes: `app!`, `rsx_modules!` and the tree shims they expand to.

/// Collects widgets of different types into the `Vec<Box<dyn LayoutItem>>` a container takes, boxing each one.
#[macro_export]
macro_rules! children {
    ($($item:expr),* $(,)?) => {
        vec![$($crate::box_item($item)),*]
    }
}

/// Caches an `Arc<str>` per call site so a string literal allocates at most once instead of once per frame.
///
/// A static rather than a thread-local: one with a destructor, expanded into a hot-reloaded app, would keep that build mapped after the host unloads it.
#[macro_export]
macro_rules! static_rc_str {
    ($s:literal) => {{
        static V: ::std::sync::LazyLock<::std::sync::Arc<str>> =
            ::std::sync::LazyLock::new(|| ::std::sync::Arc::from($s as &str));
        ::std::sync::Arc::clone(&*V)
    }};
}
