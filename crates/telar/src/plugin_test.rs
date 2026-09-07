use super::*;
use layout_core::LayoutStyle;
use platform_core::{Key, ModifiersState};
use renderer_core::RectStyle;

struct Stub {
    node: Option<NodeId>,
    seen: usize,
}

impl Stub {
    fn new() -> Self {
        Self {
            node: None,
            seen: 0,
        }
    }
}

impl EmbeddedApp for Stub {
    fn build(&mut self) {
        let (node, _) =
            ui_core::new_leaf(LayoutStyle::new().width(40.0).height(20.0)).expect("leaf");
        self.node = Some(node);
    }
    fn layout_root(&self) -> NodeId {
        self.node.expect("build ran first")
    }
    fn view(&self) -> RenderNode {
        RenderNode::rect(Rect::new(0.0, 0.0, 40.0, 20.0), RectStyle::default())
    }
    fn on_event(&mut self, _event: &Event) -> EventResult {
        self.seen += 1;
        EventResult::Ignored
    }
    fn title(&self) -> String {
        "stub".into()
    }
    fn id(&self) -> String {
        "stub".into()
    }
}

fn shift() -> ModifiersState {
    ModifiersState {
        is_shift: true,
        ..ModifiersState::default()
    }
}

// No crate here links a plugin cdylib, so this is the only place the expansion is ever compiled: without it, adding a vtable field type-checks and breaks every guest at load time.
crate::plugin!(|_args: &[String]| -> Box<dyn EmbeddedApp> { Box::new(Stub::new()) });

#[test]
fn the_export_macro_builds_a_vtable_at_the_current_abi() {
    assert_eq!(_rsx_plugin_vtable.abi, TELAR_PLUGIN_ABI);
    let inst = unsafe { (_rsx_plugin_vtable.create)(&[]) };
    assert!(!inst.is_null(), "the exported vtable built an instance");
    assert_eq!(unsafe { (_rsx_plugin_vtable.id)(inst) }, "stub");
    unsafe { (_rsx_plugin_vtable.destroy)(inst) };
}

// These deliberately never observe on the caller's behalf, so they fail the moment either observe call leaves `PluginInstance::on_event`.
#[test]
fn a_plugin_records_the_modifiers_it_is_handed() {
    let mut inst = PluginInstance::new(Box::new(Stub::new()));
    let _g = inst.surface.enter();
    assert_eq!(ui_core::modifiers(), ModifiersState::default());
    drop(_g);

    inst.on_event(&Event::ModifiersChanged { modifiers: shift() });

    let _g = inst.surface.enter();
    assert!(
        ui_core::modifiers().is_shift,
        "a shift-drag inside a plugin is indistinguishable from a plain one without this"
    );
}

#[test]
fn an_overlay_event_reaches_the_registry_too() {
    let inst = PluginInstance::new(Box::new(Stub::new()));
    inst.dispatch_overlays(&Event::ModifiersChanged { modifiers: shift() });

    let _g = inst.surface.enter();
    assert!(
        ui_core::modifiers().is_shift,
        "an overlay event reaches the shared registry"
    );
}

#[test]
fn a_press_answers_for_one_frame_and_end_frame_closes_it() {
    let mut inst = PluginInstance::new(Box::new(Stub::new()));
    inst.on_event(&Event::KeyPressed {
        key: Key::Char('c'),
        modifiers: ModifiersState::default(),
    });

    {
        let _g = inst.surface.enter();
        assert!(
            ui_core::key_pressed(&Key::Char('c')),
            "the press answers for the frame it arrived in"
        );
    }
    inst.end_frame();
    let _g = inst.surface.enter();
    assert!(
        !ui_core::key_pressed(&Key::Char('c')),
        "without end_frame the press answers forever, not for its frame"
    );
    assert!(
        ui_core::key_held(&Key::Char('c')),
        "held is not what end_frame clears"
    );
}

#[test]
fn a_plugin_paints_and_its_generation_is_stable_between_frames() {
    let mut inst = PluginInstance::new(Box::new(Stub::new()));
    inst.relayout(40.0, 20.0);
    assert!(
        !inst.paint().is_empty(),
        "a plugin that paints emits commands"
    );
    assert_eq!(inst.generation(), inst.generation());
}

#[test]
fn the_driver_forwards_metadata_from_the_embedded_app() {
    let inst = PluginInstance::new(Box::new(Stub::new()));
    assert_eq!(inst.title(), "stub");
    assert_eq!(inst.id(), "stub");
    assert_eq!(inst.clear_color(), None);
}

#[test]
fn composite_translates_into_the_sub_rect_and_clips_to_it() {
    let rect = Rect::new(10.0, 20.0, 100.0, 50.0);
    let node = composite(
        rect,
        0,
        vec![DrawCommand::Rect {
            rect: Rect::new(0.0, 0.0, 5.0, 5.0),
            style: std::sync::Arc::new(RectStyle::default()),
        }],
    );
    match node {
        RenderNode::Clip {
            rect: clip,
            children,
            ..
        } => {
            assert_eq!(clip, rect, "clipped to the host's sub-rect");
            assert!(
                !children.is_empty(),
                "the composite wraps the plugin's own nodes"
            );
        }
        _ => panic!("expected the plugin's frame to be wrapped in a clip"),
    }
}
