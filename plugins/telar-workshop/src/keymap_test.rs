use telar::preview::{Layout, PreviewCtx};
use telar::testing::{key_with, mount, press, release, route, texts};
use telar::{
    AccessNode, App, AppRuntime, ComponentList, Container, Event, Input, LayoutStyle, LocalApp,
    Role, SizeDimension, box_item, reset_layout_runtime,
};

use super::*;
use crate::canvas_toolbar::locales;
use crate::settings::CanvasSettings;
use crate::test_support::{Shell, settle};

const WIDTH: u32 = 1400;
const HEIGHT: u32 = 900;

fn blank(_: &PreviewCtx) -> Result<Box<dyn LayoutItem>, LayoutError> {
    Ok(box_item(Container::new(LayoutStyle::new(), Vec::new())?))
}

/// A field filling the canvas, for the keyboard to type into.
fn field(_: &PreviewCtx) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let input = Input::declaring(
        signal(String::new()),
        LayoutStyle::new()
            .width(SizeDimension::Percent(1.0))
            .height(SizeDimension::Percent(1.0)),
        |text| text,
    )?;
    Ok(box_item(input))
}

fn entries() -> Vec<PreviewEntry> {
    vec![
        PreviewEntry::new("button--primary", "button", "Primary", blank).title("Inputs/Button"),
        PreviewEntry::new("button--secondary", "button", "Secondary", blank).title("Inputs/Button"),
        PreviewEntry::new("checkbox--checked", "checkbox", "Checked", blank)
            .title("Inputs/Checkbox")
            .locale("ar"),
    ]
}

struct Workshop {
    runtime: LocalApp<Shell>,
    tree: ComponentList,
}

impl Workshop {
    fn open(entries: Vec<PreviewEntry>) -> Self {
        reset_layout_runtime();
        focus::clear();
        telar::install_default_text_metrics();
        let state = WorkshopState::new(entries.into(), &Default::default());
        let runtime = LocalApp(Shell { state });
        let tree = mount(runtime.0.root(), WIDTH, HEIGHT);
        let workshop = Self { runtime, tree };
        settle(&workshop.tree);
        workshop
    }

    fn state(&self) -> &WorkshopState {
        &self.runtime.0.state
    }

    fn settings(&self) -> CanvasSettings {
        self.state().canvas_settings().get()
    }

    fn send(&mut self, event: Event) {
        route(&mut self.tree, &event);
        settle(&self.tree);
    }

    fn press(&mut self, key: Key, modifiers: ModifiersState) {
        self.send(key_with(key, modifiers));
    }

    fn alt(&mut self, c: char) {
        self.press(Key::Char(c), alt(false));
    }

    fn alt_shift(&mut self, c: char) {
        self.press(Key::Char(c.to_ascii_uppercase()), alt(true));
    }

    fn alt_arrow(&mut self, arrow: NamedKey) {
        self.press(Key::Named(arrow), alt(false));
    }

    fn key(&mut self, key: NamedKey) {
        self.press(Key::Named(key), ModifiersState::default());
    }

    fn type_text(&mut self, text: &str) {
        for c in text.chars() {
            self.press(Key::Char(c), ModifiersState::default());
        }
    }

    fn selected(&self) -> String {
        self.state()
            .selection()
            .get()
            .as_deref()
            .unwrap_or_default()
            .to_owned()
    }

    fn draws(&self, text: &str) -> bool {
        texts(&self.tree).iter().any(|drawn| drawn == text)
    }

    fn nodes(&self) -> Vec<AccessNode> {
        self.runtime.access_snapshot(&self.tree.commands())
    }

    /// The dialogs open, by the names they are announced under.
    fn dialogs(&self) -> Vec<String> {
        self.nodes()
            .into_iter()
            .filter(|node| node.role == Role::Dialog)
            .map(|node| node.name)
            .collect()
    }

