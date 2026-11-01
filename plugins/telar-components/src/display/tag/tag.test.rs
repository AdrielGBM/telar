use std::cell::Cell;
use std::rc::Rc;

use telar::preview::a11y::Severity;
use telar::preview::{Play, PreviewCtx, PreviewEntry, by_role, by_text};
use telar::{
    Accessible, AvailableSpace, BorderRadius, Color, ColorScheme, ComponentList, DrawCommand,
    Event, Key, LayoutError, LayoutItem, LayoutStyle, NamedKey, Paint, PointerButton,
    PointerSource, Rect, RectStyle, Role, SurfaceAmendment, compute_layout, new_container,
    track_layout,
};

use crate::shared;
use crate::test_support::{find_text, first_painted, fresh_layout_runtime, painted_at};
use crate::{TagProps, TagTokens, tag};
use telar::testing::{centre, moved, press};
use telar::{ControlSize, Tone, Variant};

const TONES: [Tone; 6] = [
    Tone::Accent,
    Tone::Neutral,
    Tone::Info,
    Tone::Success,
    Tone::Warning,
    Tone::Danger,
];
const VARIANTS: [Variant; 4] = [
    Variant::Solid,
    Variant::Soft,
    Variant::Outline,
    Variant::Ghost,
];

fn in_a_row(item: Box<dyn LayoutItem>) -> ComponentList {
    let root = new_container(
        LayoutStyle::new().flex_row().width(200.0).height(60.0),
        &[item.layout_node()],
    )
    .unwrap();
    compute_layout(
        root,
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(60.0),
    )
    .unwrap();
    ComponentList::new(item)
}

fn mounted(props: TagProps) -> (ComponentList, Rect) {
    fresh_layout_runtime();
    let tag = tag(props).unwrap();
    let rect = track_layout(tag.layout_node()).unwrap();
    compute_layout(
        tag.layout_node(),
        AvailableSpace::MaxContent,
        AvailableSpace::MaxContent,
    )
    .unwrap();
    (ComponentList::new(tag), rect.get())
}

fn label_rect(tree: &ComponentList, label: &str) -> Rect {
    tree.commands()
        .iter()
        .find_map(|command| match command {
            DrawCommand::Text { text, rect, .. } if text.as_ref() == label => Some(*rect),
            _ => None,
        })
        .expect("the tag draws its label")
}

fn ink(tree: &ComponentList) -> Color {
    tree.commands()
        .iter()
        .find_map(|command| match command {
            DrawCommand::Text { style, .. } => Some(style.color.solid_color()),
            _ => None,
        })
        .expect("the tag draws its label")
}

fn drawn_rect(tree: &ComponentList) -> Rect {
    tree.commands()
        .iter()
        .find_map(|command| match command {
            DrawCommand::Rect { rect, .. } => Some(*rect),
            _ => None,
        })
        .expect("the tag paints a box")
}

fn solid(paint: Option<Paint>) -> Color {
    paint.map_or(Color::TRANSPARENT, |paint| paint.solid_color())
}

fn frame(style: &RectStyle) -> Color {
    solid(style.border.map(|border| border.paint))
}

/// The contrast the label has over the tag's fill laid on the surface.
fn contrast(tree: &ComponentList) -> f32 {
    let fill = solid(first_painted(tree).fill);
    ink(tree).contrast_ratio(shared::over(fill, shared::surface()))
}

fn in_scheme(scheme: ColorScheme) {
    let id = match scheme {
        ColorScheme::Dark => "tag-dark",
        _ => "tag-light",
    };
    telar::set_mode_scheme(id, scheme);
    telar::set_mode(id);
}

fn played(build: fn(&PreviewCtx) -> Result<Box<dyn LayoutItem>, LayoutError>) -> Play {
    fresh_layout_runtime();
    let entry = PreviewEntry::new("test--tag--behaviour", "tag", "behaviour", build).locale("en");
    Play::mount(&entry).unwrap_or_else(|error| panic!("the tag mounts: {error}"))
}

fn buttons(play: &Play) -> Vec<(String, Option<bool>)> {
    play.snapshot()
        .into_iter()
        .filter(|node| node.role == Role::Button)
        .map(|node| (node.name, node.toggled))
        .collect()
}

