use std::sync::Arc;

use test_process::OwnProcess;

use super::*;

struct FakeProvider {
    config: Option<PathBuf>,
    data: Option<PathBuf>,
    cache: Option<PathBuf>,
    state: Option<PathBuf>,
    runtime: Option<PathBuf>,
}

impl AppPathsProvider for FakeProvider {
    fn config_dir(&self) -> Option<PathBuf> {
        self.config.clone()
    }

    fn data_dir(&self) -> Option<PathBuf> {
        self.data.clone()
    }

    fn cache_dir(&self) -> Option<PathBuf> {
        self.cache.clone()
    }

    fn state_dir(&self) -> Option<PathBuf> {
        self.state.clone()
    }

    fn runtime_dir(&self) -> Option<PathBuf> {
        self.runtime.clone()
    }
}

struct RequiredOnly {
    data: PathBuf,
}

impl AppPathsProvider for RequiredOnly {
    fn config_dir(&self) -> Option<PathBuf> {
        None
    }

    fn data_dir(&self) -> Option<PathBuf> {
        Some(self.data.clone())
    }

    fn cache_dir(&self) -> Option<PathBuf> {
        None
    }
}

fn provider_under(root: &str) -> Arc<dyn AppPathsProvider> {
    let root = PathBuf::from(root);
    Arc::new(FakeProvider {
        config: Some(root.join("config")),
        data: Some(root.join("data")),
        cache: Some(root.join("cache")),
        state: Some(root.join("state")),
        runtime: Some(root.join("runtime")),
    })
}

// `install` keeps the first call for the whole process, so each case runs in a process of its own.
#[test]
fn nothing_is_answered_before_a_runner_installs_one() {
    OwnProcess::new(
        module_path!(),
        "nothing_is_answered_before_a_runner_installs_one",
    )
    .run(|| {
        assert_eq!(name(), None);
        assert_eq!(config(), None);
        assert_eq!(data(), None);
        assert_eq!(cache(), None);
        assert_eq!(state(), None);
        assert_eq!(runtime(), None);
    });
}

#[test]
fn install_records_the_app_name() {
    OwnProcess::new(module_path!(), "install_records_the_app_name").run(|| {
        install("telar-app", provider_under("/fake"));

        assert_eq!(name().as_deref(), Some("telar-app"));
    });
}

#[test]
fn every_app_directory_is_the_provider_directory_nested_under_the_app_name() {
    OwnProcess::new(
        module_path!(),
        "every_app_directory_is_the_provider_directory_nested_under_the_app_name",
    )
    .run(|| {
        install("telar-app", provider_under("/fake"));

        assert_eq!(config(), Some(PathBuf::from("/fake/config/telar-app")));
        assert_eq!(data(), Some(PathBuf::from("/fake/data/telar-app")));
        assert_eq!(cache(), Some(PathBuf::from("/fake/cache/telar-app")));
        assert_eq!(state(), Some(PathBuf::from("/fake/state/telar-app")));
        assert_eq!(runtime(), Some(PathBuf::from("/fake/runtime/telar-app")));
    });
}

#[test]
fn a_directory_the_provider_does_not_offer_stays_none_under_an_installed_name() {
    OwnProcess::new(
        module_path!(),
        "a_directory_the_provider_does_not_offer_stays_none_under_an_installed_name",
    )
    .run(|| {
        install(
            "telar-app",
            Arc::new(FakeProvider {
                config: None,
                data: Some(PathBuf::from("/fake/data")),
                cache: None,
                state: None,
                runtime: None,
            }),
        );

        assert_eq!(config(), None);
        assert_eq!(cache(), None);
        assert_eq!(runtime(), None);
        assert_eq!(data(), Some(PathBuf::from("/fake/data/telar-app")));
    });
}

#[test]
fn state_falls_back_to_data_and_runtime_is_none_for_a_provider_with_only_required_methods() {
    OwnProcess::new(
        module_path!(),
        "state_falls_back_to_data_and_runtime_is_none_for_a_provider_with_only_required_methods",
    )
    .run(|| {
        install(
            "telar-app",
            Arc::new(RequiredOnly {
                data: PathBuf::from("/fake/data"),
            }),
        );

        assert_eq!(state(), Some(PathBuf::from("/fake/data/telar-app")));
        assert_eq!(runtime(), None);
    });
}

#[test]
fn a_second_install_is_ignored() {
    OwnProcess::new(module_path!(), "a_second_install_is_ignored").run(|| {
        install("first", provider_under("/first"));
        install("second", provider_under("/second"));

        assert_eq!(name().as_deref(), Some("first"));
        assert_eq!(config(), Some(PathBuf::from("/first/config/first")));
    });
}

#[test]
fn home_is_the_home_variable_when_it_names_a_path() {
    let mut own = OwnProcess::new(
        module_path!(),
        "home_is_the_home_variable_when_it_names_a_path",
    );
    own.env("HOME", "/fake/home");
    own.run(|| {
        assert_eq!(home(), Some(PathBuf::from("/fake/home")));
    });
}

#[test]
fn home_is_none_when_the_home_variable_is_empty() {
    let mut own = OwnProcess::new(
        module_path!(),
        "home_is_none_when_the_home_variable_is_empty",
    );
    own.env("HOME", "");
    own.run(|| {
        assert_eq!(home(), None);
    });
}