    /// A point on the stage, under the canvas header: the middle of the area between the sidebar and the window's right edge, a third of the way down.
    fn below_header(&self) -> (f32, f32) {
        let splitter = self
            .nodes()
            .into_iter()
            .find(|node| node.role == Role::Splitter && node.name == "Resize the sidebar")
            .expect("the sidebar has a splitter")
            .rect;
        let left = splitter.x + splitter.width;
        (left + (WIDTH as f32 - left) / 2.0, HEIGHT as f32 / 3.0)
    }

    fn click_at(&mut self, x: f32, y: f32) {
        self.send(press(x, y));
        self.send(release(x, y));
    }

    fn click(&mut self, role: Role, name: &str) {
        let rect = self
            .nodes()
            .into_iter()
            .find(|node| node.role == role && node.name == name)
            .unwrap_or_else(|| panic!("no {role:?} named {name:?}"))
            .rect;
        self.click_at(rect.x + rect.width / 2.0, rect.y + rect.height / 2.0);
    }
}

fn alt(shift: bool) -> ModifiersState {
    ModifiersState {
        is_alt: true,
        is_shift: shift,
        ..ModifiersState::default()
    }
}

#[test]
fn alt_up_and_down_walk_the_previews_and_stop_at_either_end() {
    let mut workshop = Workshop::open(entries());
    assert_eq!(workshop.selected(), "button--primary");
    for expected in [
        "button--secondary",
        "checkbox--checked",
        "checkbox--checked",
    ] {
        workshop.alt_arrow(NamedKey::ArrowDown);
        assert_eq!(workshop.selected(), expected);
    }
    workshop.alt_arrow(NamedKey::ArrowUp);
    assert_eq!(workshop.selected(), "button--secondary");
}

#[test]
fn alt_left_and_right_walk_the_components_to_their_first_preview() {
    let mut workshop = Workshop::open(entries());
    workshop.state().select("button--secondary");
    workshop.alt_arrow(NamedKey::ArrowRight);
    assert_eq!(workshop.selected(), "checkbox--checked");
    workshop.alt_arrow(NamedKey::ArrowRight);
    assert_eq!(workshop.selected(), "checkbox--checked");
    workshop.alt_arrow(NamedKey::ArrowLeft);
    assert_eq!(workshop.selected(), "button--primary");
}

#[test]
fn the_preview_walk_keeps_to_what_the_search_lists() {
    let mut workshop = Workshop::open(entries());
    workshop.state().search().set("check".into());
    settle(&workshop.tree);
    workshop.alt_arrow(NamedKey::ArrowDown);
    assert_eq!(
        workshop.selected(),
        "checkbox--checked",
        "from a preview the search hides, the first one it lists"
    );
    workshop.alt_arrow(NamedKey::ArrowUp);
    assert_eq!(workshop.selected(), "checkbox--checked");
}

#[test]
fn alt_digits_switch_the_view() {
    let mut workshop = Workshop::open(entries());
    workshop.alt('3');
    assert_eq!(workshop.state().view().get(), ViewMode::Docs);
    workshop.alt('2');
    assert_eq!(
        workshop.state().view().get(),
        ViewMode::Docs,
        "the matrix view is not there yet"
    );
    workshop.alt('1');
    assert_eq!(workshop.state().view().get(), ViewMode::Canvas);
}

#[test]
fn alt_s_a_and_d_fold_and_move_the_panes() {
    let mut workshop = Workshop::open(entries());
    let state = workshop.state().clone();
    workshop.alt('s');
    assert!(state.sidebar_collapsed().get());
    workshop.alt('s');
    assert!(!state.sidebar_collapsed().get());
    workshop.alt('a');
    assert!(state.panel_collapsed().get());
    workshop.alt('a');
    assert!(!state.panel_collapsed().get());
    workshop.alt('d');
    assert_eq!(state.panel_position().get(), PanelPosition::Right);
    workshop.alt('d');
    assert_eq!(state.panel_position().get(), PanelPosition::Bottom);
}

