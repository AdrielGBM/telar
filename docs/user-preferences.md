# Preferences a person chose

What a person picks inside an app — a colour scheme, a language, a setting the app declares — is kept between
runs in a store each target provides. Keys are text and values are text.

```rust
use telar::{store_preference, stored_preference};

store_preference("reader.font", Some("serif"));
let font = stored_preference("reader.font");   // Some("serif") on the next run too
store_preference("reader.font", None);          // forgotten
```

| Target | The store |
| --- | --- |
| Desktop, terminal, Android | `preferences` in the app's config directory (`paths::config()`), a line per key. Written whole through a file renamed over it, so a run that dies mid-write keeps the previous one. |
| Web (DOM and canvas) | The page's `localStorage`, each key prefixed with the app's name. |
| Headless, a test, a preview | Memory: a preference holds for the run. |

A store that cannot keep something — a private window, storage turned off, a quota reached, a directory that
cannot be written — keeps it for the run and does not fail: a preference that does not survive is a lesser app,
not a broken one. An application or an embedding can install its own with `set_preference_store`.

## What Telar keeps

| Key | What |
| --- | --- |
| `telar.scheme` | The `SchemePreference` a person chose (`system`, `light`, `dark`), restored when the app starts and kept as it changes. See [docs/system-preferences.md](system-preferences.md#choosing-a-scheme). |
| `telar.locale` | The locale a person last read the app in, kept by an app whose address carries it (`follow_location_locale`). Opens an address that names no locale in it. See [docs/location.md](location.md#the-locale-in-the-location). |
| `telar.reduced_motion` | The reduced-motion override a person chose (`true`, `false`), restored when the app starts and kept as it changes; absent while the app follows the system. See [docs/system-preferences.md](system-preferences.md#choosing-reduced-motion). |

Under `cargo telar dev` the library that reloads carries the scheme and the reduced-motion override across each
reload, and keeps preferences for the session in memory.
