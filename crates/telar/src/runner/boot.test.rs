use std::path::{Path, PathBuf};
use std::sync::Mutex;

use platform_core::SurfaceId;
use platform_headless::{HeadlessPlatform, HeadlessWindow};
use services_core::{MemoryStore, NoPaths, PreferenceStore};
use test_process::OwnProcess;
use theme_core::SchemePreference;

use super::*;
use crate::app::App;
use crate::app_runtime::LocalApp;
use crate::user_preferences::SCHEME_KEY;

const ROOT: &str = "TELAR_BOOT_TEST_ROOT";
const STAGE: &str = "TELAR_BOOT_TEST_STAGE";
const APP: &str = "boot-probe";

struct Probe;

impl App for Probe {
    fn root(&self) -> Box<dyn ui_tree::Component> {
        Box::new(
            ui_core::Rectangle::new(
                layout_core::LayoutStyle::new().width(4.0).height(4.0),
                || renderer_core::RectStyle::filled(renderer_core::Color::BLACK, 0.0),
            )
            .expect("a rectangle builds"),
        )
    }
}

struct Rooted(PathBuf);

impl AppPathsProvider for Rooted {
    fn config_dir(&self) -> Option<PathBuf> {
        Some(self.0.clone())
    }
    fn data_dir(&self) -> Option<PathBuf> {
        Some(self.0.clone())
    }
    fn cache_dir(&self) -> Option<PathBuf> {
        Some(self.0.clone())
    }
}

/// Runs `check` in a process of its own, handed a config root in which `APP` keeps a dark scheme: the app's directories and preference store belong to the process, and the first boot in it keeps them.
fn in_own_process(test: &str, check: fn(&Path)) {
    in_own_processes(test, &[check]);
}

/// Runs each of `stages` in a process of its own, one after another over the same config root: what one boot kept is what the next finds.
fn in_own_processes(test: &str, stages: &[fn(&Path)]) {
    let mut own = OwnProcess::new(module_path!(), test);
    if own.is_current() {
        let root = std::env::var_os(ROOT).expect("the config root");
        let stage: usize = std::env::var(STAGE)
            .expect("the stage")
            .parse()
            .expect("a stage number");
        stages[stage](Path::new(&root));
        return;
    }
    let root = std::env::temp_dir().join(format!("telar-boot-{}-{test}", std::process::id()));
    std::fs::create_dir_all(root.join(APP)).expect("create the config root");
    std::fs::write(
        root.join(APP).join("preferences"),
        format!("{SCHEME_KEY}\tdark\n"),
    )
    .expect("keep a scheme");
    own.env(ROOT, &root);
    for stage in 0..stages.len() {
        own.env(STAGE, stage.to_string()).passes();
    }
    let _ = std::fs::remove_dir_all(&root);
}

fn assert_booted_as_the_app(root: &Path) {
    assert_eq!(services_core::app_paths::name().as_deref(), Some(APP));
    assert_eq!(services_core::app_paths::config(), Some(root.join(APP)));
    assert_eq!(
        theme_core::scheme_preference(),
        SchemePreference::Dark,
        "the scheme the app keeps is brought back"
    );
    theme_core::set_scheme_preference(SchemePreference::Light);
    let kept = std::fs::read_to_string(root.join(APP).join("preferences"))
        .expect("the app's preference file");
    assert!(kept.contains(&format!("{SCHEME_KEY}\tlight")), "{kept}");
}

#[test]
fn a_single_surface_run_installs_the_app_under_its_name() {
    in_own_process(
        "a_single_surface_run_installs_the_app_under_its_name",
        |root| {
            crate::run_with_platform::<_, _, ()>(
                HeadlessPlatform::new(16, 16),
                AppConfig::default(),
                Arc::new(Rooted(root.to_owned())),
                Probe,
                APP,
            )
            .expect("the run completes");
            assert_booted_as_the_app(root);
        },
    );
}

#[test]
fn a_multi_surface_run_installs_the_app_under_its_name() {
    in_own_process(
        "a_multi_surface_run_installs_the_app_under_its_name",
        |root| {
            let paths = root.to_owned();
            crate::run_multi_with_platform(
                HeadlessPlatform::new(16, 16),
                vec![(SurfaceId(0), AppConfig::default())],
                move |_| Arc::new(Rooted(paths.clone())) as Arc<dyn AppPathsProvider>,
                |_| Probe,
                APP,
            )
            .expect("the run completes");
            assert_booted_as_the_app(root);
        },
    );
}

