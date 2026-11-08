//! Plural selection through rules installed for the process. Its own binary because the installed rules are process-wide.

use telar_i18n_core::{
    BuiltinPluralRules, Catalog, Entry, Message, Part, PluralCategory, PluralOperands, PluralRules,
    plural_category, set_locale, set_plural_rules, translate,
};

/// Rules no language has, so a selection they made cannot be mistaken for the built-in table's.
struct EvenOdd;

impl PluralRules for EvenOdd {
    fn category(&self, _locale: &str, operands: PluralOperands) -> PluralCategory {
        if !operands.is_integer() {
            PluralCategory::Many
        } else if operands.integer.is_multiple_of(2) {
            PluralCategory::Two
        } else {
            PluralCategory::One
        }
    }
}

static CATALOG: Catalog = Catalog {
    locales: &["en"],
    default_locale: "en",
    entries: &[Entry {
        key: "items",
        messages: &[(
            "en",
            Message::Plural(&[
                (
                    PluralCategory::One,
                    Message::Format(&[Part::Arg("count"), Part::Lit(" odd")]),
                ),
                (
                    PluralCategory::Two,
                    Message::Format(&[Part::Arg("count"), Part::Lit(" even")]),
                ),
                (
                    PluralCategory::Many,
                    Message::Format(&[Part::Arg("count"), Part::Lit(" fractional")]),
                ),
                (
                    PluralCategory::Other,
                    Message::Format(&[Part::Arg("count"), Part::Lit(" items")]),
                ),
            ]),
        )],
    }],
};

fn items(count: &str) -> String {
    translate(&CATALOG, "items", &[("count", count)])
}

#[test]
fn installed_rules_replace_the_builtin_table_until_replaced_again() {
    set_locale("en");
    assert_eq!(items("1"), "1 odd");
    assert_eq!(items("2"), "2 items");

    set_plural_rules(&EvenOdd);
    assert_eq!(plural_category("en", 4), PluralCategory::Two);
    assert_eq!(items("1"), "1 odd");
    assert_eq!(items("2"), "2 even");
    assert_eq!(items("2.5"), "2.5 fractional");

    set_plural_rules(&BuiltinPluralRules);
    assert_eq!(items("2"), "2 items");
    assert_eq!(items("2.5"), "2.5 items");
}