#[test]
fn alt_f_takes_the_canvas_fullscreen_and_puts_the_panes_back_as_they_were() {
    let mut workshop = Workshop::open(entries());
    let state = workshop.state().clone();
    workshop.alt('a');
    workshop.alt('f');
    assert!(state.sidebar_collapsed().get() && state.panel_collapsed().get());
    workshop.alt('f');
    assert!(!state.sidebar_collapsed().get());
    assert!(
        state.panel_collapsed().get(),
        "the panels were folded before"
    );
}

#[test]
fn alt_t_l_and_r_cycle_the_mode_locale_and_direction_back_to_the_previews_own() {
    let mut workshop = Workshop::open(entries());
    let locales = locales(workshop.state());
    assert!(locales.contains(&"ar".to_string()));
    for locale in &locales {
        workshop.alt('l');
        assert_eq!(workshop.settings().locale.as_ref(), Some(locale));
    }
    workshop.alt('l');
    assert_eq!(workshop.settings().locale, None);

    for direction in [Some(Direction::Ltr), Some(Direction::Rtl), None] {
        workshop.alt('r');
        assert_eq!(workshop.settings().direction, direction);
    }

    let modes = telar::registered_modes();
    workshop.alt('t');
    assert_eq!(workshop.settings().mode, modes.first().cloned());
}

#[test]
fn alt_zoom_keys_step_the_zoom_and_alt_0_fits_it_again() {
    let mut workshop = Workshop::open(entries());
    workshop.alt('=');
    assert_eq!(workshop.settings().zoom, Zoom::Percent(150));
    workshop.alt_shift('+');
    assert_eq!(workshop.settings().zoom, Zoom::Percent(200));
    workshop.alt('-');
    workshop.alt('-');
    assert_eq!(workshop.settings().zoom, Zoom::Percent(100));
    workshop.alt('0');
    assert_eq!(workshop.settings().zoom, Zoom::Fit);
}

#[test]
fn alt_g_shows_the_grid_and_alt_shift_g_the_rulers() {
    let mut workshop = Workshop::open(entries());
    workshop.alt('g');
    assert!(workshop.settings().grid && !workshop.settings().rulers);
    workshop.alt_shift('g');
    assert!(workshop.settings().rulers);
    workshop.alt('g');
    assert!(!workshop.settings().grid && workshop.settings().rulers);
}

#[test]
fn alt_shift_r_remounts_and_alt_r_alone_does_not() {
    let mut workshop = Workshop::open(entries());
    workshop.alt('r');
    assert_eq!(workshop.state().remounts(), 0);
    workshop.alt_shift('r');
    assert_eq!(workshop.state().remounts(), 1);
}

#[test]
fn slash_and_mod_k_open_the_palette_without_typing_into_it() {
    let mut workshop = Workshop::open(entries());
    workshop.type_text("/");
    assert_eq!(workshop.dialogs(), ["Command palette"]);
    assert!(
        workshop.draws("Inputs/Checkbox / Checked") && !workshop.draws("No results"),
        "the palette opens on the whole list: {:?}",
        texts(&workshop.tree)
    );
    workshop.key(NamedKey::Escape);
    assert!(workshop.dialogs().is_empty());

    for modifiers in [
        ModifiersState {
            is_ctrl: true,
            ..ModifiersState::default()
        },
        ModifiersState {
            is_meta: true,
            ..ModifiersState::default()
        },
    ] {
        workshop.press(Key::Char('k'), modifiers);
        assert_eq!(workshop.dialogs(), ["Command palette"]);
        workshop.key(NamedKey::Escape);
    }
}

#[test]
fn the_find_trigger_opens_the_palette() {
    let mut workshop = Workshop::open(entries());
    workshop.click(Role::Button, "Find a preview or command");
    assert_eq!(workshop.dialogs(), ["Command palette"]);
}

