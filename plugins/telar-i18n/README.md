# telar-i18n

Locale data for Telar from ICU4X: CLDR plural rules for catalogs, and number and date formatting that follows
the active locale.

Part of [Telar](https://github.com/AdrielGBM/telar), a modular Rust UI framework with its own template
language, reactive signals and a self-contained renderer.

**An application depends on this crate directly**, alongside the [`telar`](https://crates.io/crates/telar)
facade. The facade selects a catalog's plural forms with a small built-in table and formats no numbers or
dates; this crate replaces the table with CLDR's rules for every locale and adds the formatting, written
against the same public API an application's own code uses.

```toml
# Cargo.toml
telar-i18n = "0.2.1"
```

## Plural rules

`telar_i18n::install()` makes every catalog plural select by CLDR's cardinal rules from ICU4X, through
`telar::i18n::set_plural_rules`. Call it once, from the setup block of `telar::app!`:

```rust
telar::app!(
    core::theme::MyTheme,
    {
        telar_i18n::install();
    },
    telar::AppConfig::default(),
    core::app::MyRoot
);
```

Not from `main`. A hot-reload build (`cargo telar dev`) loads the application as a dylib with its own copy of the
catalog runtime, and the setup block runs again on every load of it, so the dylib's widgets select by the same
rules. An application that starts its own runner calls it before running.

The built-in table covers integer counts for the language families that differ structurally and treats
everything else like English. With this crate installed, a count of `21` is `one` in Russian, `22` is `few` in
Polish, `11` is `many` in Arabic, and a fractional count such as `1.5` (`one` in French, `other` in Russian) is
read as CLDR defines it, trailing zeros included: English says "1 file" but "1.0 files".

## Formatting

```rust
use telar_i18n::{Date, Decimal, FloatPrecision, Length, format_date, format_number};

format_number(1_234_567); // "1,234,567", "1.234.567", "12,34,567"
format_number(Decimal::try_from_f64(0.5, FloatPrecision::RoundTrip).unwrap());
format_date(Date::try_new_iso(2024, 3, 15).unwrap(), Length::Long); // "March 15, 2024", "15. März 2024"
```

Both read the active locale with `use_locale()`, so text built from them inside a widget's content closure
re-renders on a language switch. With no locale set, they use the installed catalog's default locale, as a
translated string does.

Dates are formatted in the Gregorian calendar whatever calendar the locale prefers, which keeps every other
calendar's names and conversion code out of the binary.

Relative time ("3 days ago") is not offered: ICU4X ships it only in `icu_experimental`, whose API is unstable
across minor releases.

## Binary size

Each ICU4X component is a separate dependency with only `compiled_data` enabled, and the date formatter is the
fixed-calendar one with a static field set, so the linker keeps the plural rules, number symbols and Gregorian
year-month-day patterns and drops the rest of ICU4X's data. Only what an application calls is linked: an
application that installs the plural rules and never formats a date carries no date data.

Measured on a `wasm32-unknown-unknown` release build (`opt-level = "z"`, fat LTO, stripped, before `wasm-opt`),
the plural rules add about 50 KB, number formatting about 50 KB, and date formatting about 300 KB, most of it
month names for every locale CLDR covers.

Keep it on the same version as `telar` — the same lockstep `telar` and `telar-macros` already have, and for the
same reason.

- API documentation: <https://docs.rs/telar-i18n>
- The framework, and where to start: <https://github.com/AdrielGBM/telar>

## License

Licensed under either of [Apache License, Version 2.0](../../LICENSE-APACHE) or [MIT license](../../LICENSE-MIT) at your option.

The CLDR data compiled into ICU4X is under the [Unicode License v3](https://www.unicode.org/license.txt).
