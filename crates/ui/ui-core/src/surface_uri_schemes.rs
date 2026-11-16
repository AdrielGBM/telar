//! The [`UriSchemes`] a surface's links go to, kept in that surface's own worlds.
//!
//! Per surface, as its font family is: a runner hands each surface it shows the schemes its app's configuration states, a host hands the surface it draws a guest on the schemes of its own, and a canvas takes them from its [`SurfaceEnv`](crate::SurfaceEnv).

use platform_core::UriSchemes;

reactive_core::surface_local! {
    /// The active surface's URI schemes.
    slot URI_SCHEMES: UriSchemes = UriSchemes::default();
    access with_uri_schemes, with_uri_schemes_ref;
    context SurfaceUriSchemesContext;
}

/// Puts the active surface on `schemes`: an external link built on it goes only to a scheme they allow, and following one opens it only then. Whatever hosts a tree calls this before building it, as a runner does with its app's `AppConfig::uri_schemes`; until then a surface goes to the defaults alone.
pub fn set_surface_uri_schemes(schemes: UriSchemes) {
    with_uri_schemes(|held| *held = schemes);
}

/// The schemes the active surface was put on: a value, so code that opens a URI on a thread of its own is handed them and judges alike there.
pub fn surface_uri_schemes() -> UriSchemes {
    with_uri_schemes_ref(UriSchemes::clone)
}
