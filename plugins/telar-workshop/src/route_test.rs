use telar::{ControlSize, Direction, LocationFormat};

use super::*;

fn at(reference: &str) -> Location {
    LocationFormat::root().parse(reference).unwrap()
}

fn written(location: &Location) -> String {
    LocationFormat::root().format(location)
}

#[test]
fn each_page_has_an_address_of_its_own() {
    let preview = WorkshopRoute::new(Page::Preview("demo--card--a".into()), true);
    assert_eq!(written(&preview.to_location()), "/preview/demo--card--a");
    let docs = WorkshopRoute::new(Page::Docs("Inputs/Button".into()), true);
    assert_eq!(written(&docs.to_location()), "/docs/Inputs/Button");
    for route in [preview, docs] {
        assert_eq!(
            WorkshopRoute::from_location(&route.to_location()),
            Some(route)
        );
    }
}

#[test]
fn the_canvas_alone_is_asked_for_with_chrome_0() {
    let route = WorkshopRoute::from_location(&at("/preview/demo--card--a?chrome=0")).unwrap();
    assert!(!route.chrome);
    assert_eq!(route.link, None);
    assert_eq!(
        written(&route.to_location()),
        "/preview/demo--card--a?chrome=0"
    );
}

#[test]
fn an_address_the_workshop_has_no_page_for_is_none() {
    for reference in ["/", "/preview", "/settings/a", "/docs", "/preview/a/b"] {
        assert_eq!(
            WorkshopRoute::from_location(&at(reference)),
            None,
            "{reference}"
        );
    }
}

#[test]
fn a_copied_link_carries_args_and_settings_until_they_are_set() {
    let settings = CanvasSettings {
        mode: Some("dark".into()),
        locale: Some("ar".into()),
        direction: Some(Direction::Rtl),
        control_size: Some(ControlSize::Large),
        background: Background::Transparent,
        size: CanvasSize::Custom {
            width: 812.5,
            height: 600.0,
        },
        rotated: true,
        zoom: Zoom::Percent(150),
        reduced_motion: true,
        high_contrast: Some(true),
        grid: true,
        rulers: true,
    };
    let route = WorkshopRoute {
        page: Page::Preview("demo--button--primary".into()),
        chrome: true,
        link: Some(Link {
            args: vec![
                ("label".into(), ArgValue::Text("Save; \"all\": now".into())),
                ("disabled".into(), ArgValue::Bool(true)),
                ("size".into(), ArgValue::Choice("Large".into())),
            ],
            settings: Some(settings),
        }),
    };
    let link = written(&route.to_location());
    assert!(
        link.starts_with("/preview/demo--button--primary?args="),
        "{link}"
    );
    let back = WorkshopRoute::from_location(&at(&link)).unwrap();
    assert_eq!(back, route);
    let opened = WorkshopRoute { link: None, ..back };
    assert_eq!(
        written(&opened.to_location()),
        "/preview/demo--button--primary"
    );
}

#[test]
fn a_link_with_nothing_edited_is_its_page_alone() {
    let route = WorkshopRoute {
        page: Page::Preview("demo--card--a".into()),
        chrome: true,
        link: Some(Link {
            args: Vec::new(),
            settings: Some(CanvasSettings::default()),
        }),
    };
    assert_eq!(written(&route.to_location()), "/preview/demo--card--a");
}

#[test]
fn each_size_is_written_as_a_word_and_read_back() {
    for size in [
        CanvasSize::Fill,
        CanvasSize::Preset("mobile".into()),
        CanvasSize::Project("Watch face".into()),
        CanvasSize::Device("compact-phone".into()),
        CanvasSize::Custom {
            width: 320.0,
            height: 480.0,
        },
    ] {
        let settings = CanvasSettings {
            size,
            ..CanvasSettings::default()
        };
        assert_eq!(parse_settings(&settings_text(&settings)), settings);
    }
}

#[test]
fn words_a_link_cannot_mean_are_passed_over() {
    let settings = parse_settings("size:0x-4;zoom:lots;dir:up;teleport;mode:dark");
    assert_eq!(
        settings,
        CanvasSettings {
            mode: Some("dark".into()),
            ..CanvasSettings::default()
        }
    );
    assert_eq!(
        parse_args("n:2;broken;x:not a value;;flag:true"),
        [
            ("n".to_string(), ArgValue::Int(2)),
            ("flag".to_string(), ArgValue::Bool(true))
        ]
    );
}