#[test]
fn a_surface_handler_built_at_runtime_installs_the_app_under_its_name() {
    in_own_process(
        "a_surface_handler_built_at_runtime_installs_the_app_under_its_name",
        |root| {
            let _handler = crate::build_surface_handler::<HeadlessWindow, _>(
                LocalApp(Probe),
                Arc::new(Rooted(root.to_owned())),
                APP,
                AppConfig::default(),
            );
            assert_booted_as_the_app(root);
        },
    );
}

#[cfg(all(
    feature = "desktop-bare",
    not(target_os = "android"),
    not(target_arch = "wasm32")
))]
#[test]
fn a_second_window_opens_as_the_running_app() {
    in_own_process("a_second_window_opens_as_the_running_app", |root| {
        services_core::app_paths::install(APP, Arc::new(Rooted(root.to_owned())));
        let _window = crate::open_window(AppConfig::default(), Probe);
        assert_booted_as_the_app(root);
    });
}

#[derive(Default)]
struct Recording {
    kept: MemoryStore,
    schemes_written: Mutex<Vec<Option<String>>>,
}

impl PreferenceStore for Recording {
    fn get(&self, key: &str) -> Option<String> {
        self.kept.get(key)
    }

    fn set(&self, key: &str, value: Option<&str>) {
        if key == SCHEME_KEY {
            self.schemes_written
                .lock()
                .unwrap()
                .push(value.map(str::to_owned));
        }
        self.kept.set(key, value);
    }
}

fn runtime_surface() -> Box<dyn platform_core::EventHandler<HeadlessWindow>> {
    crate::build_surface_handler::<HeadlessWindow, _>(
        LocalApp(Probe),
        Arc::new(NoPaths),
        APP,
        AppConfig::default(),
    )
}

#[test]
fn surfaces_on_one_thread_follow_the_stored_choices_once_and_each_thread_follows_them() {
    in_own_process(
        "surfaces_on_one_thread_follow_the_stored_choices_once_and_each_thread_follows_them",
        |_| {
            let store = Arc::new(Recording::default());
            store.set(SCHEME_KEY, Some("dark"));
            services_core::set_preference_store(store.clone());

            let _first = runtime_surface();
            let _second = runtime_surface();
            assert_eq!(theme_core::scheme_preference(), SchemePreference::Dark);
            store.schemes_written.lock().unwrap().clear();
            theme_core::set_scheme_preference(SchemePreference::Light);
            assert_eq!(
                *store.schemes_written.lock().unwrap(),
                [Some("light".to_owned())],
                "one change is kept once"
            );

            std::thread::spawn(|| {
                let _surface = runtime_surface();
                assert_eq!(
                    theme_core::scheme_preference(),
                    SchemePreference::Light,
                    "a surface on another thread brings the kept choice back there"
                );
            })
            .join()
            .expect("the other thread's surface boots");
        },
    );
}

fn boot_under(root: &Path) -> SurfaceBoot {
    SurfaceBoot::new(
        APP,
        Arc::new(Rooted(root.to_owned())),
        FontSetup::default(),
        StoredChoices::Now,
    )
}

#[test]
fn the_backend_and_the_history_kept_by_one_boot_are_found_by_the_next_and_no_prefs_file_is_written()
{
    in_own_processes(
        "the_backend_and_the_history_kept_by_one_boot_are_found_by_the_next_and_no_prefs_file_is_written",
        &[
            |root| {
                let boot = boot_under(root);
                assert_eq!(boot.backend, config::compile_time_backend());
                crate::user_preferences::store_backend(RendererBackend::Software);
                crate::user_preferences::store_history(&[
                    "/".to_owned(),
                    "/es/projects".to_owned(),
                ]);
            },
            |root| {
                let boot = boot_under(root);
                assert_eq!(boot.backend, RendererBackend::Software);
                assert_eq!(
                    crate::user_preferences::stored_history(),
                    ["/", "/es/projects"]
                );
                let names: Vec<_> = std::fs::read_dir(root.join(APP))
                    .expect("the app's config directory")
                    .map(|entry| entry.expect("an entry").file_name())
                    .collect();
                assert_eq!(names, ["preferences"], "the store's file is the only one");
            },
        ],
    );
}
