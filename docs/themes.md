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

## Components and their slots

A component's `theme:` reaches what a call site nests inside it. The `children` placeholder builds its slot where
it stands, under the node it is placed in, so those children draw in the theme in force there, exactly as a child
written inline would:

```rsx
[logic]
let scheme = memo(use_resolved_scheme);
let palette = follow_theme(move || AppTheme::for_act(act, scheme.get()));
let ctx = Context { progress, calm };

[view]
col theme:(palette) fill:$theme.background
    col sticky
        children in:ctx
```

The same holds for every scope around the placeholder: a context an owner above provides, a locale, and the value
`in:` hands the slot, which stays readable through `use_context` when the children draw or handle an event later,
not only while they are built. Each `children` placement builds its own slot, so a placeholder inside a reactive
`if`/`for` builds its children again whenever the branch is rebuilt.

In Rust, `Children::build_slot(slot)` and `Children::build_slot_with(slot, context)` build one slot under the
current owner; `build()` and `build_with(context)` build every slot at once. A component written in Rust that wants
its children inside a scope builds them inside it.

Every target behaves the same: slots are built when the tree is, before any backend sees it.
