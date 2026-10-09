use super::*;
use crate::preview::{BuildFn, Layout, by_role, by_text};
use crate::{Accessible, Container, Input, RectStyle, Role, StyledContainer, box_item, signal};

type Built = Result<Box<dyn LayoutItem>, LayoutError>;

fn ink() -> TextStyle {
    TextStyle::new(14.0, Color::BLACK)
}

fn label(text: impl Fn() -> String + 'static) -> Box<dyn LayoutItem> {
    box_item(Text::new(text, LayoutStyle::new(), ink).unwrap())
}

fn button(
    text: impl Fn() -> String + 'static,
    on_press: impl Fn() + 'static,
) -> Box<dyn LayoutItem> {
    box_item(
        StyledContainer::new(
            LayoutStyle::new().padding_all(8.0),
            |_| RectStyle::default(),
            vec![label(text)],
        )
        .unwrap()
        .control(Role::Button)
        .on_press(on_press),
    )
}

fn counter(p: &PreviewCtx) -> Built {
    let presses = p.signal("presses", 0u32);
    let actions = p.actions();
    Ok(box_item(Container::new(
        LayoutStyle::new().flex_column(),
        vec![button(
            move || format!("Pressed {}", presses.get()),
            move || {
                presses.update(|count| *count += 1);
                actions.log("on_press", Vec::new());
            },
        )],
    )?))
}

fn two_buttons(_: &PreviewCtx) -> Built {
    Ok(box_item(Container::new(
        LayoutStyle::new().flex_row().gap(16.0),
        vec![
            button(|| String::from("One"), || {}),
            button(|| String::from("Two"), || {}),
        ],
    )?))
}

fn field(_: &PreviewCtx) -> Built {
    let value = signal(String::new());
    let input =
        Input::new(value, LayoutStyle::new().width(200.0).height(24.0), ink)?.a11y_label(|| "Name");
    Ok(box_item(Container::new(
        LayoutStyle::new().flex_column().gap(8.0),
        vec![
            box_item(input),
            label(move || format!("Hello, {}", value.get())),
        ],
    )?))
}

fn failing(_: &PreviewCtx) -> Built {
    Err(LayoutError::Engine(String::from("no tree today")))
}

fn entry(build: BuildFn) -> PreviewEntry {
    PreviewEntry::new("play--case--default", "case", "Default", build).layout(Layout::Centered)
}

#[test]
fn a_click_presses_the_control_a_reader_would_name() {
    let mut play = Play::mount(&entry(counter)).unwrap();
    play.click(by_role(Role::Button).named("Pressed 0"))
        .unwrap();
    play.expect_text("Pressed 1").unwrap();
    play.expect_action("on_press", 1).unwrap();

    let steps: Vec<(&str, bool)> = play
        .steps()
        .iter()
        .map(|step| (step.action.as_str(), step.outcome.is_ok()))
        .collect();
    assert_eq!(
        steps,
        [
            ("click button \"Pressed 0\"", true),
            ("expect text \"Pressed 1\"", true),
            ("expect 1 × on_press", true),
        ]
    );
    assert!(
        play.steps()[0]
            .frame
            .texts()
            .any(|text| text == "Pressed 1"),
        "each step keeps the frame it left behind"
    );
}

#[test]
fn a_failed_expectation_fails_its_step_and_says_what_was_there() {
    let mut play = Play::mount(&entry(counter)).unwrap();
    let error = play.expect_text("Pressed 9").unwrap_err();
    assert!(error.message.contains("Pressed 0"), "{error}");
    let error = play.expect_action("on_press", 1).unwrap_err();
    assert!(error.message.contains("saw 0"), "{error}");
    assert!(play.steps().iter().all(|step| step.outcome.is_err()));
}

#[test]
fn an_action_on_nothing_or_on_several_fails_with_what_the_canvas_reads() {
    let mut play = Play::mount(&entry(two_buttons)).unwrap();
    let missing = play
        .click(by_role(Role::Button).named("Three"))
        .unwrap_err();
    assert!(missing.message.contains("One, button"), "{missing}");
    let ambiguous = play.click(by_role(Role::Button)).unwrap_err();
    assert!(ambiguous.message.contains("matches 2 nodes"), "{ambiguous}");
    play.click(by_role(Role::Button).nth(1)).unwrap();
}

