use std::time::Duration;

use motion_core::{Animated, Easing, tween};
use ui_core::RenderNode;

const TRANSITION_MS: u64 = 220;

/// How a [`NavHost`](crate::NavHost) animates the incoming page when navigation changes the current route,
/// and how a [`TabHost`](crate::TabHost) animates the incoming tab when one is selected.
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

    /// A fresh entrance animation driving `0 → 1`, or `None` when this transition is instant.
    pub(crate) fn start(self) -> Option<Animated<f32>> {
        self.is_animated().then(|| {
            let anim = Animated::new(
                0.0,
                tween(Duration::from_millis(TRANSITION_MS), Easing::EaseOut),
            );
            anim.retarget(1.0);
            anim
        })
    }

    /// Wraps the incoming page's render tree at `progress`. `forward` picks which side a slide enters from,
    /// and `width` is the host's laid-out width — the distance that slide travels.
    pub(crate) fn wrap(
        self,
        child: RenderNode,
        progress: f32,
        forward: bool,
        width: f32,
    ) -> RenderNode {
        let p = progress.clamp(0.0, 1.0);
        match self {
            NavTransition::None => child,
            NavTransition::Fade => RenderNode::layer(p, 0.0, [child]),
            NavTransition::SlideHorizontal => {
                let dir = if forward { 1.0 } else { -1.0 };
                let dx = dir * width * (1.0 - p);
                RenderNode::transform_with([1.0, 0.0, 0.0, 1.0, dx, 0.0], [child])
            }
        }
    }
}
