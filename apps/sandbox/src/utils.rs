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
