use super::*;
use telar::{
    SystemPreferences, owner_scope, set_system_preferences, set_theme, use_theme, use_theme_tokens,
};

const AA_TEXT: f32 = 4.5;

fn linear(channel: f32) -> f32 {
    if channel <= 0.04045 {
        channel / 12.92
    } else {
        ((channel + 0.055) / 1.055).powf(2.4)
    }
}

fn luminance(c: Color) -> f32 {
    0.2126 * linear(c.r) + 0.7152 * linear(c.g) + 0.0722 * linear(c.b)
}

fn over(top: Color, base: Color) -> Color {
    let mix = |t: f32, b: f32| t * top.a + b * (1.0 - top.a);
    Color::rgba(
        mix(top.r, base.r),
        mix(top.g, base.g),
        mix(top.b, base.b),
        1.0,
    )
}

fn contrast(a: Color, b: Color) -> f32 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

fn variants() -> [(&'static str, WorkbenchTheme); 2] {
    [
        ("light", WorkbenchTheme::light()),
        ("dark", WorkbenchTheme::dark()),
    ]
}

fn assert_aa(variant: &str, what: &str, foreground: Color, background: Color) {
    let ratio = contrast(foreground, background);
    assert!(ratio >= AA_TEXT, "{variant}: {what} is {ratio:.2}:1");
}

#[derive(Clone, ThemeTokens)]
#[theme(default(
    on_primary,
    radius,
    spacing,
    icon_size,
    muted,
    scrollbar,
    ink,
    surface,
    surface_alt,
    border,
    success,
    warning,
    error,
    info,
    highlight_low,
    highlight_med,
    highlight_high
))]
struct AppTheme {
    primary: Color,
}

const APP_PRIMARY: Color = Color::rgba(1.0, 0.0, 0.0, 1.0);

fn system(color_scheme: Option<ColorScheme>) {
    set_system_preferences(SystemPreferences {
        color_scheme,
        ..SystemPreferences::default()
    });
}

#[test]
fn text_reads_aa_on_every_surface_it_sits_on() {
    for (name, theme) in variants() {
        let wb = &theme.workbench;
        for (surface_name, surface) in [
            ("panel", wb.panel_background),
            ("sidebar", wb.sidebar_background),
            ("canvas backdrop", wb.canvas_backdrop),
            ("chip", theme.surface_alt),
        ] {
            assert_aa(name, &format!("ink on {surface_name}"), theme.ink, surface);
        }
        for (surface_name, surface) in [
            ("panel", wb.panel_background),
            ("sidebar", wb.sidebar_background),
            ("canvas backdrop", wb.canvas_backdrop),
            ("chip", theme.surface_alt),
        ] {
            assert_aa(
                name,
                &format!("muted on {surface_name}"),
                theme.muted,
                surface,
            );
        }
    }
}

#[test]
fn the_accent_reads_aa_as_text_and_carries_its_label() {
    for (name, theme) in variants() {
        let wb = &theme.workbench;
        assert_aa(name, "accent on panel", theme.primary, wb.panel_background);
        assert_aa(
            name,
            "accent on sidebar",
            theme.primary,
            wb.sidebar_background,
        );
        assert_aa(
            name,
            "on_primary on accent",
            theme.on_primary,
            theme.primary,
        );
    }
}

#[test]
fn a_selected_row_keeps_its_text_readable() {
    for (name, theme) in variants() {
        let wb = &theme.workbench;
        for base in [wb.panel_background, wb.sidebar_background] {
            let row = over(wb.selection, base);
            assert_aa(name, "ink on selection", theme.ink, row);
            assert_aa(name, "accent on selection", theme.primary, row);
            assert_aa(name, "muted on selection", theme.muted, row);
        }
    }
}

