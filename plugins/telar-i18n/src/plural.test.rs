use telar::i18n::{Catalog, Entry, Message, plural_category, translate};
use telar::{locale_tag, set_locale};

use super::*;

use PluralCategory::*;

macro_rules! branches {
    ($($category:ident),*) => {
        Message::Plural(&[$((PluralCategory::$category, Message::Plain(stringify!($category)))),*])
    };
}

static CATALOG: Catalog = Catalog {
    locales: &[
        locale_tag!("ar"),
        locale_tag!("en"),
        locale_tag!("fr"),
        locale_tag!("pl"),
        locale_tag!("ru"),
    ],
    default_locale: locale_tag!("en"),
    entries: &[Entry {
        key: "files",
        messages: &[
            (
                locale_tag!("ar"),
                branches!(Zero, One, Two, Few, Many, Other),
            ),
            (locale_tag!("en"), branches!(One, Other)),
            (locale_tag!("fr"), branches!(One, Many, Other)),
            (locale_tag!("pl"), branches!(One, Few, Many, Other)),
            (locale_tag!("ru"), branches!(One, Few, Many, Other)),
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
            plural_category(&tag(locale), operands(count)),
            want,
            "{locale} {count}"
        );
    }
}

#[test]
fn installed_rules_select_a_catalog_plural() {
    install();
    for &(locale, count, want) in CASES {
        set_locale(tag(locale));
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
        assert_eq!(
            CldrPluralRules.category(&tag(locale), 22.into()),
            Few,
            "{locale}"
        );
    }
}

#[test]
fn a_locale_icu4x_has_no_data_for_picks_other() {
    assert_eq!(CldrPluralRules.category(&tag("zz"), 1.into()), Other);
}

fn tag(text: &str) -> LocaleTag {
    LocaleTag::parse(text).unwrap()
}
