# System preferences

The user has already told their system how they want interfaces to look: light or dark, with less motion,
with more contrast, in these languages, in this order. A Telar application reads all of that as one value,
`SystemPreferences`, on every target, and follows changes live.

```rust
use telar::{ColorScheme, use_color_scheme, use_preferred_locales, use_reduced_motion};

let dark = use_color_scheme() == Some(ColorScheme::Dark);
let calm = use_reduced_motion() == Some(true);
let languages = use_preferred_locales(); // ["es-CL", "es", "en"]
```

| Field | Type | Unknown means |
| --- | --- | --- |
| `color_scheme` | `Option<ColorScheme>` | the platform has no preference to report, or cannot be asked |
| `reduced_motion` | `Option<bool>` | the platform cannot say |
| `high_contrast` | `Option<bool>` | the platform cannot say |
| `locales` | `Vec<String>` (BCP 47, most preferred first) | empty |

Unknown is never quietly turned into a default. `None` and "the user said no" are different answers, and an
application keeps its own default for the first.

## Reading them

Each field is its own signal, so a view that reads the locales does not re-render when the colour scheme
flips.

| Function | Reactive | Returns |
| --- | --- | --- |
| `use_system_preferences()` | yes, all four fields | `SystemPreferences` |
| `use_color_scheme()` | yes | `Option<ColorScheme>` |
| `use_reduced_motion()` | yes | `Option<bool>` |
| `use_high_contrast()` | yes | `Option<bool>` |
| `use_preferred_locales()` | yes | `Vec<String>` |
| `system_preferences()` | no | `SystemPreferences`, for event handlers |

The tree is built after the first snapshot arrives, so the first layout already follows it. There is no
flash of the wrong theme.

`set_system_preferences` writes the store. The runner calls it, and a test or a tool with no platform can
call it too.

## How they arrive

A platform sends `Event::SystemPreferencesChanged { preferences }` with the **whole** snapshot. It sends one
before a surface first resumes, and another whenever any field changes. The runner consumes the event, so
the widget tree never sees it. It writes the store, which everything below follows on its own, and then calls
`App::on_system_preferences(&preferences)`.

That hook is for a host that draws other applications' trees: each tree it loads from a dylib has its own
copy of the store, and only the host's is written. A `telar-plugin` host forwards the snapshot with
`LoadedPlugin::set_system_preferences` (`load_plugin` already seeds a new plugin with the host's current
snapshot). Under `cargo telar dev` the runner forwards the snapshot into the hot-reloaded library, and hands
it over again after every reload, before the reloaded state is restored so a mode picked by hand survives.

The store lives in `telar-preferences-core`, below the facade, so the crates that follow it do not need the
facade to do so.

## What follows them

