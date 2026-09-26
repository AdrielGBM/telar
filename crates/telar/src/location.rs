//! The app's address, from the application's side.

/// One back as the user means it: closes the frontmost dialog or drawer if one is open, and otherwise steps the app's history back. `false` when there was nothing to go back from.
///
/// What Android's back button, a mouse's back button and a desktop `BrowserBack` key do, and what an app's own back affordance should do too, so every way of saying back agrees.
pub fn navigate_back() -> bool {
    ui_core::dismiss::dismiss_top() || platform_core::history_back()
}
