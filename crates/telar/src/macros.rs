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
/// A released static rather than a thread-local, which with a destructor would keep a hot-reloaded build mapped after its host unloads it, or a `LazyLock`, whose string that host would never free.
#[macro_export]
macro_rules! static_rc_str {
    ($s:literal) => {{
        $crate::released_static! {
            static V: ::std::sync::Arc<str> = ::std::sync::Arc::from($s as &str);
        }
        V.get()
    }};
}
