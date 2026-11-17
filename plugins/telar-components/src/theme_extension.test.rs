//! A plugin's own tokens, declared and read through the facade alone, and supplied by an application's derived theme.

use std::cell::RefCell;
use std::rc::Rc;

use telar::{
    Color, ControlSize, ScopedTheme, ThemeTokens, effect, memo, owner_scope, register_mode,
    set_mode, set_theme, use_theme_tokens,
};
use telar_components::TagTokens;

mod plugin {
    #[derive(Clone, Debug, PartialEq)]
    pub struct ChipTokens {
        pub radius: f32,
        pub gap: f32,
    }

    impl Default for ChipTokens {
        fn default() -> Self {
            Self {
                radius: 999.0,
                gap: 4.0,
            }
        }
    }

    pub fn chip_radius() -> f32 {
        telar::use_theme_extension::<ChipTokens>().radius
    }
}

use plugin::{ChipTokens, chip_radius};

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
    on_error,
    info,
    highlight_low,
    highlight_med,
    highlight_high
))]
struct AppTheme {
    primary: Color,
    #[theme(extension)]
    chips: ChipTokens,
    #[theme(extension)]
    tags: TagTokens,
}

impl AppTheme {
    fn with_tags(tags: TagTokens) -> Self {
        Self {
            tags,
            ..Self::with_radius(4.0)
        }
    }

    fn with_radius(radius: f32) -> Self {
        Self {
            primary: Color::rgba(0.0, 0.0, 1.0, 1.0),
            chips: ChipTokens { radius, gap: 2.0 },
            tags: TagTokens::default(),
        }
    }
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
    on_error,
    info,
    highlight_low,
    highlight_med,
    highlight_high
))]
struct PlainTheme {
    primary: Color,
}

const RED: Color = Color::rgba(1.0, 0.0, 0.0, 1.0);

#[test]
fn a_theme_without_extension_fields_leaves_the_plugin_on_its_defaults() {
    set_theme(PlainTheme { primary: RED });
    assert_eq!(use_theme_tokens().primary(), RED, "its tokens still apply");
    assert_eq!(
        telar::use_theme_extension::<ChipTokens>(),
        ChipTokens::default()
    );
}

#[test]
fn a_derived_theme_supplies_its_extension_field() {
    set_theme(AppTheme::with_radius(6.0));
    assert_eq!(
        telar::use_theme_extension::<ChipTokens>(),
        ChipTokens {
            radius: 6.0,
            gap: 2.0
        }
    );
}

#[test]
fn a_provided_theme_supplies_its_own_extension_to_its_subtree() {
    set_theme(AppTheme::with_radius(6.0));
    let _app = owner_scope();
    ScopedTheme::new(AppTheme::with_radius(10.0)).provide();
    assert_eq!(chip_radius(), 10.0);
    let _library = owner_scope();
    ScopedTheme::new(PlainTheme { primary: RED }).provide();
    assert_eq!(
        chip_radius(),
        999.0,
        "a theme that supplies none is not mixed with the one above it"
    );
}

#[test]
fn a_mode_switch_re_runs_a_reader() {
    let tight = register_mode("tight", || set_theme(AppTheme::with_radius(2.0)));
    let soft = register_mode("soft", || set_theme(AppTheme::with_radius(16.0)));
    set_mode(&tight);

    let seen = Rc::new(RefCell::new(Vec::new()));
    let reader = Rc::clone(&seen);
    effect(move || reader.borrow_mut().push(chip_radius()));
    set_mode(&soft);

    assert_eq!(*seen.borrow(), vec![2.0, 16.0]);
}

#[test]
fn a_token_the_theme_sets_replaces_its_fallback_and_one_it_leaves_follows_it() {
    set_theme(AppTheme::with_tags(TagTokens {
        radius: Some(9.0),
        ..TagTokens::default()
    }));
    let tokens = TagTokens::current();
    assert_eq!(tokens.radius(), 9.0);
    assert_eq!(
        tokens.border_width(),
        TagTokens::default().border_width(),
        "an unset token is what the theme's role says"
    );
}

#[test]
fn a_scaled_token_scales_the_value_the_theme_sets_and_gives_the_size_to_its_fallback() {
    set_theme(AppTheme::with_tags(TagTokens {
        pad_x: Some(8.0),
        ..TagTokens::default()
    }));
    let tokens = TagTokens::current();
    assert_eq!(tokens.pad_x(ControlSize::Regular), 8.0);
    assert_eq!(
        tokens.pad_x(ControlSize::Large),
        8.0 * ControlSize::Large.scale()
    );

    let unset = TagTokens::default();
    assert_ne!(
        unset.pad_x(ControlSize::Small),
        unset.pad_x(ControlSize::Large),
        "the fallback is asked for the size"
    );
}

#[test]
fn a_memo_of_the_tokens_follows_a_theme_switch() {
    let flat = register_mode("flat", || {
        set_theme(AppTheme::with_tags(TagTokens {
            radius: Some(1.0),
            ..TagTokens::default()
        }))
    });
    let round = register_mode("round", || {
        set_theme(AppTheme::with_tags(TagTokens {
            radius: Some(12.0),
            ..TagTokens::default()
        }))
    });
    set_mode(&flat);
    let tokens = memo(TagTokens::current);
    let radius = memo(move || tokens.get().radius());
    assert_eq!(radius.get(), 1.0);
    set_mode(&round);
    assert_eq!(radius.get(), 12.0);
}
