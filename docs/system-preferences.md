# System preferences

The user has already told their system how they want interfaces to look: light or dark, with less motion,
with more contrast, with larger text, in these languages, in this order. A Telar application reads all of that
as one value, `SystemPreferences`, on every target, and follows changes live.

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
| `text_scale` | `TextScale` (a finite factor above zero) | never unknown: a platform with no such setting, or one it cannot read, says `TextScale::UNSCALED` (1.0) |

Unknown is never quietly turned into a default. `None` and "the user said no" are different answers, and an
application keeps its own default for the first. The text scale is the one field with no such gap: a scale nobody
set and one nobody can read both mean text at the size it was designed at.

## Reading them

Each field is its own signal, so a view that reads the locales does not re-render when the colour scheme
flips.

| Function | Reactive | Returns |
| --- | --- | --- |
| `use_system_preferences()` | yes, all five fields | `SystemPreferences`, as the system reported it |
| `use_color_scheme()` | yes | `Option<ColorScheme>` |
| `use_reduced_motion()` | yes | `Option<bool>`, the app's override while it sets one (see [Choosing reduced motion](#choosing-reduced-motion)) |
| `use_system_reduced_motion()` | yes | `Option<bool>`, the system's alone |
| `use_high_contrast()` | yes | `Option<bool>`, the surface's or the app's choice while one is set (see [Choosing high contrast](#choosing-high-contrast)) |
| `use_system_high_contrast()` | yes | `Option<bool>`, the system's alone |
| `use_preferred_locales()` | yes | `Vec<String>` |
| `use_text_scale()` | yes | `TextScale` |
| `system_preferences()` | no | `SystemPreferences` as the system reported it, for event handlers |

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
copy of the store, and only the host's is written. A `telar-embed` host forwards the snapshot with
`LoadedEmbed::set_system_preferences` (`load_embedded` already seeds a new guest with the host's current
snapshot). Under `cargo telar dev` the runner forwards the snapshot into the hot-reloaded library, and hands
it over again after every reload, before the reloaded state is restored so a mode picked by hand survives.

The store lives in `telar-preferences-core`, below the facade, so the crates that follow it do not need the
facade to do so.

## What follows them

| Preference | Follower | Behaviour |
| --- | --- | --- |
| Colour scheme | `follow_system(light, dark)` (theme) | Selects `light` or `dark` as the resolved scheme changes: the system's, or the person's choice (see [Choosing a scheme](#choosing-a-scheme)). Under `System`, an unknown scheme leaves an active mode alone and selects `light` only if no mode is active yet. A manual `set_mode` of another mode wins until the next change. |
| Reduced motion | the motion ticker, by default | Follows `use_reduced_motion()`: the system's preference, or the app's override (see [Choosing reduced motion](#choosing-reduced-motion)). Every animation jumps to its end; scroll momentum keeps moving. `motion::follow_reduced_motion(false)` opts out. See [animations.md](animations.md#d5-one-time-scale-and-reduced-motion-zeroes-it). |
| Locales | `follow_system_locale(available, fallback)` | Sets the active locale to `negotiate_locale(&use_preferred_locales(), available, fallback)`, again whenever the list changes. An empty list leaves an active locale alone. |
| Locales, once | `follow_location_locale(available, base)` | For an app whose address carries its locale: the system's list is negotiated only when the address the app opens at names none and the person never chose one, and is not followed after. See [docs/location.md](location.md#the-locale-in-the-location). |
| High contrast | the theme's tokens, always | Wherever `use_high_contrast()` is `Some(true)` (the system's preference, the app's override, or the surface's own; see [Choosing high contrast](#choosing-high-contrast)), `use_theme_tokens()` answers the high-contrast set derived from the theme, so every catalogue component follows. An application that reads its own theme type's fields reads them as written; see [themes](themes.md#high-contrast). |
| Text scale | the text cascade, always | Every size the cascade resolves is multiplied by it: the document's own, one a box declares, one a text's call authors, one a span names in pixels. A size in `em` follows the size it is of, and a fraction of the surface stays the surface's. A widget that sizes a box from the text around it (`measure`, a control's caption) grows with it. A style built whole in Rust (`TextStyle::new(13.0, …)`) is drawn at the size it names. |

Each follower is opt-in except motion, contrast and the text scale: an application calls `follow_system` and
`follow_system_locale` once at start, in its setup. Setup runs before the first snapshot arrives, which is why
these are followers rather than one-off reads: they settle as soon as it does, and the tree is built after that.

### Choosing a scheme

A person can fix the scheme whatever the system says: `set_scheme_preference(SchemePreference::Dark)`. The
choice holds until the app changes it again, not until the system next changes; `SchemePreference::System`
hands it back. `use_resolved_scheme()` is the scheme the app is in (the preference resolved against
`use_color_scheme()`, a system that reports none taken as light), `use_scheme_preference()` the choice itself,
and `is_dark()` and `use_mode()` read the active mode reactively. The choice is app state: it is the same on
every target, survives a hot reload, and persists between sessions where the target keeps preferences.
In `.rsx`, `$scheme` is `use_resolved_scheme()` as a built-in name (see [themes](themes.md)); a `let scheme` in
`[logic]` takes the name over.

### Choosing reduced motion

A person can ask the app itself for less motion, or for all of it, whatever the system says:
`set_reduced_motion_override(Some(true))` or `Some(false)`. The choice holds until the app changes it again,
through every change the system reports meanwhile; `None` hands it back. `use_reduced_motion()` is then the
value in effect, and the motion ticker follows that same value, so an override reduces every animation the
system's preference would. `use_system_reduced_motion()` and `use_system_preferences()` still answer what the
system said, and `use_reduced_motion_override()` the choice itself.

```rust
use telar::{set_reduced_motion_override, use_reduced_motion, use_system_reduced_motion};

set_reduced_motion_override(Some(true));         // less motion here, whatever the system says
let calm = use_reduced_motion() == Some(true);   // true
let system = use_system_reduced_motion();        // unchanged
set_reduced_motion_override(None);               // follow the system again
```

In `.rsx`, a control that offers the choice calls it from a handler:

```rsx
button label:"Less motion" on_press:(|| telar::set_reduced_motion_override(Some(true)))
button label:"Follow the system" variant:ghost on_press:(|| telar::set_reduced_motion_override(None))
```

The override is meant for that: a choice the person makes in the app. An application that merely prefers its own
animations should not set it, nor turn `follow_reduced_motion` off.

Like the scheme, it is app state, so it behaves the same on every target. It persists between sessions in the
target's preference store under `telar.reduced_motion` (`localStorage` on the web, a file in the config
directory on desktop, a terminal and Android, memory in a headless run), and survives a hot reload. It belongs
to the runtime that set it: the snapshot a `telar-embed` host forwards is what the system reported, so a
guest keeps following the system, or its own override, rather than its host's.

### Choosing high contrast

High contrast has the same two layers as reduced motion, and one more. `set_high_contrast_override(Some(true))`
or `Some(false)` fixes it for the whole app whatever the system says, and `None` hands it back. A surface can
then make a choice of its own with `SurfaceCanvas::set_high_contrast`, which wins over both on that surface
alone: that is how a workshop canvas shows a preview in high contrast beside the regular chrome around it.
`use_high_contrast()` answers the value in force on the surface it is read on; `use_system_high_contrast()` and
`use_system_preferences()` still answer what the system said.

```rust
use telar::{set_high_contrast_override, use_high_contrast, use_system_high_contrast};

set_high_contrast_override(Some(true));         // more contrast here, whatever the system says
let strong = use_high_contrast() == Some(true); // true, unless this surface chose otherwise
let system = use_system_high_contrast();        // unchanged
set_high_contrast_override(None);               // follow the system again
```

Unlike the reduced-motion override, it is not stored between sessions; it does survive a hot reload.

What the value changes is the theme's tokens: where it is `Some(true)`, `use_theme_tokens()` answers the
high-contrast set derived from the theme in force, and every catalogue component draws with it (see
[themes](themes.md#high-contrast)). An application reading `use_high_contrast()` itself is for what the tokens
do not cover, such as a palette of its own type.

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
use telar::{follow_location_locale, follow_system_locale, negotiate_locale, system_preferences};

follow_system_locale(["es", "en"], "es");
// or, where the locale is part of the address (`/es/`, `/en/`) and the system only picks the first one:
follow_location_locale(["es", "en"], "es");
// or, where the locale is chosen somewhere else and only needs a default:
let locale = negotiate_locale(&system_preferences().locales, &["es", "en"], "es");
```

## Where each target reads them

| Target | Colour scheme | Reduced motion | High contrast | Locales | Text scale | Changes |
| --- | --- | --- | --- | --- | --- | --- |
| Web (both renderers) | `prefers-color-scheme` | `prefers-reduced-motion` | `prefers-contrast: more` or `forced-colors: active` | `navigator.languages` | the root element's font size over 16 px | media-query `change`, `languagechange` |
| Linux desktop | settings portal `color-scheme` | portal `reduced-motion` | portal `contrast` | `LANGUAGE`, then `LC_ALL`/`LC_MESSAGES`/`LANG` | portal `org.gnome.desktop.interface` `text-scaling-factor`, where the desktop publishes it | portal `SettingChanged` |
| Windows desktop | the OS theme, via winit | `SPI_GETCLIENTAREAANIMATION` off | `SPI_GETHIGHCONTRAST` | `GetUserPreferredUILanguages` | `TextScaleFactor` under `HKCU\Software\Microsoft\Accessibility` | `WM_SETTINGCHANGE` (`SPI_SETCLIENTAREAANIMATION`, `SPI_SETHIGHCONTRAST`, `intl`), theme change |
| macOS desktop | the OS appearance, via winit | `accessibilityDisplayShouldReduceMotion` | `accessibilityDisplayShouldIncreaseContrast` | `NSLocale.preferredLanguages` | unscaled: macOS has no system text size | `NSWorkspaceAccessibilityDisplayOptionsDidChangeNotification`, `NSCurrentLocaleDidChangeNotification`, appearance change |
| Android | `UI_MODE_NIGHT` | `animator_duration_scale` = 0 | `high_text_contrast_enabled` | `LocaleList` (API 24+), else the configuration's one locale | `font_scale` | polled every 500 ms |
| Terminal | `COLORFGBG` | unknown | unknown | environment, as on Linux | unscaled: a cell is the terminal's size | none |
| Headless | declared | declared | declared | declared | declared | none |

Notes on the less obvious rows:

- **Linux.** The portal keys come from the `org.freedesktop.appearance` namespace, read in one `ReadAll`.
  A portal too old to carry `contrast` or `reduced-motion` leaves that field unknown. `color-scheme` 0 means
  "no preference", so it stays unknown rather than counting as light. Without the `system-theme` feature, or
  without a session bus, only the locales are known. The text scale is GNOME's `text-scaling-factor`, which its
  portal publishes under `org.gnome.desktop.interface` in the same `ReadAll` and `SettingChanged`; a desktop
  whose portal does not publish it leaves text unscaled.
- **Web.** The text scale is the reader's text size setting, which a browser applies as the root element's font
  size, over the 16 px every browser starts from. The document renderer writes every font size in `rem` of that
  root, so the size the layout scaled comes back to the same `rem`, and a page written ahead of time at the default
  root (see [prerender](prerender.md)) is shown at the reader's size before the app has run. A browser announces
  no change of its text size alone; the scale is read again with the next snapshot.
- **Windows.** Windows announces a changed setting by broadcasting `WM_SETTINGCHANGE` to every top-level
  window, with `SendMessageTimeout`. winit handles that message for its own windows only to update the theme.
  winit's message hook (`with_msg_hook`) cannot see it either, because it only sees posted messages, and
  Windows hands a sent message straight to the window procedure. So a hidden top-level window on its own
  thread listens instead. A message-only window would not work, because broadcasts skip it. On
  `SPI_SETCLIENTAREAANIMATION`, `SPI_SETHIGHCONTRAST` or the `intl` area, the listener wakes the loop and the
  whole snapshot is read again there. Changing the Windows display language takes effect only at the next
  sign-in, so a running app never sees that list change. "Make text bigger" is kept in the registry as a
  percentage and is read with every snapshot; the listener does not wake the loop for it alone.
- **macOS.** Observers on `NSWorkspace`'s notification centre (accessibility display options) and on the
  default centre (current locale) wake the loop, and the snapshot is read again there. Changes to the
  preferred-language list reach an app that is already running only as far as the locale notification and
  `NSLocale` carry them; macOS applies a new language list fully at the next launch.
- **No focus polling.** Every desktop source now has a real change signal, so nothing is re-read when a window
  regains focus.
- **Android.** Reduced motion, high contrast, the font scale and the full locale list come over JNI through the
  activity's `ContentResolver` and `Resources`. `high_text_contrast_enabled` is a secure setting outside the public
  SDK, and `Settings.Secure` reads it by name. If a JNI call fails, only that field is unknown,
  or unscaled for the font scale.
- **Not yet run on a device.** The Windows, macOS and Android readers compile and pass CI, but none has been
  exercised on real hardware yet; treat those rows as expected rather than confirmed behaviour.
- **Terminal.** A terminal emulator keeps motion and contrast settings to itself; no program inside it can
  see them. A cell is the size the terminal draws it, so text stays unscaled.
- **Headless.** No user is present, so the caller declares the snapshot:

  ```rust
  HeadlessPlatform::new(800, 600).with_system_preferences(SystemPreferences {
      color_scheme: Some(ColorScheme::Dark),
      locales: vec!["es".into()],
      ..SystemPreferences::default()
  })
  ```

  Left undeclared, every field is unknown and text is unscaled. Tests and prerendering both use this.

## For a backend author

Send `Event::SystemPreferencesChanged` before the first `on_resume` of every surface, and again with a full
snapshot whenever something changes. `platform_core::locales_from_env` and `posix_locale_to_bcp47` turn a
POSIX environment into ordered BCP 47 tags following gettext's precedence. A `C` locale disables
`LANGUAGE`, as it does in gettext. A backend with no text size setting to read says `TextScale::UNSCALED` in its
snapshot, by name, rather than leaving it to `..SystemPreferences::default()`.
