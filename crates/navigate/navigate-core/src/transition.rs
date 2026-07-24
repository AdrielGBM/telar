/// How a [`NavHost`](crate::NavHost) animates the incoming page when navigation changes the current route.
///
/// The animation is one-sided — it moves only the incoming page over the host background — so it never needs
/// two pages laid out at once (which a flex container would stack, not overlap). The outgoing page is hidden
/// immediately; the incoming one animates from an offset/transparent start to its resting identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NavTransition {
    /// Instant swap (no animation). The default.
    #[default]
    None,
    /// The incoming page fades in (opacity 0 → 1).
    Fade,
    /// The incoming page slides in horizontally — from the right on a forward push, from the left on a back
    /// pop — to its resting position.
    SlideHorizontal,
}

impl NavTransition {
    pub(crate) fn is_animated(self) -> bool {
        !matches!(self, NavTransition::None)
    }
}
