use layout_core::{LayoutError, LayoutStyle, NodeId};
use platform_core::Event;
use reactive_core::{Effect, effect, signal};
use renderer_core::RectStyle;
use ui_core::{
    Component, EventResult, LayoutItem, RenderNode, StyledContainer, box_item, mark_dirty,
    set_display,
};

/// Shows `default` normally and swaps to `revealed` while the pointer hovers the widget (mouse only — touch
/// never sets hover, matching the rest of the catalogue). Both children stay mounted; the hidden one is
/// collapsed out of flow via `display`, so the widget sizes to whichever is showing and the swap keeps each
/// child's state across reveals.
pub fn hover_reveal(
    default: Box<dyn LayoutItem>,
    revealed: Box<dyn LayoutItem>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let hovered = signal(false);
    let default_node = default.layout_node();
    let revealed_node = revealed.layout_node();

    let root = StyledContainer::new(
        LayoutStyle::new().flex_column(),
        |_| RectStyle::default(),
        vec![default, revealed],
    )?
    .on_hover({
        let hovered = hovered.clone();
        move |h| hovered.set(h)
    });

    let _effect = effect(move || {
        let h = hovered.get();
        set_display(default_node, !h);
        set_display(revealed_node, h);
        let _ = mark_dirty(default_node);
        let _ = mark_dirty(revealed_node);
    });

    Ok(box_item(HoverReveal { root, _effect }))
}

struct HoverReveal {
    root: StyledContainer,
    _effect: Effect,
}

impl LayoutItem for HoverReveal {
    fn layout_node(&self) -> NodeId {
        self.root.layout_node()
    }
}

impl Component for HoverReveal {
    fn view(&self) -> RenderNode {
        self.root.view()
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        self.root.on_event(event)
    }

    fn debug_name(&self) -> &'static str {
        "HoverReveal"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use layout_core::AvailableSpace;
    use platform_core::PointerSource;
    use ui_core::{Container, compute_layout, reset_layout_runtime, track_layout};

    fn boxed(w: f32, h: f32) -> Box<dyn LayoutItem> {
        box_item(Container::new(LayoutStyle::new().width(w).height(h), vec![]).unwrap())
    }

    fn relayout(node: NodeId) {
        compute_layout(
            node,
            AvailableSpace::Definite(400.0),
            AvailableSpace::Definite(400.0),
        )
        .unwrap();
    }

    #[test]
    fn hover_swaps_default_and_revealed() {
        reset_layout_runtime();
        let default = boxed(40.0, 20.0);
        let revealed = boxed(120.0, 60.0);
        let default_node = default.layout_node();
        let revealed_node = revealed.layout_node();
        let mut widget = hover_reveal(default, revealed).unwrap();
        let root = widget.layout_node();
        relayout(root);

        let default_rect = track_layout(default_node).unwrap();
        let revealed_rect = track_layout(revealed_node).unwrap();
        assert!(default_rect.get().height > 0.0, "default shows initially");
        assert_eq!(
            revealed_rect.get().height,
            0.0,
            "revealed is collapsed initially"
        );

        widget.on_event(&Event::PointerMoved {
            x: 10.0,
            y: 10.0,
            source: PointerSource::Mouse,
        });
        relayout(root);
        assert_eq!(default_rect.get().height, 0.0, "default collapses on hover");
        assert!(revealed_rect.get().height > 0.0, "revealed shows on hover");

        widget.on_event(&Event::CursorLeft);
        relayout(root);
        assert!(default_rect.get().height > 0.0, "default returns on leave");
        assert_eq!(
            revealed_rect.get().height,
            0.0,
            "revealed collapses on leave"
        );
    }
}
