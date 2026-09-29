# Themes for part of a tree

`set_theme` installs the theme every `$theme` read resolves by default. A subtree can have a theme of its own:

```rsx
col theme:(AppTheme::for_act(Act::Web, $scheme)) fill:$theme.background
    text "ACTO I" color:$theme.ink_display
    box fill:$theme.accent
```

`theme:` works on any built-in tag and provides the theme to that box and everything under it, however many
children it has; the box's own `$theme` reads resolve it too. The value is a theme, or a `ScopedTheme` the
caller keeps to switch the subtree later. A value that reads `$state` is followed: when the state changes the
theme is swapped in place and only the readers under it run again. The global `set_theme` no longer reaches the
subtree.

In Rust:

| Item | What it is |
| --- | --- |
| `provide_theme(theme, \|\| build)` | Builds `build` with `theme` in force. Adds no layout node. |
| `ScopedTheme::new(theme)`, `.set(theme)` | A theme for one subtree, switchable in place. |
| `follow_theme(\|\| theme)` | A `ScopedTheme` that follows what the closure reads, which is what `theme:` with a `$` value builds. |
| `nearest_theme()` | The theme provided at or above the current owner, if any. |

Every target resolves themes the same way: they are reactive state, read where a style is built.
