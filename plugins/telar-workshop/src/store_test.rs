use super::*;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "telar-workshop-store-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[test]
fn the_state_lives_under_the_workspace_s_telar_directory() {
    assert_eq!(
        state_file(Path::new("/ws")),
        PathBuf::from("/ws/.telar/workshop/state")
    );
}

#[test]
fn only_the_workshop_s_own_keys_outlive_the_run() {
    let file = state_file(&scratch("keys"));
    let store = WorkshopStore::open(&file);
    store.set(keys::SIDEBAR_WIDTH, Some("300.0"));
    store.set("telar.scheme", Some("dark"));
    assert_eq!(store.get("telar.scheme").as_deref(), Some("dark"));

    let next_run = WorkshopStore::open(&file);
    assert_eq!(next_run.get(keys::SIDEBAR_WIDTH).as_deref(), Some("300.0"));
    assert_eq!(
        next_run.get("telar.scheme"),
        None,
        "a preview's preferences stay in memory"
    );
}

mod across_runs {
    use std::rc::Rc;

    use telar::preview::host::PreviewRequest;
    use telar::preview::{ArgValue, PreviewCtx, PreviewEntry};
    use telar::{Color, LayoutError, LayoutItem, LayoutStyle, Text, TextStyle, box_item};

    use super::*;
    use crate::settings::{CanvasSettings, Zoom};
    use crate::state::{PanelPosition, WorkshopState};

    fn body(_: &PreviewCtx) -> Result<Box<dyn LayoutItem>, LayoutError> {
        Ok(box_item(Text::new(
            || "body".to_string(),
            LayoutStyle::new(),
            || TextStyle::new(13.0, Color::BLACK),
        )?))
    }

    fn entries() -> Rc<[PreviewEntry]> {
        Rc::from(vec![
            PreviewEntry::new("fake--button--primary", "button", "Primary", body)
                .title("Inputs/Button"),
            PreviewEntry::new("fake--field--empty", "field", "Empty", body).title("Inputs/Field"),
            PreviewEntry::new("fake--card--plain", "card", "Plain", body).title("Layout/Card"),
        ])
    }

    fn run(keeper: &Keeper, requested: &PreviewRequest) -> WorkshopState {
        WorkshopState::kept_in(entries(), requested, keeper)
    }

    #[test]
    fn a_restart_reopens_the_preview_panes_and_toolbar_with_the_args_at_their_defaults() {
        let keeper = Keeper::in_store(Arc::new(MemoryStore::default()));
        let first = run(&keeper, &PreviewRequest::default());
        first.select("fake--field--empty");
        first.sidebar_width().set(300.0);
        first.panel_collapsed().set(true);
        first.panel_position().set(PanelPosition::Right);
        first
            .canvas_settings()
            .update(|settings| settings.zoom = Zoom::Percent(50));
        let entry = first.selected().unwrap();
        first.args(&entry).set("n", ArgValue::Int(2)).unwrap();

        let next = run(&keeper, &PreviewRequest::default());
        assert_eq!(
            next.selection().peek().as_deref(),
            Some("fake--field--empty")
        );
        assert_eq!(next.sidebar_width().peek(), 300.0);
        assert!(next.panel_collapsed().peek());
        assert_eq!(next.panel_position().peek(), PanelPosition::Right);
        assert_eq!(
            next.canvas_settings().peek(),
            CanvasSettings {
                zoom: Zoom::Percent(50),
                ..CanvasSettings::default()
            }
        );
        let entry = next.selected().unwrap();
        assert!(next.args(&entry).overrides().is_empty());
    }

    #[test]
    fn a_requested_id_wins_over_the_stored_selection() {
        let keeper = Keeper::in_store(Arc::new(MemoryStore::default()));
        run(&keeper, &PreviewRequest::default()).select("fake--field--empty");
        let next = run(
            &keeper,
            &PreviewRequest::new(Some("fake--card--plain"), None),
        );
        assert_eq!(
            next.selection().peek().as_deref(),
            Some("fake--card--plain")
        );
    }

