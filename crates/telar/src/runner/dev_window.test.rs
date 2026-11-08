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
        resolved_window(dev_window.clone(), &Opinionated, "pkg").title,
        "from the app"
    );
    assert_eq!(
        resolved_window(dev_window, &Indifferent, "pkg").title,
        "from cargo telar dev"
    );
}

#[test]
fn a_window_nobody_titled_is_named_after_the_package() {
    struct Silent;
    impl App for Silent {
        fn root(&self) -> Box<dyn ui_tree::Component> {
            unreachable!("window resolution never builds the tree")
        }
    }
    assert_eq!(
        resolved_window(platform_core::WindowConfig::default(), &Silent, "my-app").title,
        "my-app"
    );
}

#[cfg(feature = "hot-reload")]
#[test]
fn the_window_cargo_telar_dev_encodes_is_the_window_the_app_opens() {
    use telar_project::WindowSection;
    use telar_project::protocol::{
        FullscreenMode, WindowPosition, dev_window_env, dev_window_from_env,
    };

    let section = WindowSection {
        title: Some("dev".to_string()),
        width: Some(640),
        height: Some(480),
        decorations: Some(false),
        resizable: Some(false),
        transparent: Some(true),
        fullscreen: Some(FullscreenMode::Borderless),
        position: Some(WindowPosition::At { x: 10, y: -20 }),
    };
    let env = dev_window_env(&section);
    let overrides = dev_window_from_env(|name| {
        env.iter()
            .find(|(var, _)| *var == name)
            .map(|(_, value)| value.clone())
    })
    .expect("what cargo telar dev writes reads back");

    let mut window = platform_core::WindowConfig::default();
    super::apply(overrides, &mut window);
    assert_eq!(window.title, "dev");
    assert_eq!((window.width, window.height), (640, 480));
    assert!(!window.has_decorations);
    assert!(!window.is_resizable);
    assert!(window.is_transparent);
    assert_eq!(window.fullscreen, platform_core::FullscreenMode::Borderless);
    assert_eq!(window.position, platform_core::WindowPosition::At(10, -20));
}