#[test]
fn the_palette_lists_every_preview_then_every_command_with_its_keys() {
    let mut workshop = Workshop::open(entries());
    workshop.type_text("/");
    for shown in [
        "Previews",
        "Inputs/Button / Primary",
        "Inputs/Button / Secondary",
        "Navigate",
        "Next preview",
        "Alt",
    ] {
        assert!(
            workshop.draws(shown),
            "{shown}: {:?}",
            texts(&workshop.tree)
        );
    }
    let ids: Vec<_> = palette::commands(workshop.state())
        .into_iter()
        .map(|command| command.id.to_string())
        .collect();
    assert_eq!(
        ids[..3],
        [
            "preview:button--primary",
            "preview:button--secondary",
            "preview:checkbox--checked"
        ]
    );
    assert_eq!(ids.len(), 3 + ACTIONS.len() - 1, "every action but Find");
}

#[test]
fn the_arrows_walk_the_palette_and_enter_runs_what_the_cursor_is_on() {
    let mut workshop = Workshop::open(entries());
    workshop.type_text("/");
    workshop.key(NamedKey::ArrowDown);
    workshop.key(NamedKey::ArrowDown);
    workshop.key(NamedKey::ArrowUp);
    workshop.key(NamedKey::Enter);
    assert_eq!(workshop.selected(), "button--secondary");
    assert!(workshop.dialogs().is_empty(), "running a command closes it");
}

#[test]
fn typing_in_the_palette_finds_a_preview_or_a_command() {
    let mut workshop = Workshop::open(entries());
    workshop.type_text("/");
    workshop.type_text("checked");
    workshop.key(NamedKey::Enter);
    assert_eq!(workshop.selected(), "checkbox--checked");

    workshop.type_text("/");
    workshop.type_text("hide the grid");
    workshop.key(NamedKey::Enter);
    assert!(workshop.settings().grid);
}

#[test]
fn question_mark_lists_every_shortcut_on_its_caps() {
    let mut workshop = Workshop::open(entries());
    workshop.type_text("?");
    assert_eq!(workshop.dialogs(), ["Keyboard shortcuts"]);
    for shown in [
        "Navigate",
        "View",
        "Canvas",
        "Next preview",
        "Fullscreen canvas",
        "Remount",
        "Alt",
        "Shift",
        "↓",
        "/",
        "?",
    ] {
        assert!(
            workshop.draws(shown),
            "{shown}: {:?}",
            texts(&workshop.tree)
        );
    }
    workshop.key(NamedKey::Escape);
    assert!(workshop.dialogs().is_empty());
}

#[test]
fn shortcuts_wait_while_the_shortcuts_overlay_is_open() {
    let mut workshop = Workshop::open(entries());
    workshop.type_text("?");
    workshop.alt_arrow(NamedKey::ArrowDown);
    assert_eq!(workshop.selected(), "button--primary");
}

#[test]
fn no_shortcut_fires_while_a_field_in_the_chrome_has_the_caret() {
    let mut workshop = Workshop::open(entries());
    workshop.state().focus_search();
    settle(&workshop.tree);
    assert!(focus::text_entry_focused());
    workshop.alt('s');
    workshop.alt('g');
    workshop.alt_arrow(NamedKey::ArrowDown);
    workshop.alt('3');
    workshop.type_text("/?");
    workshop.press(
        Key::Char('k'),
        ModifiersState {
            is_ctrl: true,
            ..ModifiersState::default()
        },
    );
    assert!(!workshop.state().sidebar_collapsed().get());
    assert_eq!(workshop.settings(), CanvasSettings::default());
    assert_eq!(workshop.selected(), "button--primary");
    assert_eq!(workshop.state().view().get(), ViewMode::Canvas);
    assert!(workshop.dialogs().is_empty());
    assert!(
        workshop.state().search().get().contains("/?"),
        "the field takes what it is typed"
    );
}

