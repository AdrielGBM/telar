use super::*;
use telar::i18n::{Catalog, Entry, Message, plural_category, translate};
use telar::set_locale;

use PluralCategory::*;

macro_rules! branches {
    ($($category:ident),*) => {
        Message::Plural(&[$((PluralCategory::$category, Message::Plain(stringify!($category)))),*])
    };
}

static CATALOG: Catalog = Catalog {
    locales: &["ar", "en", "fr", "pl", "ru"],
    default_locale: "en",
    entries: &[Entry {
        key: "files",
        messages: &[
            ("ar", branches!(Zero, One, Two, Few, Many, Other)),
            ("en", branches!(One, Other)),
            ("fr", branches!(One, Many, Other)),
            ("pl", branches!(One, Few, Many, Other)),
            ("ru", branches!(One, Few, Many, Other)),
        ],
    }],
};

const CASES: &[(&str, &str, PluralCategory)] = &[
    ("ru", "1", One),
    ("ru", "2", Few),
    ("ru", "5", Many),
    ("ru", "11", Many),
    ("ru", "21", One),
    ("ru", "1.5", Other),
    ("pl", "1", One),
    ("pl", "2", Few),
    ("pl", "5", Many),
    ("pl", "12", Many),
    ("pl", "22", Few),
    ("ar", "0", Zero),
    ("ar", "1", One),
    ("ar", "2", Two),
    ("ar", "3", Few),
    ("ar", "11", Many),
    ("ar", "100", Other),
    ("en", "0", Other),
    ("en", "1", One),
    ("en", "1.0", Other),
    ("fr", "1.5", One),
    ("fr", "1000000", Many),
];

fn operands(count: &str) -> PluralOperands {
    PluralOperands::parse(count).unwrap()
}

#[test]
fn installed_rules_answer_through_the_seam() {
    install();
    for &(locale, count, want) in CASES {
        assert_eq!(
            plural_category(locale, operands(count)),
            want,
            "{locale} {count}"
        );
    }
}

#[test]
fn installed_rules_select_a_catalog_plural() {
    install();
    for &(locale, count, want) in CASES {
        set_locale(locale);
        assert_eq!(
            translate(&CATALOG, "files", &[("count", count)]),
            format!("{want:?}"),
            "{locale} {count}"
        );
    }
}

#[test]
fn a_region_resolves_like_its_language_however_it_is_separated() {
    for locale in ["ru-RU", "ru_RU"] {
        assert_eq!(CldrPluralRules.category(locale, 22.into()), Few, "{locale}");
    }
}

#[test]
fn a_tag_icu4x_cannot_place_picks_other() {
    assert_eq!(CldrPluralRules.category("", 1.into()), Other);
    assert_eq!(CldrPluralRules.category("not a locale", 1.into()), Other);
}
