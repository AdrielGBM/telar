use std::cell::RefCell;

use telar::testing::{key_with, named, press, release, route};
use telar::{
    AvailableSpace, ComponentList, Event, NamedKey, compute_layout, focus, new_container,
    relayout_if_dirty,
};

use super::*;

/// Within a step or two of eight bits a channel: a press lands on a pixel, not on the exact edge of the area.
fn close(a: Color, b: Color) -> bool {
    a.to_rgba8()
        .iter()
        .zip(b.to_rgba8())
        .all(|(x, y)| x.abs_diff(y) <= 2)
}

#[test]
fn hsv_and_rgb_name_the_same_colours() {
    let red = Color::rgb(1.0, 0.0, 0.0);
    assert_eq!(to_hsv(red), (Some(0.0), 1.0, 1.0));
    let green = from_hsv(
        Hsv {
            h: 120.0,
            s: 1.0,
            v: 1.0,
        },
        1.0,
    );
    assert!(close(green, Color::rgb(0.0, 1.0, 0.0)));
    for color in [
        Color::from_rgb_u8(51, 102, 153),
        Color::from_rgb_u8(230, 25, 128),
        Color::from_rgb_u8(77, 77, 25),
    ] {
        let (h, s, v) = to_hsv(color);
        let back = from_hsv(
            Hsv {
                h: h.unwrap(),
                s,
                v,
            },
            1.0,
        );
        assert_eq!(
            back.to_rgba8(),
            color.to_rgba8(),
            "{color:?} came back {back:?}"
        );
    }
}

#[test]
fn a_grey_has_no_hue_of_its_own() {
    assert_eq!(to_hsv(Color::rgb(0.5, 0.5, 0.5)).0, None);
    let shown = Hsv {
        h: 200.0,
        s: 0.7,
        v: 0.4,
    };
    let grey = Picker::hsv_keeping(Color::rgb(0.5, 0.5, 0.5), shown);
    assert_eq!((grey.h, grey.s), (200.0, 0.0), "the hue stays where it was");
    let black = Picker::hsv_keeping(Color::BLACK, shown);
    assert_eq!(
        (black.h, black.s, black.v),
        (200.0, 0.7, 0.0),
        "black keeps the hue and the saturation"
    );
}

#[test]
fn hex_carries_the_alpha_only_when_it_is_shown_and_not_opaque() {
    assert_eq!(hex_of(Color::rgb(1.0, 0.0, 0.0), true), "#ff0000");
    assert_eq!(hex_of(Color::rgba(1.0, 0.0, 0.0, 0.5), true), "#ff000080");
    assert_eq!(hex_of(Color::rgba(1.0, 0.0, 0.0, 0.5), false), "#ff0000");
}

struct Mounted {
    tree: ComponentList,
    value: RwSignal<Color>,
    changes: Rc<RefCell<Vec<Color>>>,
}

impl Mounted {
    fn new(start: Color, alpha: bool) -> Self {
        crate::test_support::fresh_layout_runtime();
        telar::set_locale("en");
        ui_core::reset_keyboard();
        focus::clear();
        let value = signal(start);
        let changes = Rc::new(RefCell::new(Vec::new()));
        let sink = changes.clone();
        let picker = color_picker(
            ColorPickerProps::props()
                .value(value)
                .alpha(alpha)
                .swatches(vec![Color::rgb(1.0, 0.0, 0.0), Color::rgb(0.0, 0.0, 1.0)])
                .swatch_names(vec!["Red".to_string(), "Blue".to_string()])
                .on_change(Rc::new(move |color| sink.borrow_mut().push(color)))
                .build(),
            Children::default(),
        )
        .unwrap();
        let root = new_container(
            LayoutStyle::new().flex_column().width(400.0).height(500.0),
            &[picker.layout_node()],
        )
        .unwrap();
        compute_layout(
            root,
            AvailableSpace::Definite(400.0),
            AvailableSpace::Definite(500.0),
        )
        .unwrap();
        let mut mounted = Self {
            tree: ComponentList::new(picker),
            value,
            changes,
        };
        mounted.settle();
        mounted
    }

    fn settle(&mut self) {
        relayout_if_dirty();
        let _ = self.tree.commands();
    }

    fn send(&mut self, event: Event) {
        route(&mut self.tree, &event);
        self.settle();
    }

    fn node(&self, role: Role, name: &str) -> Option<telar::AccessNode> {
        ui_core::accessibility::snapshot(&self.tree.commands())
            .into_iter()
            .find(|node| node.role == role && node.name == name)
    }

    fn rect(&self, role: Role, name: &str) -> Rect {
        self.node(role, name)
            .unwrap_or_else(|| panic!("a {role:?} named `{name}`"))
            .rect
    }

