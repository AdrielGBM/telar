use crate::app::App;
use crate::runner::resolved_window;

// The order the two window sources resolve in, pinned so a boot-path merge cannot flip it silently: the dev overrides land on the caller's config, and `App::window_config` replaces it outright.
#[test]
fn the_app_window_config_wins_over_the_dev_overrides() {
    struct Opinionated;
    impl App for Opinionated {
        fn root(&self) -> Box<dyn ui_tree::Component> {
            unreachable!("window resolution never builds the tree")
        }
        fn window_config(&self) -> Option<platform_core::WindowConfig> {
            Some(platform_core::WindowConfig {
                title: "from the app".to_string(),
                ..Default::default()
            })
        }
    }
    struct Indifferent;
    impl App for Indifferent {
        fn root(&self) -> Box<dyn ui_tree::Component> {
            unreachable!("window resolution never builds the tree")
        }
    }

    let dev_window = platform_core::WindowConfig {
        title: "from cargo telar dev".to_string(),
        ..Default::default()
    };
    assert_eq!(
        resolved_window(dev_window.clone(), &Opinionated).title,
        "from the app"
    );
    assert_eq!(
        resolved_window(dev_window, &Indifferent).title,
        "from cargo telar dev"
    );
}
