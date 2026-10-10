use telar::inspect::a11y::Severity;
use telar::testing::{key_with, press, release};
use telar::{
    AccessAudit, AccessRule, AccessViolation, Color, DevOverlay, DrawCommand, Insets, Key,
    ModifiersState, Rect, SegmentNodeInfo, Size,
};

use crate::{Built, DevTools};

/// Windows from roomy to the narrowest the drawer and the panel still sit side by side in.
const WINDOWS: [Size; 2] = [Size::new(1000.0, 700.0), Size::new(600.0, 500.0)];
/// An application drawing the lightest and the darkest it can under the overlay.
const APP_BACKGROUNDS: [Color; 2] = [Color::WHITE, Color::BLACK];
const ERROR: &str = "error[E0425]: cannot find value `count` in this scope\n --> src/app.rs:12:9";

fn chord(key: char) -> telar::Event {
    let modifiers = ModifiersState {
        is_ctrl: true,
        is_shift: true,
        ..ModifiersState::default()
    };
    key_with(Key::Char(key), modifiers)
}

fn node(id: u64, name: &'static str, depth: usize, rect: Rect) -> SegmentNodeInfo {
    SegmentNodeInfo {
        id,
        name,
        depth,
        rect,
        padding: Insets::default(),
        margin: Insets::default(),
        border: Insets::default(),
        gap: Size::ZERO,
    }
}

/// What a reader is told about the overlay in `window` over an app drawing `background`, and what checking it finds.
fn audit(dev: &mut DevTools, window: Size, background: Color) -> AccessAudit {
    let frame = dev
        .on_frame(&[], window.width, window.height, false)
        .into_owned();
    let Built::Ready(overlay) = &dev.overlay else {
        panic!("the overlay builds")
    };
    let _entered = overlay.canvas.enter();
    telar::inspect::audit(&frame, Some(background))
}

fn click(dev: &mut DevTools, window: Size, text: &str) {
    let frame = dev
        .on_frame(&[], window.width, window.height, false)
        .into_owned();
    let mut found = None;
    telar::for_each_with_matrix(&frame, |command, matrix| {
        if let DrawCommand::Text {
            text: drawn, rect, ..
        } = command
            && **drawn == *text
            && found.is_none()
        {
            found = Some(telar::transform_clip_rect(matrix, *rect));
        }
    });
    let rect = found.unwrap_or_else(|| panic!("{text:?} is not drawn"));
    let (x, y) = (rect.x + rect.width / 2.0, rect.y + rect.height / 2.0);
    dev.on_event(&press(x, y));
    dev.on_event(&release(x, y));
}

fn assert_passes(dev: &mut DevTools, window: Size, shown: &str) {
    for background in APP_BACKGROUNDS {
        let audit = audit(dev, window, background);
        assert!(
            audit.nodes.iter().any(|node| node.name == "Devtools panel"),
            "the audit reads the overlay: {:#?}",
            audit.nodes
        );
        let errors: Vec<_> = audit
            .violations
            .iter()
            .filter(|violation| violation.severity == Severity::Error)
            .map(ToString::to_string)
            .collect();
        assert!(
            errors.is_empty(),
            "{shown} in a {}×{} window over {background:?}:\n{}",
            window.width,
            window.height,
            errors.join("\n")
        );
    }
}

/// The overlay passes the checks it runs on an application, in every state it can be in: the badge alone, the panel, the inspector on either tab, and the build-error banner.
#[test]
fn the_overlay_passes_its_own_accessibility_checks() {
    for window in WINDOWS {
        telar::install_default_text_metrics();
        let mut dev = DevTools::default();
        dev.on_tree(&[
            node(
                0,
                "AppRoot",
                0,
                Rect::new(0.0, 0.0, window.width, window.height),
            ),
            node(1, "SaveButton", 1, Rect::new(400.0, 100.0, 120.0, 32.0)),
        ]);
        assert_passes(&mut dev, window, "the badge");

        dev.on_event(&chord('d'));
        assert_passes(&mut dev, window, "the panel");

        dev.on_event(&chord('i'));
        click(&mut dev, window, "SaveButton");
        assert_passes(&mut dev, window, "the components tab");

        click(&mut dev, window, "Accessibility");
        dev.on_violations(&[AccessViolation::new(
            AccessRule::UnnamedControl,
            "a button has no name, so a reader announces only its role",
            Some(Rect::new(400.0, 100.0, 120.0, 32.0)),
        )]);
        assert_passes(&mut dev, window, "the accessibility tab");

        dev.set_build_error(Some(ERROR.to_owned()));
        assert_passes(&mut dev, window, "the build-error banner");
    }
}
