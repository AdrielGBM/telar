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
    let matrix = WorkshopRoute::new(Page::Matrix("demo--card--a".into()), true);
    assert_eq!(written(&matrix.to_location()), "/matrix/demo--card--a");
    let docs = WorkshopRoute::new(Page::Docs("Inputs/Button".into()), true);
    assert_eq!(written(&docs.to_location()), "/docs/Inputs/Button");
    for route in [preview, matrix, docs] {
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
    assert_eq!(route, WorkshopRoute::new(route.page.clone(), false));
    assert_eq!(
        written(&route.to_location()),
        "/preview/demo--card--a?chrome=0"
    );
}

#[test]
fn an_address_the_workshop_has_no_page_for_is_none() {
    for reference in [
        "/",
        "/preview",
        "/settings/a",
        "/docs",
        "/preview/a/b",
        "/matrix",
        "/matrix/a/b",
    ] {
        assert_eq!(
            WorkshopRoute::from_location(&at(reference)),
            None,
            "{reference}"
        );
    }
}

#[test]
fn a_copied_link_carries_args_and_settings() {
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
        args: vec![
            ("label".into(), ArgValue::Text("Save; \"all\": now".into())),
            ("disabled".into(), ArgValue::Bool(true)),
            ("size".into(), ArgValue::Choice("Large".into())),
        ],
        settings: Some(settings),
        ..WorkshopRoute::new(Page::Preview("demo--button--primary".into()), true)
    };
    let link = written(&route.to_location());
    assert!(
        link.starts_with("/preview/demo--button--primary?args="),
        "{link}"
    );
    let back = WorkshopRoute::from_location(&at(&link)).unwrap();
    assert_eq!(back, route);
    let reported = WorkshopRoute {
        settings: None,
        ..back
    };
    let address = written(&reported.to_location());
    assert!(
        address.starts_with("/preview/demo--button--primary?args=") && !address.contains("globals"),
        "the address the workshop reports keeps the args alone: {address}"
    );
}

#[test]
fn a_page_with_nothing_set_is_its_page_alone() {
    let route = WorkshopRoute {
        settings: Some(CanvasSettings::default()),
        ..WorkshopRoute::new(Page::Preview("demo--card--a".into()), true)
    };
    assert_eq!(written(&route.to_location()), "/preview/demo--card--a");
}

#[test]
fn a_matrix_page_carries_its_axes_and_its_focused_cell() {
    let route = WorkshopRoute {
        axes: Some(vec![
            CustomAxis::Mode,
            CustomAxis::Direction,
            CustomAxis::Arg("size".into()),
        ]),
        cell: Some("demo--card--a--mode=dark--dir=rtl--size=Large".into()),
        ..WorkshopRoute::new(Page::Matrix("demo--card--a".into()), true)
    };
    let address = written(&route.to_location());
    assert_eq!(
        address,
        "/matrix/demo--card--a?axes=mode;dir;arg.size&cell=mode%3Ddark--dir%3Drtl--size%3DLarge"
    );
    assert_eq!(WorkshopRoute::from_location(&at(&address)), Some(route));
}

#[test]
fn a_matrix_built_from_no_axes_is_told_from_the_preview_s_own() {
    let built = WorkshopRoute {
        axes: Some(Vec::new()),
        ..WorkshopRoute::new(Page::Matrix("demo--card--a".into()), true)
    };
    let address = written(&built.to_location());
    assert_eq!(address, "/matrix/demo--card--a?axes=");
    assert_eq!(WorkshopRoute::from_location(&at(&address)), Some(built));
    let own = WorkshopRoute::from_location(&at("/matrix/demo--card--a")).unwrap();
    assert_eq!(own.axes, None);
}

#[test]
fn only_a_matrix_page_reads_axes_and_a_cell_and_a_cell_of_another_preview_is_left_out() {
    let preview =
        WorkshopRoute::from_location(&at("/preview/demo--card--a?axes=mode&cell=mode%3Ddark"))
            .unwrap();
    assert_eq!((preview.axes, preview.cell), (None, None));
    let elsewhere = WorkshopRoute {
        cell: Some("demo--card--b--mode=dark".into()),
        ..WorkshopRoute::new(Page::Matrix("demo--card--a".into()), true)
    };
    assert_eq!(written(&elsewhere.to_location()), "/matrix/demo--card--a");
}

#[test]
fn axes_this_workshop_cannot_build_are_passed_over() {
    assert_eq!(
        parse_axes("mode; teleport ;arg.;arg.flag;viewport"),
        [
            CustomAxis::Mode,
            CustomAxis::Arg("flag".into()),
            CustomAxis::Viewport
        ]
    );
    let every = [
        CustomAxis::Mode,
        CustomAxis::Locale,
        CustomAxis::Direction,
        CustomAxis::ControlSize,
        CustomAxis::Viewport,
        CustomAxis::Arg("tone".into()),
    ];
    assert_eq!(parse_axes(&axes_text(&every)), every);
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

#[test]
fn every_page_an_export_writes_opens_its_own_route_at_the_directory_a_static_host_serves() {
    use telar_project::protocol::{ListedPreview, workshop_pages};
    let listed = |id: &str, title: &str| ListedPreview {
        id: id.to_string(),
        title: title.to_string(),
    };
    let pages = workshop_pages(&[
        listed("demo--card--a", "Inputs/Card"),
        listed("demo--text field--ñ", "Text field"),
    ]);
    let expected = [
        Page::Preview("demo--card--a".into()),
        Page::Matrix("demo--card--a".into()),
        Page::Docs("Inputs/Card".into()),
        Page::Preview("demo--text field--ñ".into()),
        Page::Matrix("demo--text field--ñ".into()),
        Page::Docs("Text field".into()),
    ];
    assert_eq!(pages.len(), expected.len());
    for (page, expected) in pages.iter().zip(expected) {
        let route = WorkshopRoute::new(expected, true);
        assert_eq!(route.to_location().segments(), page.segments.as_slice());
        let directory = format!("{}/", written(&route.to_location()));
        assert_eq!(WorkshopRoute::from_location(&at(&directory)), Some(route));
    }
}

const AWKWARD_TITLES: [&str; 7] = [
    "Forms / Button",
    "Forms/Button",
    " Forms  /  Text field ",
    "Formulários / Botão ñ",
    "100% / a?b#c",
    "Tom & Jerry / a+b",
    "Single title",
];

#[test]
fn a_docs_title_reaches_its_route_as_its_canonical_form_and_back() {
    use telar_project::protocol::canonical_title;
    for title in AWKWARD_TITLES {
        let route = WorkshopRoute::new(Page::Docs(canonical_title(title).into()), true);
        let written = written(&route.to_location());
        assert_eq!(
            WorkshopRoute::from_location(&at(&written)),
            Some(route),
            "{title}"
        );
    }
}

#[test]
fn every_awkward_title_an_export_writes_opens_its_own_route() {
    use telar_project::protocol::{ListedPreview, canonical_title, workshop_pages};
    let previews: Vec<_> = AWKWARD_TITLES
        .iter()
        .enumerate()
        .map(|(at, title)| ListedPreview {
            id: format!("p{at}"),
            title: title.to_string(),
        })
        .collect();
    for (at, title) in AWKWARD_TITLES.iter().enumerate() {
        let route = WorkshopRoute::new(Page::Docs(canonical_title(title).into()), true);
        let page = workshop_pages(&previews[at..=at]).pop().unwrap();
        assert_eq!(route.to_location().segments(), page.segments.as_slice());
    }
}
