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
the widget tree never sees it. The runner writes the store, drives the theme's `follow_system` when the
scheme changes, and calls the application's hooks:

- `App::on_color_scheme(dark)` runs when the scheme changes. It does not run while the scheme stays unknown.
- `App::on_system_preferences(&preferences)` runs on every snapshot.

A host that draws other applications' trees uses these hooks to carry the change across its own boundary.
Under `cargo telar dev` the runner forwards the snapshot into the hot-reloaded library, and hands it over
again after every reload.

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
