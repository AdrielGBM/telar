const MAX_EMBEDDED_IMAGE_BYTES: usize = 8 * 1024 * 1024;

// SVG comes from untrusted sources, and usvg's default string resolver would read any href that names a local file.
pub(crate) fn usvg_options() -> &'static usvg::Options<'static> {
    static OPTIONS: std::sync::OnceLock<usvg::Options<'static>> = std::sync::OnceLock::new();
    OPTIONS.get_or_init(|| {
        let default_data = usvg::ImageHrefResolver::default_data_resolver();
        usvg::Options {
            image_href_resolver: usvg::ImageHrefResolver {
                resolve_data: Box::new(move |mime, data, opts| {
                    if data.len() > MAX_EMBEDDED_IMAGE_BYTES {
                        None
                    } else {
                        default_data(mime, data, opts)
                    }
                }),
                resolve_string: Box::new(|_: &str, _: &usvg::Options| None),
            },
            ..usvg::Options::default()
        }
    })
}

#[cfg(test)]
#[path = "options.test.rs"]
mod tests;
