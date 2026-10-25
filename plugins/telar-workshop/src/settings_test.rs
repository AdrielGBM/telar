use super::*;

const RED: Color = Color::rgb(1.0, 0.0, 0.0);
const PROJECT: &[ViewportPreset] = &[ViewportPreset::new("watch", 198.0, 242.0)];

fn resolve(settings: &CanvasSettings, env: PreviewEnv) -> CanvasEnv {
    settings.resolve(&env, true, PROJECT, ColorScheme::Light)
}

#[test]
fn settings_left_at_their_defaults_take_what_the_preview_asks_for() {
    let env = PreviewEnv::NONE
        .mode("night")
        .locale("fr")
        .viewport(300.0, 200.0)
        .background(RED);
    assert_eq!(
        resolve(&CanvasSettings::default(), env),
        CanvasEnv {
            mode: Some("night".into()),
            locale: Some("fr".into()),
            direction: Some(Direction::Ltr),
            control_size: None,
            high_contrast: None,
            background: Some(RED),
            viewport: Some(Size::new(300.0, 200.0)),
            safe_area: Insets::default(),
        }
    );
    assert_eq!(
        resolve(&CanvasSettings::default(), PreviewEnv::NONE),
        CanvasEnv {
            mode: None,
            locale: None,
            direction: None,
            control_size: None,
            high_contrast: None,
            background: None,
            viewport: None,
            safe_area: Insets::default(),
        },
        "nothing asked for leaves the canvas to the application"
    );
}

#[test]
fn a_setting_wins_over_what_the_preview_asks_for() {
    let settings = CanvasSettings {
        mode: ModeChoice::Named("day".into()),
        locale: Some("en".into()),
        control_size: Some(ControlSize::Small),
        background: Background::Dark,
        size: CanvasSize::Preset("tablet".into()),
        ..CanvasSettings::default()
    };
    let env = PreviewEnv::NONE
        .mode("night")
        .locale("fr")
        .viewport(300.0, 200.0)
        .background(RED);
    let resolved = resolve(&settings, env);
    assert_eq!(resolved.mode.as_deref(), Some("day"));
    assert_eq!(resolved.locale.as_deref(), Some("en"));
    assert_eq!(resolved.control_size, Some(ControlSize::Small));
    assert_eq!(resolved.background, Some(DARK_BACKGROUND));
    assert_eq!(resolved.viewport, Some(Size::new(768.0, 1024.0)));
}

#[test]
fn a_locale_turns_the_direction_unless_one_is_chosen() {
    let arabic = CanvasSettings {
        locale: Some("ar".into()),
        ..CanvasSettings::default()
    };
    assert_eq!(
        resolve(&arabic, PreviewEnv::NONE).direction,
        Some(Direction::Rtl)
    );
    assert_eq!(
        resolve(&arabic, PreviewEnv::NONE.direction(Direction::Ltr)).direction,
        Some(Direction::Ltr),
        "the direction the preview names is chosen too"
    );
    let chosen = CanvasSettings {
        direction: Some(Direction::Ltr),
        ..arabic
    };
    assert_eq!(
        resolve(&chosen, PreviewEnv::NONE).direction,
        Some(Direction::Ltr)
    );
    assert_eq!(
        resolve(&CanvasSettings::default(), PreviewEnv::NONE.locale("he")).direction,
        Some(Direction::Rtl),
        "the preview's own locale turns it as well"
    );
    let english = CanvasSettings {
        locale: Some("en".into()),
        ..CanvasSettings::default()
    };
    assert_eq!(
        resolve(&english, PreviewEnv::NONE.locale("he")).direction,
        Some(Direction::Ltr),
        "the locale the toolbar sets wins over the preview's"
    );
}