/// A control a caller brings to the tag's `action` slot: a button named `name`, a full pointer target, that runs `run`.
fn acting(name: &'static str, run: Rc<dyn Fn()>) -> telar::Children {
    telar::Children::one(move || {
        let run = run.clone();
        let button = telar::StyledContainer::new(
            LayoutStyle::new()
                .width(shared::MIN_TARGET)
                .height(shared::MIN_TARGET),
            |_| RectStyle::default(),
            vec![],
        )?
        .control(Role::Button)
        .on_press(move || run())
        .a11y_label(move || name);
        Ok(telar::box_item(button))
    })
}

fn static_tag(_: &PreviewCtx) -> Result<Box<dyn LayoutItem>, LayoutError> {
    tag(TagProps::props().label("Saved").tone(Tone::Success).build())
}

fn with_an_action(ctx: &PreviewCtx) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let actions = ctx.actions();
    tag(TagProps::props()
        .label("Logo")
        .action(acting(
            "Remove Logo",
            Rc::new(move || actions.log("remove", Vec::new())),
        ))
        .build())
}

fn bound(ctx: &PreviewCtx) -> Result<Box<dyn LayoutItem>, LayoutError> {
    tag(TagProps::props()
        .label("Images")
        .selected(ctx.signal("on", false))
        .build())
}

fn uncontrolled(ctx: &PreviewCtx) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let actions = ctx.actions();
    tag(TagProps::props()
        .label("Images")
        .on_toggle(Rc::new(move |on| {
            actions.log("on_toggle", vec![on.to_string()])
        }))
        .build())
}

fn bound_and_told(ctx: &PreviewCtx) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let actions = ctx.actions();
    tag(TagProps::props()
        .label("Images")
        .selected(ctx.signal("on", false))
        .on_toggle(Rc::new(move |on| {
            actions.log("on_toggle", vec![on.to_string()])
        }))
        .build())
}

fn filter_with_an_action(ctx: &PreviewCtx) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let on = ctx.signal("on", true);
    let actions = ctx.actions();
    tag(TagProps::props()
        .label("Images")
        .selected(on)
        .action(acting(
            "Remove Images",
            Rc::new(move || actions.log("remove", Vec::new())),
        ))
        .build())
}

#[test]
fn a_tag_draws_its_label() {
    fresh_layout_runtime();
    let tree = in_a_row(tag(TagProps::props().label("Saved").build()).unwrap());
    assert!(
        find_text(&tree.commands(), "Saved"),
        "a tag draws its label"
    );
}

#[test]
fn an_empty_label_builds_without_panic() {
    fresh_layout_runtime();
    let tree = in_a_row(tag(TagProps::props().build()).unwrap());
    let _ = tree.commands();
}

#[test]
fn the_corner_is_the_item_role_until_the_tag_tokens_name_one() {
    crate::test_support::assert_corner_follows_the_role_until_the_tokens_name_one(
        || mounted(TagProps::props().label("Beta").build()).0,
        |tree| first_painted(tree).radius,
        |tree, role| BorderRadius::all(role.min(drawn_rect(tree).height / 2.0)),
        |radius| TagTokens {
            radius: Some(radius),
            ..TagTokens::default()
        },
    );
}

#[test]
fn a_style_amendment_reshapes_the_tag() {
    let flat: SurfaceAmendment = Rc::new(|s: RectStyle| s.with_radius(BorderRadius::all(0.0)));
    let (tree, _) = mounted(TagProps::props().label("Beta").style(flat).build());
    let painted = first_painted(&tree);
    assert_eq!(painted.radius, BorderRadius::all(0.0));
    assert!(painted.fill.is_some(), "the fill is kept");
    assert!(painted.border.is_some(), "the frame is kept");
}

const SIZES: [ControlSize; 3] = [ControlSize::Small, ControlSize::Regular, ControlSize::Large];

/// Every length follows the size the tag is written at: the padding either side of its label, and the label's share of the text around it.
#[test]
fn the_padding_and_the_label_follow_the_size() {
    for size in SIZES {
        let (tree, rect) = mounted(TagProps::props().label("Beta").size(size).build());
        let padding = rect.width - label_rect(&tree, "Beta").width;
        let expected = 2.0 * shared::gap_md(size);
        assert!(
            (padding - expected).abs() <= 0.5,
            "{size:?} padded by {padding}, not {expected}, within the half pixel layout rounds the box by"
        );
        let font_size = tree
            .commands()
            .iter()
            .find_map(|command| match command {
                DrawCommand::Text { style, .. } => Some(style.font_size),
                _ => None,
            })
            .expect("the tag draws its label");
        let expected =
            telar::Inherited::initial().text.font_size * size.text_scale() * shared::CAPTION_RATIO;
        assert!(
            (font_size - expected).abs() < 0.01,
            "{size:?} draws its label at {font_size}, not {expected}"
        );
    }
}