| Preference | Follower | Behaviour |
| --- | --- | --- |
| Colour scheme | `follow_system(light, dark)` (theme) | Selects `light` or `dark` as the scheme changes. An unknown scheme leaves an active mode alone and selects `light` only if no mode is active yet. A manual `set_mode` wins until the scheme next changes. |
| Reduced motion | the motion ticker, by default | Every animation jumps to its end; scroll momentum keeps moving. `motion::follow_reduced_motion(false)` opts out. See [animations.md](animations.md#d5-one-time-scale-and-reduced-motion-zeroes-it). |
| Locales | `follow_system_locale(available, fallback)` | Sets the active locale to `negotiate_locale(&use_preferred_locales(), available, fallback)`, again whenever the list changes. An empty list leaves an active locale alone. |
| High contrast | nothing built in | An application reads `use_high_contrast()` and picks its own palette. |

Each follower is opt-in except motion: an application calls `follow_system` and `follow_system_locale` once at
start, in its setup. Setup runs before the first snapshot arrives, which is why these are followers rather
than one-off reads: they settle as soon as it does, and the tree is built after that.

### Negotiating a locale

`negotiate_locale(preferred, available, fallback)` picks what to show from the locales an application ships.
It is three rules, kept that small on purpose so the same answer can be computed where Rust does not run (a
page choosing its language before any wasm loads):

1. For each preferred tag, most preferred first: an available tag equal to it, ignoring ASCII case.
2. Otherwise, the first available tag, in `available`'s order, with the same primary language subtag
   (`es-CL` picks `es`, and `es` picks `es-MX`).
3. Otherwise the next preferred tag; when none match, `fallback`.

A match comes back spelled as `available` spells it. Order of preference beats quality of match:
`["en-US", "es"]` against `["es", "en-GB"]` picks `en-GB`.

```rust
use telar::{follow_system_locale, negotiate_locale, system_preferences};

follow_system_locale(["es", "en"], "es");
// or, where the locale is chosen somewhere else and only needs a default:
let locale = negotiate_locale(&system_preferences().locales, &["es", "en"], "es");
```

## Where each target reads them

| Target | Colour scheme | Reduced motion | High contrast | Locales | Changes |
| --- | --- | --- | --- | --- | --- |
| Web (both renderers) | `prefers-color-scheme` | `prefers-reduced-motion` | `prefers-contrast: more` or `forced-colors: active` | `navigator.languages` | media-query `change`, `languagechange` |
| Linux desktop | settings portal `color-scheme` | portal `reduced-motion` | portal `contrast` | `LANGUAGE`, then `LC_ALL`/`LC_MESSAGES`/`LANG` | portal `SettingChanged` |
| Windows desktop | the OS theme, via winit | `SPI_GETCLIENTAREAANIMATION` off | `SPI_GETHIGHCONTRAST` | `GetUserPreferredUILanguages` | `WM_SETTINGCHANGE` (`SPI_SETCLIENTAREAANIMATION`, `SPI_SETHIGHCONTRAST`, `intl`), theme change |
| macOS desktop | the OS appearance, via winit | `accessibilityDisplayShouldReduceMotion` | `accessibilityDisplayShouldIncreaseContrast` | `NSLocale.preferredLanguages` | `NSWorkspaceAccessibilityDisplayOptionsDidChangeNotification`, `NSCurrentLocaleDidChangeNotification`, appearance change |
| Android | `UI_MODE_NIGHT` | `animator_duration_scale` = 0 | `high_text_contrast_enabled` | `LocaleList` (API 24+), else the configuration's one locale | polled every 500 ms |
| Terminal | `COLORFGBG` | unknown | unknown | environment, as on Linux | none |
| Headless | declared | declared | declared | declared | none |

Notes on the less obvious rows:

- **Linux.** The portal keys come from the `org.freedesktop.appearance` namespace, read in one `ReadAll`.
  A portal too old to carry `contrast` or `reduced-motion` leaves that field unknown. `color-scheme` 0 means
  "no preference", so it stays unknown rather than counting as light. Without the `system-theme` feature, or
  without a session bus, only the locales are known.
- **Windows.** Windows announces a changed setting by broadcasting `WM_SETTINGCHANGE` to every top-level
  window, with `SendMessageTimeout`. winit handles that message for its own windows only to update the theme.
  winit's message hook (`with_msg_hook`) cannot see it either, because it only sees posted messages, and
  Windows hands a sent message straight to the window procedure. So a hidden top-level window on its own
  thread listens instead. A message-only window would not work, because broadcasts skip it. On
  `SPI_SETCLIENTAREAANIMATION`, `SPI_SETHIGHCONTRAST` or the `intl` area, the listener wakes the loop and the
  whole snapshot is read again there. Changing the Windows display language takes effect only at the next
  sign-in, so a running app never sees that list change.
- **macOS.** Observers on `NSWorkspace`'s notification centre (accessibility display options) and on the
  default centre (current locale) wake the loop, and the snapshot is read again there. Changes to the
  preferred-language list reach an app that is already running only as far as the locale notification and
  `NSLocale` carry them; macOS applies a new language list fully at the next launch.
- **No focus polling.** Every desktop source now has a real change signal, so nothing is re-read when a window
  regains focus.
- **Android.** Reduced motion, high contrast and the full locale list come over JNI through the activity's
  `ContentResolver` and `Resources`. `high_text_contrast_enabled` is a secure setting outside the public
  SDK, and `Settings.Secure` reads it by name. If a JNI call fails, only that field is unknown.
- **Not yet run on a device.** The Windows, macOS and Android readers compile and pass CI, but none has been
  exercised on real hardware yet; treat those rows as expected rather than confirmed behaviour.
- **Terminal.** A terminal emulator keeps motion and contrast settings to itself; no program inside it can
  see them.
- **Headless.** No user is present, so the caller declares the snapshot:

  ```rust
  HeadlessPlatform::new(800, 600).with_system_preferences(SystemPreferences {
      color_scheme: Some(ColorScheme::Dark),
      locales: vec!["es".into()],
      ..SystemPreferences::default()
  })
  ```

  Left undeclared, every field is unknown. Tests and prerendering both use this.

## For a backend author

Send `Event::SystemPreferencesChanged` before the first `on_resume` of every surface, and again with a full
snapshot whenever something changes. `platform_core::locales_from_env` and `posix_locale_to_bcp47` turn a
POSIX environment into ordered BCP 47 tags following gettext's precedence. A `C` locale disables
`LANGUAGE`, as it does in gettext.