#[test]
fn each_size_resolves_to_its_viewport() {
    let sized = |size: CanvasSize| {
        let settings = CanvasSettings {
            size,
            ..CanvasSettings::default()
        };
        resolve(&settings, PreviewEnv::NONE.viewport(300.0, 200.0)).viewport
    };
    assert_eq!(sized(CanvasSize::Preview), Some(Size::new(300.0, 200.0)));
    assert_eq!(sized(CanvasSize::Fill), None);
    assert_eq!(
        sized(CanvasSize::Preset("mobile".into())),
        Some(Size::new(360.0, 800.0))
    );
    assert_eq!(
        sized(CanvasSize::Project("watch".into())),
        Some(Size::new(198.0, 242.0))
    );
    assert_eq!(
        sized(CanvasSize::Device("phone".into())),
        Some(Size::new(390.0, 844.0))
    );
    assert_eq!(
        sized(CanvasSize::Custom {
            width: 640.0,
            height: 480.0
        }),
        Some(Size::new(640.0, 480.0))
    );
    assert_eq!(
        sized(CanvasSize::Preset("gone".into())),
        None,
        "a name nothing answers to any more fills the stage"
    );
}

#[test]
fn a_device_sets_the_safe_area_and_rotating_turns_it_with_the_size() {
    let mut settings = CanvasSettings {
        size: CanvasSize::Device("phone".into()),
        ..CanvasSettings::default()
    };
    let upright = resolve(&settings, PreviewEnv::NONE);
    assert_eq!(upright.viewport, Some(Size::new(390.0, 844.0)));
    assert_eq!(upright.safe_area, Insets::new(47.0, 0.0, 34.0, 0.0));

    settings.rotated = true;
    let turned = resolve(&settings, PreviewEnv::NONE);
    assert_eq!(turned.viewport, Some(Size::new(844.0, 390.0)));
    assert_eq!(turned.safe_area, Insets::new(0.0, 34.0, 0.0, 47.0));

    settings.size = CanvasSize::Preset("laptop".into());
    let laptop = resolve(&settings, PreviewEnv::NONE);
    assert_eq!(laptop.viewport, Some(Size::new(800.0, 1280.0)));
    assert_eq!(laptop.safe_area, Insets::default(), "only a device has one");
}

#[test]
fn a_fullscreen_preview_takes_no_size_and_no_safe_area() {
    let settings = CanvasSettings {
        size: CanvasSize::Device("phone".into()),
        locale: Some("ar".into()),
        ..CanvasSettings::default()
    };
    let resolved = settings.resolve(
        &PreviewEnv::NONE.viewport(300.0, 200.0),
        false,
        PROJECT,
        ColorScheme::Light,
    );
    assert_eq!(resolved.viewport, None);
    assert_eq!(resolved.safe_area, Insets::default());
    assert_eq!(resolved.direction, Some(Direction::Rtl));
}

#[test]
fn the_backgrounds_cover_or_show_the_checker() {
    let with = |background: Background| {
        let settings = CanvasSettings {
            background,
            ..CanvasSettings::default()
        };
        resolve(&settings, PreviewEnv::NONE.background(RED)).background
    };
    assert_eq!(with(Background::Preview), Some(RED));
    assert_eq!(with(Background::Transparent), Some(Color::TRANSPARENT));
    assert_eq!(with(Background::Light), Some(LIGHT_BACKGROUND));
    assert_eq!(with(Background::Dark), Some(DARK_BACKGROUND));
}

#[test]
fn fitting_never_zooms_in_and_shrinks_what_the_room_cannot_hold() {
    let room = Size::new(800.0, 600.0);
    assert_eq!(fit_scale(Size::new(400.0, 300.0), room), 1.0);
    assert_eq!(fit_scale(Size::new(1600.0, 600.0), room), 0.5);
    assert_eq!(fit_scale(Size::new(800.0, 2400.0), room), 0.25);
    assert_eq!(fit_scale(Size::new(800.0, 600.0), Size::ZERO), 1.0);
    assert_eq!(Zoom::Fit.fixed_scale(), None);
    assert_eq!(Zoom::Percent(150).fixed_scale(), Some(1.5));
}

