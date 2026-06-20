use geometry_core::Rect;
use platform_core::Event;
use renderer_core::BorderRadius;
use ui_tree::{Component, EventResult, NodeVec, RenderNode};

use crate::pointer::{
    clip_pointer_event, clip_pointer_event_rounded, dispatch_to_children, transform_pointer,
};

pub struct Group {
    clip: Option<(Box<dyn Fn() -> Rect>, BorderRadius)>,
    matrix: Option<Box<dyn Fn() -> [f32; 6]>>,
    children: Vec<Box<dyn Component>>,
}

impl Group {
    pub fn clip(rect: impl Fn() -> Rect + 'static, children: Vec<Box<dyn Component>>) -> Self {
        Self {
            clip: Some((Box::new(rect), BorderRadius::zero())),
            matrix: None,
            children,
        }
    }

    pub fn clip_rounded(
        rect: impl Fn() -> Rect + 'static,
        radius: BorderRadius,
        children: Vec<Box<dyn Component>>,
    ) -> Self {
        Self {
            clip: Some((Box::new(rect), radius)),
            matrix: None,
            children,
        }
    }

    pub fn transform(
        matrix: impl Fn() -> [f32; 6] + 'static,
        children: Vec<Box<dyn Component>>,
    ) -> Self {
        Self {
            clip: None,
            matrix: Some(Box::new(matrix)),
            children,
        }
    }
}

impl Component for Group {
    fn view(&self) -> RenderNode {
        let children = NodeVec::collect(self.children.iter().map(|c| c.view()));
        match (&self.clip, &self.matrix) {
            (Some((rect_fn, radius)), None) => RenderNode::Clip {
                rect: (rect_fn)(),
                radius: *radius,
                children,
            },
            (None, Some(matrix_fn)) => RenderNode::Transform {
                matrix: (matrix_fn)(),
                children,
            },
            (Some((rect_fn, radius)), Some(matrix_fn)) => RenderNode::Clip {
                rect: (rect_fn)(),
                radius: *radius,
                children: NodeVec::collect([RenderNode::Transform {
                    matrix: (matrix_fn)(),
                    children,
                }]),
            },
            (None, None) => unreachable!("Group requires at least clip or matrix"),
        }
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        let after_clip = match &self.clip {
            Some((rect_fn, radius)) if radius.is_zero() => clip_pointer_event(event, (rect_fn)()),
            Some((rect_fn, radius)) => clip_pointer_event_rounded(event, (rect_fn)(), *radius),
            None => Some(event),
        };
        let Some(event) = after_clip else {
            return EventResult::Ignored;
        };

        let transformed = match &self.matrix {
            Some(matrix_fn) => transform_pointer(event, (matrix_fn)()),
            None => None,
        };
        let event = transformed.as_ref().unwrap_or(event);

        dispatch_to_children(&mut self.children, event)
    }
}