/// A tag stands 20, 24 and 28 px tall from small to large, and a filter never below the smallest pointer target.
#[test]
fn a_tag_stands_at_its_size_s_height_and_a_filter_at_the_smallest_target_at_least() {
    for (size, plain, filter) in [
        (ControlSize::Small, 20.0, 24.0),
        (ControlSize::Regular, 24.0, 24.0),
        (ControlSize::Large, 28.0, 28.0),
    ] {
        let (_, rect) = mounted(TagProps::props().label("Beta").size(size).build());
        assert_eq!(rect.height, plain, "a static {size:?} tag");
        let (_, rect) = mounted(
            TagProps::props()
                .label("Beta")
                .size(size)
                .selected(telar::signal(false))
                .build(),
        );
        assert_eq!(rect.height, filter, "a {size:?} filter");
    }
}

/// A padding the theme names is a length at the regular size, scaled to the tag's own.
#[test]
fn a_named_padding_scales_with_the_size() {
    let tokens = TagTokens {
        pad_x: Some(10.0),
        ..TagTokens::default()
    };
    assert_eq!(tokens.pad_x(ControlSize::Regular), 10.0);
    assert_eq!(tokens.pad_x(ControlSize::Large), 12.5);
}

fn small_remove(name: &'static str, run: Rc<dyn Fn()>) -> telar::Children {
    telar::Children::one(move || {
        crate::icon_button(
            crate::IconButtonProps::props()
                .icon("lucide:x")
                .label(name)
                .size(ControlSize::Small)
                .on_press(run.clone())
                .build(),
        )
    })
}

/// A small icon button is as tall as a regular tag and as a small one round an action, so removing a tag never makes it taller than a filter of its size.
#[test]
fn a_small_icon_button_in_the_action_keeps_the_tag_at_its_own_height() {
    for (size, height) in [
        (ControlSize::Small, 24.0),
        (ControlSize::Regular, 24.0),
        (ControlSize::Large, 28.0),
    ] {
        let (_, removable) = mounted(
            TagProps::props()
                .label("Logo")
                .size(size)
                .action(small_remove("Remove Logo", Rc::new(|| {})))
                .build(),
        );
        assert_eq!(
            removable.height, height,
            "a {size:?} tag round a small action"
        );
    }
}

/// A filter's toggle and the small action beside it are each a whole pointer target at every size, however short the label, and side by side rather than over each other.
#[test]
fn a_filter_and_its_small_action_are_each_a_full_target_side_by_side_at_every_size() {
    for size in SIZES {
        fresh_layout_runtime();
        telar::focus::clear();
        let tag = tag(TagProps::props()
            .label("3")
            .size(size)
            .selected(telar::signal(true))
            .action(small_remove("Remove 3", Rc::new(|| {})))
            .build())
        .unwrap();
        in_a_row_node(tag.layout_node());
        let _tree = ComponentList::new(tag);
        let [toggle, remove] = crate::test_support::drawn(Role::Button)[..] else {
            panic!("a {size:?} filter with an action shows two buttons");
        };
        for (part, rect) in [("toggle", toggle), ("action", remove)] {
            assert!(
                rect.width >= shared::MIN_TARGET && rect.height >= shared::MIN_TARGET,
                "the {size:?} {part} is {rect:?}"
            );
        }
        assert!(
            toggle.x + toggle.width <= remove.x + 0.5,
            "the {size:?} toggle {toggle:?} runs into the action {remove:?}"
        );
    }
}

/// The outline's frame is drawn inside the box, so swapping one variant for another never moves what is beside it.
#[test]
fn every_variant_is_the_same_size() {
    let sizes: Vec<Rect> = VARIANTS
        .into_iter()
        .map(|variant| mounted(TagProps::props().label("Beta").variant(variant).build()).1)
        .collect();
    assert!(
        sizes
            .iter()
            .all(|r| (r.width, r.height) == (sizes[0].width, sizes[0].height)),
        "{sizes:?}"
    );
}