#[test]
fn the_settings_are_kept_as_one_json_value() {
    let settings = CanvasSettings {
        mode: ModeChoice::Named("night".into()),
        locale: Some("ar".into()),
        direction: Some(Direction::Rtl),
        control_size: Some(ControlSize::Large),
        background: Background::Transparent,
        size: CanvasSize::Device("phone".into()),
        rotated: true,
        zoom: Zoom::Percent(50),
        reduced_motion: true,
        high_contrast: Some(true),
        grid: true,
        rulers: true,
    };
    let json = serde_json::to_string(&settings).unwrap();
    assert_eq!(
        json,
        r#"{"mode":"night","locale":"ar","direction":"rtl","control_size":"large","background":"transparent","size":{"device":"phone"},"rotated":true,"zoom":{"percent":50},"reduced_motion":true,"high_contrast":true,"grid":true,"rulers":true}"#
    );
    assert_eq!(
        serde_json::from_str::<CanvasSettings>(&json).unwrap(),
        settings
    );
    assert_eq!(
        serde_json::from_str::<CanvasSettings>(r#"{"zoom":"fit"}"#).unwrap(),
        CanvasSettings::default(),
        "a field missing from what was kept takes its default"
    );
}

#[test]
fn high_contrast_is_the_previews_until_the_toolbar_sets_it() {
    let asked = PreviewEnv::NONE.high_contrast(false);
    assert_eq!(
        resolve(&CanvasSettings::default(), asked).high_contrast,
        Some(false)
    );
    assert_eq!(
        resolve(&CanvasSettings::default(), PreviewEnv::NONE).high_contrast,
        None,
        "nothing asked for leaves it to the application"
    );
    let pressed = CanvasSettings {
        high_contrast: Some(true),
        ..CanvasSettings::default()
    };
    assert_eq!(resolve(&pressed, asked).high_contrast, Some(true));
}

#[test]
fn zooming_steps_through_the_zoom_steps_and_stops_at_either_end() {
    assert_eq!(Zoom::Fit.zoomed_in(), Zoom::Percent(150));
    assert_eq!(Zoom::Fit.zoomed_out(), Zoom::Percent(75));
    assert_eq!(Zoom::Percent(50).zoomed_in(), Zoom::Percent(75));
    assert_eq!(Zoom::Percent(60).zoomed_out(), Zoom::Percent(50));
    assert_eq!(Zoom::Percent(200).zoomed_in(), Zoom::Percent(200));
    assert_eq!(Zoom::Percent(25).zoomed_out(), Zoom::Percent(25));
}

#[test]
fn the_grid_and_the_rulers_start_off() {
    let settings = CanvasSettings::default();
    assert!(!settings.grid && !settings.rulers);
    assert!(
        !serde_json::from_str::<CanvasSettings>(r#"{"grid":true}"#)
            .unwrap()
            .rulers,
        "settings kept before the rulers existed still load"
    );
}

#[test]
fn the_preset_viewports_a_matrix_varies_are_the_presets_sizes() {
    let written: Vec<Size> = PRESET_VIEWPORTS
        .iter()
        .map(|text| telar::preview::parse_size(text).unwrap())
        .collect();
    let presets: Vec<Size> = PRESETS.iter().map(|preset| preset.size).collect();
    assert_eq!(written, presets);
}

/// Registers `paper` as the application's light mode, `ink` as its dark one and `sepia` beside them.
fn register_modes() {
    for (id, scheme) in [
        ("paper", Some(ColorScheme::Light)),
        ("sepia", None),
        ("ink", Some(ColorScheme::Dark)),
    ] {
        telar::register_mode(id, || {});
        if let Some(scheme) = scheme {
            telar::set_mode_scheme(id, scheme);
        }
    }
}

fn mode_under(mode: ModeChoice, env: PreviewEnv, scheme: ColorScheme) -> Option<String> {
    let settings = CanvasSettings {
        mode,
        ..CanvasSettings::default()
    };
    settings.resolve(&env, true, PROJECT, scheme).mode
}

#[test]
fn the_system_choice_shows_the_application_s_mode_for_the_scheme_the_workshop_is_in() {
    register_modes();
    let system = |scheme| mode_under(ModeChoice::System, PreviewEnv::NONE, scheme);
    assert_eq!(system(ColorScheme::Light).as_deref(), Some("paper"));
    assert_eq!(system(ColorScheme::Dark).as_deref(), Some("ink"));
    assert_eq!(
        mode_under(
            ModeChoice::System,
            PreviewEnv::NONE.mode("sepia"),
            ColorScheme::Dark
        )
        .as_deref(),
        Some("sepia"),
        "a preview asking for a mode keeps it"
    );
}

#[test]
fn a_picked_mode_holds_whatever_the_scheme_and_the_preview_ask_for() {
    register_modes();
    let asking = PreviewEnv::NONE.mode("sepia");
    for scheme in [ColorScheme::Light, ColorScheme::Dark] {
        assert_eq!(
            mode_under(ModeChoice::Light, asking, scheme).as_deref(),
            Some("paper")
        );
        assert_eq!(
            mode_under(ModeChoice::Dark, asking, scheme).as_deref(),
            Some("ink")
        );
        assert_eq!(
            mode_under(ModeChoice::Named("ink".into()), asking, scheme).as_deref(),
            Some("ink")
        );
    }
}

#[test]
fn a_scheme_the_application_has_no_mode_for_leaves_the_preview_its_own() {
    telar::register_mode("sepia", || {});
    let asking = PreviewEnv::NONE.mode("sepia");
    assert_eq!(
        mode_under(ModeChoice::Dark, asking, ColorScheme::Light).as_deref(),
        Some("sepia")
    );
    assert_eq!(
        mode_under(ModeChoice::Dark, PreviewEnv::NONE, ColorScheme::Light),
        None
    );
    assert_eq!(
        mode_under(ModeChoice::System, PreviewEnv::NONE, ColorScheme::Dark),
        None,
        "nothing to show the scheme with leaves the canvas to the application"
    );
}

#[test]
fn the_mode_choices_are_the_system_the_application_s_schemes_and_its_other_modes() {
    telar::register_mode("dark", || {});
    telar::register_mode("sepia", || {});
    assert_eq!(
        mode_choices(),
        [ModeChoice::System, ModeChoice::Named("sepia".into())],
        "a scheme with no mode for it is not offered, and the word for it names no mode"
    );
    register_modes();
    assert_eq!(
        mode_choices(),
        [
            ModeChoice::System,
            ModeChoice::Light,
            ModeChoice::Dark,
            ModeChoice::Named("sepia".into()),
        ]
    );
    assert_eq!(
        mode_choices()
            .iter()
            .map(ModeChoice::label)
            .collect::<Vec<_>>(),
        ["System", "Light", "Dark", "sepia"]
    );
}

#[test]
fn a_kept_mode_reads_back_from_what_earlier_workshops_kept() {
    let kept = |json: &str| serde_json::from_str::<CanvasSettings>(json).unwrap().mode;
    assert_eq!(kept(r#"{"mode":null}"#), ModeChoice::System);
    assert_eq!(kept("{}"), ModeChoice::System);
    assert_eq!(
        kept(r#"{"mode":"night"}"#),
        ModeChoice::Named("night".into())
    );
    assert_eq!(kept(r#"{"mode":"dark"}"#), ModeChoice::Dark);
    for mode in [
        ModeChoice::System,
        ModeChoice::Light,
        ModeChoice::Dark,
        ModeChoice::Named("night".into()),
    ] {
        let settings = CanvasSettings {
            mode: mode.clone(),
            ..CanvasSettings::default()
        };
        let json = serde_json::to_string(&settings).unwrap();
        assert!(
            json.starts_with(&format!(r#"{{"mode":"{}""#, mode.word())),
            "{json}"
        );
        assert_eq!(kept(&json), mode);
    }
}

#[test]
fn a_mode_is_chosen_as_the_scheme_it_stands_for() {
    register_modes();
    assert_eq!(ModeChoice::for_mode("paper"), ModeChoice::Light);
    assert_eq!(ModeChoice::for_mode("ink"), ModeChoice::Dark);
    assert_eq!(
        ModeChoice::for_mode("sepia"),
        ModeChoice::Named("sepia".into())
    );
}