#[test]
fn tab_walks_the_canvas_in_order() {
    let mut play = Play::mount(&entry(two_buttons)).unwrap();
    play.tab().unwrap();
    play.expect_focused(by_text("One")).unwrap();
    play.tab().unwrap();
    play.expect_focused(by_text("Two")).unwrap();
    play.tab_back().unwrap();
    play.expect_focused(by_text("One")).unwrap();
    assert!(play.expect_focused(by_text("Two")).is_err());
}

#[test]
fn typing_reaches_the_field_a_reader_names() {
    let mut play = Play::mount(&entry(field)).unwrap();
    play.type_text(by_role(Role::TextInput).named("Name"), "Ada")
        .unwrap();
    play.expect_text("Hello, Ada").unwrap();
    play.press(Key::Named(NamedKey::Backspace)).unwrap();
    play.expect_text("Hello, Ad").unwrap();
}

#[test]
fn run_calls_the_entry_play_and_ends_the_log_on_its_own_error() {
    let passing = entry(counter).play(|canvas| {
        canvas.click(by_role(Role::Button).named("Pressed 0"))?;
        canvas.expect_action("on_press", 1)
    });
    let mut play = Play::mount(&passing).unwrap();
    assert_eq!(play.run(), Ok(()));
    assert_eq!(play.steps().len(), 2);

    let checking = entry(counter).play(|_| Err(PlayError::new("not today")));
    let mut play = Play::mount(&checking).unwrap();
    assert_eq!(play.run(), Err(PlayError::new("not today")));
    let last = play.steps().last().unwrap();
    assert_eq!(last.action, "play");
    assert!(last.outcome.is_err());
}

#[test]
fn a_play_is_set_on_a_const_entry() {
    const ENTRY: PreviewEntry =
        PreviewEntry::new("play--const--default", "const", "Default", counter)
            .play(|canvas| canvas.expect_text("Pressed 0"));
    let mut play = Play::mount(&ENTRY).unwrap();
    assert_eq!(play.run(), Ok(()));
}

#[test]
fn each_play_starts_from_the_args_defaults() {
    for _ in 0..2 {
        let mut play = Play::mount(&entry(counter)).unwrap();
        play.click(by_role(Role::Button).named("Pressed 0"))
            .unwrap();
    }
}

#[test]
fn a_preview_that_fails_to_build_does_not_mount() {
    let error = Play::mount(&entry(failing)).unwrap_err();
    assert!(error.message.contains("the preview failed"), "{error}");
}

fn ink_read_once(_: &PreviewCtx) -> Built {
    let ink = format!("{:?}", crate::use_theme_tokens().ink());
    Ok(label(move || ink.clone()))
}

#[test]
fn a_token_read_once_at_build_is_read_in_the_canvas_environment() {
    theme_core::register_mode("play-dusk", || {});
    theme_core::set_mode_scheme("play-dusk", crate::ColorScheme::Dark);
    let host = format!("{:?}", crate::use_theme_tokens().ink());
    let play = Play::mount(&entry(ink_read_once).mode("play-dusk")).unwrap();
    let dark = {
        let _entered = play.canvas().enter();
        format!("{:?}", crate::use_theme_tokens().ink())
    };
    assert_ne!(
        dark, host,
        "the canvas's mode is dark and the host's is not"
    );
    assert!(play.frame().texts().any(|text| text == dark));
}

fn notice(_: &PreviewCtx) -> Built {
    let shown = signal(true);
    let timer = crate::run_after(Duration::from_secs(3), move || shown.set(false));
    Ok(label(move || {
        let _waiting = &timer;
        String::from(if shown.get() { "Saved" } else { "Gone" })
    }))
}

#[test]
fn advance_lets_a_wait_pass_and_runs_what_came_due() {
    let mut play = Play::mount(&entry(notice)).unwrap();
    play.settle().unwrap();
    play.expect_text("Saved").unwrap();
    play.advance(Duration::from_secs(2)).unwrap();
    play.expect_text("Saved").unwrap();
    play.advance(Duration::from_secs(1)).unwrap();
    play.expect_text("Gone").unwrap();
    let advanced: Vec<&str> = play
        .steps()
        .iter()
        .map(|step| step.action.as_str())
        .filter(|action| action.starts_with("advance"))
        .collect();
    assert_eq!(advanced, ["advance 2s", "advance 1s"]);
}