#[test]
fn tapping_the_action_runs_it() {
    fresh_layout_runtime();
    let removed = Rc::new(Cell::new(false));
    let sink = removed.clone();
    let mut tag = tag(TagProps::props()
        .label("Logo")
        .action(acting("Remove Logo", Rc::new(move || sink.set(true))))
        .build())
    .unwrap();
    let rect = track_layout(tag.layout_node()).unwrap();
    in_a_row_node(tag.layout_node());
    let r = rect.get();
    let (x, y) = (
        (r.x + r.width - shared::MIN_TARGET / 2.0) as f64,
        (r.y + r.height / 2.0) as f64,
    );
    for event in [
        Event::PointerPressed {
            x,
            y,
            button: PointerButton::Primary,
            source: PointerSource::Mouse,
        },
        Event::PointerReleased {
            x,
            y,
            button: PointerButton::Primary,
            source: PointerSource::Mouse,
        },
    ] {
        tag.on_event(&event);
    }
    assert!(removed.get(), "a tap at the tag's end reaches its action");
}

fn in_a_row_node(node: telar::NodeId) {
    let root = new_container(
        LayoutStyle::new().flex_row().width(200.0).height(60.0),
        &[node],
    )
    .unwrap();
    compute_layout(
        root,
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(60.0),
    )
    .unwrap();
}

#[test]
fn a_tag_with_none_of_the_behaviours_is_text_and_no_tab_stop() {
    let mut play = played(static_tag);
    assert!(play.find(&by_text("Saved")).is_ok(), "the label is read");
    assert_eq!(buttons(&play), [], "nothing to operate");
    assert!(
        play.tab().is_err(),
        "Tab finds no stop in the canvas and leaves it"
    );
    assert!(play.snapshot().iter().all(|node| !node.focused));
}

#[test]
fn selected_alone_makes_a_filter_bound_to_it() {
    let mut play = played(bound);
    let images = || by_role(Role::Button).named("Images");
    assert_eq!(buttons(&play), [("Images".to_string(), Some(false))]);
    play.click(images()).unwrap();
    play.expect_toggled(images(), true).unwrap();
    assert!(play_signal(&play), "the press flips the bound state");
    play.expect_focused(images()).unwrap();
    play.press(Key::Named(NamedKey::Space)).unwrap();
    play.expect_toggled(images(), false).unwrap();
    assert!(!play_signal(&play));
    play.press(Key::Named(NamedKey::Enter)).unwrap();
    play.expect_toggled(images(), true).unwrap();
    assert!(play_signal(&play));
}

#[test]
fn on_toggle_alone_makes_a_filter_that_keeps_its_own_state() {
    let mut play = played(uncontrolled);
    let images = || by_role(Role::Button).named("Images");
    assert_eq!(buttons(&play), [("Images".to_string(), Some(false))]);
    play.click(images()).unwrap();
    play.expect_toggled(images(), true).unwrap();
    play.expect_focused(images()).unwrap();
    play.press(Key::Named(NamedKey::Space)).unwrap();
    play.expect_toggled(images(), false).unwrap();
    play.press(Key::Named(NamedKey::Enter)).unwrap();
    play.expect_toggled(images(), true).unwrap();
    let told: Vec<String> = play
        .actions()
        .calls()
        .into_iter()
        .map(|call| call.args.join(""))
        .collect();
    assert_eq!(
        told,
        ["true", "false", "true"],
        "each press tells the new state"
    );
}

#[test]
fn selected_with_on_toggle_flips_the_bound_state_and_tells_it() {
    let mut play = played(bound_and_told);
    play.click(by_role(Role::Button).named("Images")).unwrap();
    assert!(play_signal(&play));
    assert_eq!(play.actions().last().expect("told").args, ["true"]);
}

fn play_signal(play: &Play) -> bool {
    play.ctx().signal("on", false).get()
}