#[test]
fn status_colours_read_aa_on_the_panel() {
    for (name, theme) in variants() {
        let panel = theme.workbench.panel_background;
        for (what, colour) in [
            ("success", theme.success),
            ("warning", theme.warning),
            ("error", theme.error),
            ("info", theme.info),
        ] {
            assert_aa(name, what, colour, panel);
        }
    }
}

#[test]
fn the_checker_pair_is_two_distinct_tones() {
    for (name, theme) in variants() {
        let wb = &theme.workbench;
        assert_ne!(wb.checker_a, wb.checker_b, "{name}");
    }
}

#[test]
fn the_workbench_keeps_its_grid_radius_and_text_size() {
    for (_, theme) in variants() {
        assert_eq!(theme.spacing(), 8.0);
        assert_eq!(theme.radius(), 6.0);
        assert_eq!(theme.radius_lg(), 6.0);
        assert_eq!(theme.workbench.mono_family, FontFamily::Monospace);
        assert!(theme.workbench.mono_size < WORKBENCH_TEXT_SIZE);
    }
    assert_eq!(WORKBENCH_TEXT_SIZE, 13.0);
    assert_eq!(WORKBENCH_CONTROL_SIZE, ControlSize::Regular);
}

#[test]
fn the_shared_tokens_resolve_inside_the_scope_and_not_outside() {
    set_theme(AppTheme {
        primary: APP_PRIMARY,
    });
    system(Some(ColorScheme::Light));

    let outside = use_workbench_tokens();
    assert_eq!(outside, WorkbenchTokens::default());
    assert_eq!(use_theme_tokens().primary(), APP_PRIMARY);

    {
        let _scope = owner_scope();
        workbench_theme().provide();
        assert_eq!(
            use_theme_tokens().primary(),
            WorkbenchTheme::light().primary
        );
        assert_eq!(use_theme_tokens().radius(), 6.0);
        assert_eq!(use_theme_tokens().spacing(), 8.0);
        assert_eq!(use_workbench_tokens(), WorkbenchTokens::light());
        assert_eq!(
            use_theme::<WorkbenchTheme>().ink,
            WorkbenchTheme::light().ink
        );
    }

    assert_eq!(use_theme_tokens().primary(), APP_PRIMARY);
    assert_eq!(use_workbench_tokens(), outside);
}

#[test]
fn the_scope_follows_the_system_scheme() {
    system(Some(ColorScheme::Light));
    let _scope = owner_scope();
    workbench_theme().provide();
    assert_eq!(
        use_theme_tokens().surface(),
        WorkbenchTheme::light().surface
    );

    system(Some(ColorScheme::Dark));
    assert_eq!(use_theme_tokens().surface(), WorkbenchTheme::dark().surface);
    assert_eq!(use_workbench_tokens(), WorkbenchTokens::dark());

    system(Some(ColorScheme::Light));
    assert_eq!(use_workbench_tokens(), WorkbenchTokens::light());
}

#[test]
fn an_unknown_system_scheme_opens_on_the_light_variant() {
    system(None);
    let _scope = owner_scope();
    workbench_theme().provide();
    assert_eq!(use_workbench_tokens(), WorkbenchTokens::light());
}

#[test]
fn the_applications_theme_is_untouched_by_the_scope_and_its_switching() {
    set_theme(AppTheme {
        primary: APP_PRIMARY,
    });
    system(Some(ColorScheme::Light));
    {
        let _scope = owner_scope();
        workbench_theme().provide();
        system(Some(ColorScheme::Dark));
        assert_ne!(use_theme_tokens().primary(), APP_PRIMARY);
    }
    assert_eq!(use_theme::<AppTheme>().primary, APP_PRIMARY);
    assert_eq!(use_theme_tokens().primary(), APP_PRIMARY);
}

#[test]
fn the_root_row_declares_the_workbench_text_size() {
    for (_, theme) in variants() {
        let root = theme.root();
        assert_eq!(
            root,
            Declared::default()
                .with_font_size(13.0)
                .with_color(theme.ink)
        );
    }
}