    #[test]
    fn a_requested_component_filters_the_sidebar_and_chooses_among_what_it_keeps() {
        let keeper = Keeper::in_store(Arc::new(MemoryStore::default()));
        run(&keeper, &PreviewRequest::default()).select("fake--field--empty");
        let next = run(&keeper, &PreviewRequest::new(None, Some("Inputs")));
        assert_eq!(next.search().peek(), "Inputs");
        assert_eq!(
            next.selection().peek().as_deref(),
            Some("fake--field--empty"),
            "the stored selection is among what the filter keeps"
        );
        let next = run(&keeper, &PreviewRequest::new(None, Some("Card")));
        assert_eq!(
            next.selection().peek().as_deref(),
            Some("fake--card--plain"),
            "the stored selection is not"
        );
    }

    #[test]
    fn nothing_is_kept_without_a_store() {
        let first = run(&Keeper::none(), &PreviewRequest::default());
        first.select("fake--card--plain");
        let next = run(&Keeper::none(), &PreviewRequest::default());
        assert_eq!(
            next.selection().peek().as_deref(),
            Some("fake--button--primary")
        );
    }
}

mod writes {
    use std::sync::Mutex;

    use telar::testing::advance_time;
    use telar::{dispose_owner, owner_scope, signal};

    use super::*;

    /// A store in memory that tells how many times it was written.
    #[derive(Default)]
    struct Counting {
        values: MemoryStore,
        writes: Mutex<u32>,
    }

    impl Counting {
        fn writes(&self) -> u32 {
            *self.writes.lock().unwrap()
        }
    }

    impl PreferenceStore for Counting {
        fn get(&self, key: &str) -> Option<String> {
            self.values.get(key)
        }

        fn set(&self, key: &str, value: Option<&str>) {
            *self.writes.lock().unwrap() += 1;
            self.values.set(key, value);
        }
    }

    #[test]
    fn a_drag_is_written_once_the_state_rests() {
        let store = Arc::new(Counting::default());
        let keeper = Keeper::in_store(store.clone());
        let width = signal(200.0_f32);
        keeper.keep(width, keys::SIDEBAR_WIDTH);
        for step in 1..=20 {
            width.set(200.0 + step as f32);
            advance_time(WRITE_DELAY / 4);
        }
        assert_eq!(store.writes(), 0, "every step starts the wait again");
        assert_eq!(keeper.stored::<f32>(keys::SIDEBAR_WIDTH), Some(220.0));

        advance_time(WRITE_DELAY);
        assert_eq!(store.writes(), 1);
        assert_eq!(store.get(keys::SIDEBAR_WIDTH).as_deref(), Some("220.0"));

        advance_time(WRITE_DELAY);
        assert_eq!(store.writes(), 1, "nothing is left to write");
    }

    #[test]
    fn a_flush_writes_what_is_waiting_at_once() {
        let store = Arc::new(Counting::default());
        let keeper = Keeper::in_store(store.clone());
        let collapsed = signal(false);
        keeper.keep(collapsed, keys::PANEL_COLLAPSED);
        collapsed.set(true);
        keeper.flush();
        assert_eq!(store.get(keys::PANEL_COLLAPSED).as_deref(), Some("true"));
        advance_time(WRITE_DELAY);
        assert_eq!(store.writes(), 1, "the wait ended with the flush");
    }

    #[test]
    fn what_is_waiting_is_written_when_the_state_goes() {
        let store = Arc::new(Counting::default());
        let owner = owner_scope();
        let id = owner.id();
        let size = signal(240.0_f32);
        Keeper::in_store(store.clone()).keep(size, keys::PANEL_SIZE);
        drop(owner);
        size.set(300.0);
        dispose_owner(id);
        assert_eq!(store.get(keys::PANEL_SIZE).as_deref(), Some("300.0"));
    }
}