/// A button inside a button is one no reader can make sense of, so the action sits beside the toggle, a Tab stop of its own.
#[test]
fn a_filter_with_an_action_is_a_toggle_with_the_action_beside_it() {
    let mut play = played(filter_with_an_action);
    assert_eq!(
        buttons(&play),
        [
            ("Images".to_string(), Some(true)),
            ("Remove Images".to_string(), None),
        ]
    );
    play.tab().unwrap();
    play.expect_focused(by_role(Role::Button).named("Images"))
        .unwrap();
    play.tab().unwrap();
    play.expect_focused(by_role(Role::Button).named("Remove Images"))
        .unwrap();
    play.press(Key::Named(NamedKey::Space)).unwrap();
    play.expect_action("remove", 1).unwrap();
    play.expect_toggled(by_role(Role::Button).named("Images"), true)
        .unwrap();
}

#[test]
fn an_unselected_filter_is_drawn_neutral_and_a_selected_one_in_its_tone() {
    let drawn = |on: bool| {
        let (tree, _) = mounted(
            TagProps::props()
                .label("Images")
                .tone(Tone::Success)
                .variant(Variant::Solid)
                .selected(telar::signal(on))
                .build(),
        );
        first_painted(&tree)
    };
    let off = drawn(false);
    assert_eq!(solid(off.fill).a, 0.0, "no fill while off");
    assert_eq!(
        frame(&off),
        shared::border(),
        "the theme's border while off"
    );
    let on = drawn(true);
    assert_eq!(solid(on.fill), telar::use_theme_tokens().success());
}

#[test]
fn a_colour_is_a_tone_of_its_own() {
    let violet = Color::rgba(0.4, 0.2, 0.7, 1.0);
    let (tree, _) = mounted(
        TagProps::props()
            .label("Beta")
            .tone(violet)
            .variant(Variant::Solid)
            .build(),
    );
    assert_eq!(solid(first_painted(&tree).fill), violet);
    assert!(contrast(&tree) >= 4.5);
}

/// A caller's colour is worked out as a named tone is, in every variant and every state of a filter.
#[test]
fn a_colour_tone_derives_every_state_and_a_legible_ink() {
    let pale = Color::rgb(0.98, 0.85, 0.3);
    for scheme in [ColorScheme::Light, ColorScheme::Dark] {
        for variant in VARIANTS {
            for (does, props) in behaviours(Tone::from(pale), variant) {
                in_scheme(scheme);
                let (ink, grounds) = ink_and_grounds(props);
                for ground in grounds {
                    let contrast = ink.contrast_ratio(ground);
                    assert!(
                        contrast >= 4.5,
                        "{scheme:?} {variant:?} {does}: {contrast:.2}"
                    );
                }
            }
            let [rest, hovered, pressed] = part_paint(
                TagProps::props()
                    .label("Tag")
                    .tone(pale)
                    .variant(variant)
                    .selected(telar::signal(true))
                    .build(),
            );
            assert!(
                rest != hovered && hovered != pressed,
                "{variant:?} answers the pointer"
            );
        }
    }
}

/// `transparent` is a colour like any other, not a way of saying "unset": nothing is painted, and the label still reads on what shows through.
#[test]
fn a_transparent_tone_paints_nothing_and_keeps_its_label_legible() {
    for variant in VARIANTS {
        let (tree, _) = mounted(
            TagProps::props()
                .label("Beta")
                .variant(variant)
                .tone(Color::TRANSPARENT)
                .build(),
        );
        let painted = first_painted(&tree);
        assert_eq!(solid(painted.fill).a, 0.0, "{variant:?} fills nothing");
        assert_eq!(frame(&painted).a, 0.0, "{variant:?} frames nothing");
        let contrast = contrast(&tree);
        assert!(contrast >= 4.5, "{variant:?}: {contrast}");
    }
}