#[test]
fn no_shortcut_fires_while_a_field_inside_the_canvas_has_the_caret() {
    let mut workshop = Workshop::open(vec![
        PreviewEntry::new("form--field", "form", "Field", field).layout(Layout::Fullscreen),
        PreviewEntry::new("form--other", "form", "Other", blank),
    ]);
    let stage = workshop.below_header();
    workshop.click_at(stage.0, stage.1);
    assert!(
        focus::text_entry_focused(),
        "the field inside the canvas has the caret"
    );
    workshop.alt('g');
    workshop.alt_arrow(NamedKey::ArrowDown);
    workshop.type_text("?");
    assert!(!workshop.settings().grid);
    assert_eq!(workshop.selected(), "form--field");
    assert!(workshop.dialogs().is_empty());
}

#[test]
fn a_chord_names_one_action_whatever_the_case_of_its_letter() {
    assert_eq!(
        action_for(&Key::Char('S'), alt(false)),
        Some(Action::ToggleSidebar)
    );
    assert_eq!(
        action_for(&Key::Char('R'), alt(true)),
        Some(Action::Remount)
    );
    assert_eq!(action_for(&Key::Char('s'), ModifiersState::default()), None);
    assert_eq!(
        action_for(
            &Key::Char('s'),
            ModifiersState {
                is_alt: true,
                is_ctrl: true,
                ..ModifiersState::default()
            }
        ),
        None,
        "Ctrl or the command key with Alt is not a workshop chord"
    );
}

#[test]
fn every_action_has_a_distinct_id_and_a_chord_that_runs_it() {
    let mut ids: Vec<_> = ACTIONS.iter().map(|action| action.id()).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), ACTIONS.len());
    for action in ACTIONS {
        assert!(GROUPS.contains(&action.group()));
        for chord in action.chords() {
            assert_eq!(
                parse(chord).and_then(|(key, modifiers)| action_for(&key, modifiers)),
                Some(action),
                "{chord} runs {action:?}"
            );
        }
    }
}

/// The key and modifiers a chord as `kbd` shows it stands for.
fn parse(chord: &str) -> Option<(Key, ModifiersState)> {
    let mut modifiers = ModifiersState::default();
    let parts: Vec<&str> = chord.split('+').collect();
    let (last, held) = parts.split_last()?;
    for part in held {
        match *part {
            "Alt" => modifiers.is_alt = true,
            "Shift" => modifiers.is_shift = true,
            "Mod" => modifiers.is_ctrl = true,
            _ => return None,
        }
    }
    let key = match *last {
        "↑" => Key::Named(NamedKey::ArrowUp),
        "↓" => Key::Named(NamedKey::ArrowDown),
        "←" => Key::Named(NamedKey::ArrowLeft),
        "→" => Key::Named(NamedKey::ArrowRight),
        key => Key::Char(key.chars().next()?.to_ascii_lowercase()),
    };
    Some((key, modifiers))
}

fn chord_action(key: char, unmodified: Option<char>, modifiers: ModifiersState) -> Option<Action> {
    let key = Key::Char(key);
    telar::observe_keyboard(&Event::KeyPressed {
        key: key.clone(),
        modifiers,
        unmodified: unmodified.map(Key::Char),
    });
    pressed_action(&key, modifiers)
}

/// Option+T types `†` on macOS, so the chord has to be matched on the key the press makes with no modifier.
#[test]
fn an_option_chord_composed_into_another_character_still_runs() {
    assert_eq!(
        chord_action('†', Some('t'), alt(false)),
        Some(Action::CycleMode),
        "Alt+T on a keyboard that composes it"
    );
    assert_eq!(
        chord_action('Ø', Some('o'), alt(false)),
        None,
        "a composed key whose plain key is no chord is none"
    );
    assert_eq!(
        chord_action('t', None, alt(false)),
        Some(Action::CycleMode),
        "a backend with no unmodified key falls back to the key itself"
    );
}

#[test]
fn the_unmodified_key_is_not_read_without_alt() {
    assert_eq!(
        chord_action('?', Some('/'), ModifiersState::default()),
        Some(Action::Shortcuts)
    );
}
