//! Opening a URI from a page: a web page in a new tab, any other scheme handed to whatever the browser hands it to.

use platform_core::{RouteAddress, Uri};
use services_core::UriOpener;

/// Opens URIs with `window.open`, and the app's own addresses in a new tab beside the running one.
///
/// A web page opens beside the app with `noopener`, so the page it opens cannot reach back into this one; a `mailto:` or an app scheme is navigated to in place, which hands it to its handler without leaving the page. Only a [`Uri`] reaches it, so a scheme that would run in this page's origin (`javascript:`, `data:`) never does.
#[derive(Clone, Copy, Default, Debug)]
pub struct WebUriOpener;

impl UriOpener for WebUriOpener {
    fn open(&self, uri: &Uri) -> bool {
        let target = if uri.is_web() { "_blank" } else { "_self" };
        open_window(uri.as_str(), target)
    }

    fn open_beside(&self, address: &RouteAddress) -> bool {
        open_window(address.as_str(), "_blank")
    }
}

fn open_window(address: &str, target: &str) -> bool {
    crate::dom::window()
        .open_with_url_and_target_and_features(address, target, "noopener")
        .is_ok()
}