#[test]
fn every_tone_and_variant_reads_at_4_5_to_1_in_light_and_dark_in_every_state() {
    let mut failures = Vec::new();
    for scheme in [ColorScheme::Light, ColorScheme::Dark] {
        for tone in TONES {
            for variant in VARIANTS {
                for (does, props) in behaviours(tone.clone(), variant) {
                    in_scheme(scheme);
                    let (ink, grounds) = ink_and_grounds(props);
                    for (state, ground) in ["rest", "hovered", "pressed"].iter().zip(grounds) {
                        let contrast = ink.contrast_ratio(ground);
                        if contrast < 4.5 {
                            failures.push(format!(
                                "{scheme:?} {tone:?} {variant:?} {does} {state}: {contrast:.2}"
                            ));
                        }
                    }
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// A tag of `tone` and `variant` as each behaviour makes it: static, a filter on and off, one with an action.
fn behaviours(tone: Tone, variant: Variant) -> Vec<(&'static str, TagProps)> {
    let tag = || {
        TagProps::props()
            .label("Tag")
            .tone(tone.clone())
            .variant(variant)
    };
    vec![
        ("static", tag().build()),
        ("filter on", tag().selected(telar::signal(true)).build()),
        ("filter off", tag().selected(telar::signal(false)).build()),
        (
            "with an action",
            tag().action(acting("Act", Rc::new(|| {}))).build(),
        ),
    ]
}

/// The label's ink, and what it sits on over the surface: at rest, then, where the tag has a part the pointer operates, under the pointer on that part and pressed on it.
fn ink_and_grounds(props: TagProps) -> (Color, Vec<Color>) {
    fresh_layout_runtime();
    let item = tag(props).unwrap();
    in_a_row_node(item.layout_node());
    let mut tree = ComponentList::new(item);
    let rest = shared::over(solid(first_painted(&tree).fill), shared::surface());
    let ink = ink(&tree);
    let mut grounds = vec![rest];
    if let Some(part) = crate::test_support::drawn(Role::Button).first().copied() {
        let (x, y) = centre(part);
        tree.on_event(&moved(x, y));
        grounds.push(shared::over(solid(painted_at(&tree, part).fill), rest));
        tree.on_event(&press(x, y));
        grounds.push(shared::over(solid(painted_at(&tree, part).fill), rest));
    }
    (ink, grounds)
}

fn part_paint(props: TagProps) -> [Option<Paint>; 3] {
    fresh_layout_runtime();
    let item = tag(props).unwrap();
    in_a_row_node(item.layout_node());
    let mut tree = ComponentList::new(item);
    let part = crate::test_support::drawn(Role::Button)[0];
    let (x, y) = centre(part);
    let rest = painted_at(&tree, part).fill;
    tree.on_event(&moved(x, y));
    let hovered = painted_at(&tree, part).fill;
    tree.on_event(&press(x, y));
    [rest, hovered, painted_at(&tree, part).fill]
}

#[test]
fn a_filter_answers_the_pointer_in_every_variant() {
    for variant in VARIANTS {
        for (does, props) in behaviours(Tone::Accent, variant)
            .into_iter()
            .skip(1)
            .take(2)
        {
            let [rest, hovered, pressed] = part_paint(props);
            assert_ne!(rest, hovered, "{variant:?} {does}: a hover shows");
            assert_ne!(hovered, pressed, "{variant:?} {does}: a press shows");
        }
    }
}

#[test]
fn a_static_tag_never_reacts_to_the_pointer() {
    for variant in VARIANTS {
        fresh_layout_runtime();
        let item = tag(TagProps::props().label("Beta").variant(variant).build()).unwrap();
        let rect = track_layout(item.layout_node()).unwrap();
        in_a_row_node(item.layout_node());
        let mut tree = ComponentList::new(item);
        let at_rest = format!("{:?}", tree.commands());
        let (x, y) = centre(rect.get());
        tree.on_event(&moved(x, y));
        assert_eq!(
            format!("{:?}", tree.commands()),
            at_rest,
            "{variant:?} hovered"
        );
        tree.on_event(&press(x, y));
        assert_eq!(
            format!("{:?}", tree.commands()),
            at_rest,
            "{variant:?} pressed"
        );
    }
}

fn word(text: &'static str) -> telar::Children {
    telar::Children::one(move || {
        Ok(telar::box_item(telar::Text::declaring(
            move || text.to_string(),
            LayoutStyle::new(),
            |t| t,
        )?))
    })
}

fn placed_text(tree: &ComponentList, text: &str) -> Rect {
    let mut found = None;
    telar::for_each_with_matrix(&tree.commands(), |command, matrix| {
        if let DrawCommand::Text {
            text: drawn, rect, ..
        } = command
            && drawn.as_ref() == text
        {
            found = Some(telar::transform_clip_rect(matrix, *rect));
        }
    });
    found.expect("the text is drawn")
}

fn text_x(tree: &ComponentList, text: &str) -> f32 {
    placed_text(tree, text).x
}

#[test]
fn leading_and_trailing_sit_before_and_after_the_label_and_before_the_action() {
    for filter in [false, true] {
        fresh_layout_runtime();
        let props = TagProps::props()
            .label("Logo")
            .leading(word("L"))
            .trailing(word("T"))
            .action(acting("Remove Logo", Rc::new(|| {})));
        let props = match filter {
            true => props.selected(telar::signal(true)),
            false => props,
        };
        let item = tag(props.build()).unwrap();
        in_a_row_node(item.layout_node());
        let tree = ComponentList::new(item);
        let (leading, label, trailing) = (
            text_x(&tree, "L"),
            text_x(&tree, "Logo"),
            text_x(&tree, "T"),
        );
        assert!(
            leading < label && label < trailing,
            "{leading} {label} {trailing}"
        );
        let parts = crate::test_support::drawn(Role::Button);
        let remove = parts.last().expect("the action");
        let trailing_end = placed_text(&tree, "T").x + placed_text(&tree, "T").width;
        assert!(trailing_end <= remove.x, "{remove:?}");
        if filter {
            let body = parts[0];
            assert!(
                body.x <= leading && trailing < body.x + body.width,
                "the slots are inside the toggle: {body:?}"
            );
        }
    }
}

#[test]
fn a_spinner_leading_a_tag_turns() {
    fresh_layout_runtime();
    let item = tag(TagProps::props()
        .label("Exporting")
        .leading(telar::Children::one(|| {
            crate::spinner(crate::SpinnerProps::props().build())
        }))
        .build())
    .unwrap();
    in_a_row_node(item.layout_node());
    let tree = ComponentList::new(item);
    let start = web_time::Instant::now();
    telar::motion::tick(start);
    let before = format!("{:?}", tree.commands());
    telar::motion::tick(start + std::time::Duration::from_millis(225));
    assert_ne!(
        format!("{:?}", tree.commands()),
        before,
        "the spinner moved"
    );
}

#[test]
fn solid_is_the_default_variant_and_accent_the_default_tone() {
    let props = TagProps::props().build();
    assert_eq!((props.variant, props.tone), (Variant::Solid, Tone::Accent));
}

/// The tag places the caller's control whole: as large a pointer target as it brings, beside a toggle as well.
#[test]
fn the_action_keeps_its_full_pointer_target() {
    for build in [with_an_action, filter_with_an_action] {
        let play = played(build);
        let findings: Vec<String> = play
            .check_a11y()
            .at_least(Severity::Warning)
            .map(ToString::to_string)
            .collect();
        assert!(findings.is_empty(), "{findings:?}");
        let remove = play
            .snapshot()
            .into_iter()
            .find(|node| node.name.starts_with("Remove"))
            .expect("the action");
        assert!(
            remove.rect.width >= shared::MIN_TARGET && remove.rect.height >= shared::MIN_TARGET,
            "{:?}",
            remove.rect
        );
    }
}

/// The action sits beside the toggle, not over it, so a press at the end of the toggle's label is the toggle's at every size.
#[test]
fn a_press_at_the_toggle_s_label_edge_toggles_and_does_not_reach_the_action() {
    for size in SIZES {
        fresh_layout_runtime();
        telar::focus::clear();
        let on = telar::signal(false);
        let removed = Rc::new(Cell::new(0));
        let sink = removed.clone();
        let mut tag = tag(TagProps::props()
            .label("Images")
            .size(size)
            .selected(on)
            .action(acting(
                "Remove Images",
                Rc::new(move || sink.set(sink.get() + 1)),
            ))
            .build())
        .unwrap();
        in_a_row_node(tag.layout_node());
        let body = crate::test_support::drawn(Role::Button)[0];
        let y = (body.y + body.height / 2.0) as f64;
        let label_end = body.x + body.width - TagTokens::current().pad_x(size);
        for x in [label_end - 0.5, body.x + body.width - 0.5] {
            let before = on.get();
            for event in [
                Event::PointerPressed {
                    x: x as f64,
                    y,
                    button: PointerButton::Primary,
                    source: PointerSource::Mouse,
                },
                Event::PointerReleased {
                    x: x as f64,
                    y,
                    button: PointerButton::Primary,
                    source: PointerSource::Mouse,
                },
            ] {
                tag.on_event(&event);
            }
            assert_eq!(on.get(), !before, "a {size:?} press at x={x} toggles");
        }
        assert_eq!(removed.get(), 0, "and a {size:?} one never removes");
    }
}