    fn focus(&mut self, role: Role, name: &str) {
        let id = self
            .node(role, name)
            .and_then(|node| node.id)
            .expect("a focusable control");
        focus::request(id);
        self.settle();
    }

    fn click(&mut self, x: f32, y: f32) {
        self.send(press(x as f64, y as f64));
        self.send(release(x as f64, y as f64));
    }

    fn key(&mut self, key: NamedKey) {
        self.send(named(key));
    }
}

#[test]
fn pressing_the_area_sets_saturation_across_and_brightness_up() {
    let mut mounted = Mounted::new(Color::rgb(1.0, 0.0, 0.0), true);
    let area = mounted.rect(Role::Slider, "Saturation and brightness");
    mounted.click(area.x + area.width - 0.5, area.y + 0.5);
    assert!(close(mounted.value.get(), Color::rgb(1.0, 0.0, 0.0)));

    mounted.click(area.x + 0.5, area.y + 0.5);
    assert!(
        close(mounted.value.get(), Color::WHITE),
        "{:?}",
        mounted.value.get()
    );
    mounted.click(area.x + area.width / 2.0, area.y + area.height - 0.5);
    assert!(close(mounted.value.get(), Color::BLACK));
    assert!(!mounted.changes.borrow().is_empty());
}

#[test]
fn the_arrows_step_the_area_and_the_sliders() {
    let mut mounted = Mounted::new(Color::rgb(0.5, 0.25, 0.25), true);
    mounted.focus(Role::Slider, "Saturation and brightness");
    mounted.key(NamedKey::ArrowUp);
    let (_, _, v) = to_hsv(mounted.value.get());
    assert!((v - 0.51).abs() < 0.005, "{v}");

    mounted.focus(Role::Slider, "Hue");
    mounted.key(NamedKey::End);
    let (h, _, _) = to_hsv(mounted.value.get());
    assert!(
        h.unwrap() < 1.0 || h.unwrap() > 359.0,
        "the end of the hue is red again: {h:?}"
    );
    mounted.key(NamedKey::Home);
    mounted.key(NamedKey::ArrowRight);
    let (h, _, _) = to_hsv(mounted.value.get());
    assert!((h.unwrap() - 3.6).abs() < 0.5, "{h:?}");

    mounted.focus(Role::Slider, "Opacity");
    mounted.key(NamedKey::Home);
    assert_eq!(mounted.value.get().a, 0.0);
}

#[test]
fn a_typed_hex_is_applied_on_enter_and_rejected_when_it_does_not_parse() {
    let mut mounted = Mounted::new(Color::rgb(1.0, 0.0, 0.0), true);
    mounted.focus(Role::TextInput, "Hex");
    mounted.send(key_with(
        Key::Char('a'),
        ModifiersState {
            is_ctrl: true,
            ..Default::default()
        },
    ));
    for c in "#00ff00".chars() {
        mounted.send(key_with(Key::Char(c), ModifiersState::default()));
    }
    mounted.key(NamedKey::Enter);
    assert!(close(mounted.value.get(), Color::rgb(0.0, 1.0, 0.0)));

    mounted.send(key_with(
        Key::Char('a'),
        ModifiersState {
            is_ctrl: true,
            ..Default::default()
        },
    ));
    for c in "nope".chars() {
        mounted.send(key_with(Key::Char(c), ModifiersState::default()));
    }
    mounted.key(NamedKey::Enter);
    assert!(close(mounted.value.get(), Color::rgb(0.0, 1.0, 0.0)));
}

#[test]
fn a_swatch_picks_its_colour_and_the_swatches_follow_the_value() {
    let mut mounted = Mounted::new(Color::rgb(0.2, 0.2, 0.2), true);
    assert_eq!(
        mounted.node(Role::Radio, "Blue").unwrap().toggled,
        Some(false)
    );
    let blue = mounted.rect(Role::Radio, "Blue");
    mounted.click(blue.x + blue.width / 2.0, blue.y + blue.height / 2.0);
    assert!(close(mounted.value.get(), Color::rgb(0.0, 0.0, 1.0)));

    mounted.value.set(Color::rgb(1.0, 0.0, 0.0));
    mounted.settle();
    assert_eq!(
        mounted.node(Role::Radio, "Red").unwrap().toggled,
        Some(true)
    );
}

#[test]
fn without_alpha_there_is_no_opacity_slider() {
    let mounted = Mounted::new(Color::rgb(0.2, 0.2, 0.2), false);
    assert!(mounted.node(Role::Slider, "Opacity").is_none());
    assert!(mounted.node(Role::Slider, "Hue").is_some());
}
