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

`$scheme` is the scheme the app is in, read the way `$theme` is: it is `use_resolved_scheme()` as a built-in
name, so a component needs no `let scheme = memo(use_resolved_scheme);` and reads follow a change of the system's
scheme or of `set_scheme_preference`. Its value is a `ColorScheme`, so `$scheme == ColorScheme::Dark` works in any
expression, and inside a closure it is the handle: `|| log($scheme.get())`. A `let scheme` in `[logic]` takes the
name over, and then `$scheme` is that binding, with no built-in injected; rename it to use both. The built-in is
`ResolvedScheme`, a zero-sized `Copy` handle with a `get()`, and it is the same on every target because the
scheme itself is.

In Rust:

| Item | What it is |
| --- | --- |
| `provide_theme(theme, \|\| build)` | Builds `build` with `theme` in force. Adds no layout node. |
| `ScopedTheme::new(theme)`, `.set(theme)` | A theme for one subtree, switchable in place. |
| `follow_theme(\|\| theme)` | A `ScopedTheme` that follows what the closure reads, which is what `theme:` with a `$` value builds. |
| `nearest_theme()` | The theme provided at or above the current owner, if any. |

Every target resolves themes the same way: they are reactive state, read where a style is built.

A plugin's own tokens, the fields a theme marks `#[theme(extension)]` (see [plugins](plugins.md#plugin-specific-tokens)),
belong to the theme that carries them: under a subtree's theme a plugin reads that theme's value, or its own
default when that theme supplies none, never the value of a theme further out.

## Components and their slots

A component's `theme:` reaches what a call site nests inside it. The `children` placeholder builds its slot where
it stands, under the node it is placed in, so those children draw in the theme in force there, exactly as a child
written inline would:

```rsx
[logic]
let palette = follow_theme(move || AppTheme::for_act(act, use_resolved_scheme()));
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

## Roles

`ThemeTokens` has two layers. The primitives (`radius`, `radius_sm/md/lg`, `spacing`, `spacing_sm/md/lg/xl`,
`icon_size`) are the numbers a theme moves to restyle everything. The roles say what a value is for, and each
defaults to a primitive or a convention, so a theme that names none of them still moves them through its base:

| Role | For | Default |
| --- | --- | --- |
| `radius_control` | buttons, fields, select and menu triggers, tabs, icon buttons, steppers | `radius` |
| `radius_surface` | popups and menus, tooltips, toasts, context menus, code blocks | `radius × 1.5` |
| `radius_dialog` | modals, the command palette, drawers | `radius × 3` |
| `radius_item` | rows, options and items inside a surface | `radius_sm` |
| `border_width`, `border_width_strong` | a hairline; a frame that is the control's shape | `1`, `1.5` |
| `focus_ring`, `focus_ring_width`, `focus_ring_offset` | the keyboard's ring, and a list's cursor | `primary`, `2`, `0` |
| `scrollbar_width`, `scrollbar_inset`, `scrollbar_radius` | the lane a scroll area's bar runs in (scaled by the control size); how much narrower the thumb is than its lane; the corner of its thumb | `8`, `2`, fully rounded (`(width - inset) / 2`) |
| `highlight_low/med/high` | the wash of hover, of selected or held open, of pressed | neutral washes |
| `inverse_surface`, `inverse_ink` | a tooltip's bubble and its text | `ink`, `surface` |
| `overlay` | the scrim behind a modal or a drawer | black at 50% |
| `thumb` | a toggle's knob, a slider's thumb | `surface` in light, `ink` in dark |
| `font_weight_strong` | titles, headings, matched runs | `600` |

A pill (a chip, a badge, a radio, a toggle's track) is fully rounded at whatever size it is laid out, which no
number can say, so it is not a role: a component rounds it with `metrics::pill(rect)`. The focus ring's offset
is measured inward: `0` draws the ring on the box's edge in place of its border, more keeps the border and leaves
a gap. A `[telar] library`'s `$theme` reads every role like any other token.

## Control size for a subtree

The control size (`Mini`, `Small`, `Regular`, `Large`) scales every length a catalogue control derives from the
theme. It resolves like a theme: the nearest subtree scope, else the surface's own (`SurfaceCanvas::set_control_size`),
else the thread's (`set_control_size`). In Rust:

| Item | What it is |
| --- | --- |
| `provide_control_size(size, \|\| build)` | Builds `build` at `size`. Adds no layout node. |
| `ScopedControlSize::new(size)`, `.set(size)` | A size for one subtree, switchable in place; only its readers re-run. |
| `follow_control_size(\|\| size)` | A `ScopedControlSize` that follows what the closure reads. |
| `use_control_size()`, `control_scale()` | The size in force here, and its factor, read reactively. |

So a form can be `Large` while the toolbar beside it stays `Regular`, in one window.

## Drawing like the catalogue

`telar_components::metrics` is what the catalogue draws from, public for an application's own controls:
`control_height`, `control_pad_x`, `row_height`, `panel_pad`, `popup_offset`, `gap_xs/sm/md/lg/xl`, `spacing`, `icon_size`,
`icon_pad`, `measure`, the radius roles and `pill`, `border_width(_strong)`, `focus_ring`, `hover_wash`/`held_wash`/
`pressed_wash`, `hovered_fill`/`pressed_fill`, `thumb_on`, `strong_weight`, `control_text`, `ink_on` and
`mark`. Every length is at the control size in force, and the heights and `measure` are in whole pixels. Every single-line control is
`control_height` tall and centres its line in it, so a button, a field, a select, a tab and a stepper stand level
in one row; every row of a list, a menu, a select, a tree or a context menu is `row_height` tall; and a panel that drops
from a control — a menu's, a select's — stands `popup_offset` away from it, on whichever side it opens.

The rules every catalogue component follows, and a control of an application's own should too:

| Rule | What it says |
| --- | --- |
| Widths | A box not sized by what it holds — a field, a slider, a trigger and its list, a tooltip, a dialog, a notice — defaults to `measure(ems)`: `ems` of the document's text at the control size in force, so it keeps as many characters to a line at any text size. A token width wins, at the regular size and scaled with the control size. |
| Frames | A frame that is the control's shape — a checkbox's box, a radio's ring, an outline button, the ring round a chosen swatch or a thumb — is `border_width_strong`; a selection control draws its frame in `border` until it is on and in its colour once it is. A passive hairline — a field's frame, a chip, a key cap, a panel's edge, a divider — is `border_width` in `border`. |
| Thumbs | A knob, a thumb or the ring round one is `thumb_on(fill)` over what it sits on: the theme's `thumb` wherever that stands out from the fill at 3:1, else the ink that reads on it. A toggle's knob, a slider's thumb, a handle and a colour picker's thumbs all take it. |
| Washes | A pointer resting on a control or a row lays `hover_wash` over it, the selected row or a control held open `held_wash`, a press `pressed_wash`. A filled control cannot take a wash over its own fill, so it darkens instead: `hovered_fill`, `pressed_fill`. A soft surface is its colour at a wash's strength. |
| Marks | A caret, a disclosure, a sort, a close, a tick, a magnifying glass, an external link's arrow or a window control is `mark`, strokes sized from `icon_size`, never a text glyph a face may not carry. |

## A component's own tokens

A knob worth changing on one component and not on the rest is that component's `XTokens`, supplied by the
application's theme as a `#[theme(extension)]` field. Each field is an `Option`, and `None` follows the role named
beside it, resolved when the component paints, so a theme that moves `radius_control` still moves every button
it gave no radius of its own:

```rust
#[derive(Clone, ThemeTokens)]
pub struct AppTheme {
    // …
    #[theme(extension)]
    pub buttons: ButtonTokens,
}

let theme = AppTheme { buttons: ButtonTokens { radius: Some(999.0), ..Default::default() }, /* … */ };
```

A component reads its tokens with `XTokens::current()` inside the closure that builds its style, and resolves each
through the method of the same name. Every length is at the regular control size and scaled with the control size
in force. Each field and what `None` falls back to (roles in `code`, `metrics` helpers by name):

| Tokens | Fields → fallback |
| --- | --- |
| `AccordionTokens` | `pad_x` → `control_pad_x`; `radius` → `radius_control` |
| `BadgeTokens` | `radius` → a pill; `pad_x` → `gap_md` |
| `ButtonTokens` | `radius` → `radius_control`; `pad_x` → `control_pad_x`; `border_width` → `border_width_strong` |
| `CheckboxTokens` | `size` → `icon_size`; `radius` → `radius_item`; `border_width` → `border_width_strong` |
| `ChipTokens` | `radius` → a pill; `pad_x` → `gap_md`; `border_width` → `border_width` |
| `CodeViewTokens` | `radius` → `radius_surface`; `pad` → `gap_md` |
| `ColorPickerTokens` | `width` → `measure(16)`; `radius` → `radius_control`; `ring` → `border_width_strong` |
| `CommandPaletteTokens` | `width` → `measure(40)`; `radius` → `radius_dialog`; `inset_top` → six `gap_xl` |
| `DataTableTokens` | `row_height` (the header's too) → `row_height`; `pad_x` → `gap_md`; `radius` (a row's wash and the cursor's ring) → `radius_item` |
| `DrawerTokens` | `radius` → `radius_dialog`; `pad` → `gap_xl`; `width` → `measure(20)` |
| `HandleTokens` | `size` → a slider's thumb less `gap_sm`; `ring` → `border_width_strong` |
| `IconButtonTokens` | `radius` → `radius_control` |
| `ImageTokens` | `radius` (the picture and its placeholder; an image's own `radius` wins) → `radius_item` |
| `KbdTokens` | `radius` → `radius_item`; `border_width` → `border_width` |
| `LinkTokens` | `radius` (the wash under the pointer) → `radius_item` |
| `ListTokens` | the rows of a `select`, a menu, the command palette and a `virtual_list`: `row_height` → `row_height`; `radius` → `radius_item`; `pad_x` → `gap_md` |
| `MenuTokens` | a `menu`'s and a `context_menu`'s panels alike: `background` → `surface`; `border` → `border`; `label` → the inherited ink; `faint` → `muted`; `highlight` → `highlight_low`; `radius` → `radius_surface`; `row_height` → `ListTokens::row_height`; `padding` → `panel_pad`; `width` (the least; a panel widens to its widest row) → `measure(13)`; `offset` → `popup_offset` |
| `MenuTriggerTokens` | the button a `menu` drops from: `radius` → `radius_control`; `pad_x` → `control_pad_x`; `border_width` → `border_width`; `width` → `measure(13)` |
| `ModalTokens` | `radius` → `radius_dialog`; `pad` → `gap_xl`; `min_width` → `measure(24)`; `max_width` → `measure(32)` |
| `ProgressTokens` | `radius` → a pill; `width` → `measure(16)`; `height` → `gap_md` |
| `RadioTokens` | `size` → `icon_size`; `border_width` → `border_width_strong` |
| `ResizableTokens` | a `resizable`'s and a `resize_handle`'s handles: `thickness` → `spacing`; `grip` (the corner at rest) → `border`; `active` (under the pointer or dragged) → the accent |
| `ScrubFieldTokens` | `radius` → `radius_control`; `pad_x` → `control_pad_x` |
| `SelectTokens` | `radius` → `radius_control`; `pad_x` → `control_pad_x`; `border_width` → `border_width`; `width` → `measure(13)`; `offset` → `popup_offset` |
| `SliderTokens` | `width` → `measure(16)`; `track` → `spacing`; `thumb` → `icon_size`; `ring` → `border_width_strong` |
| `SpinnerTokens` | `size` → `control_height` |
| `SplitPaneTokens` | `thickness` → `gap_sm`; `radius` → a pill |
| `StepperTokens` | `radius` → `radius_control` |
| `SwatchesTokens` | `size` → `icon_size` and `gap_sm`; `ring` → `border_width_strong` |
| `TabsTokens` | `radius` → `radius_control`; `pad_x` → `control_pad_x` |
| `TextFieldTokens` | a `text_field`'s frame, and the `textarea`'s and `search_field`'s drawn in it: `radius` → `radius_control`; `pad_x` → `control_pad_x`; `border_width` → `border_width`; `width` → `measure(21.5)` |
| `ToastTokens` | `radius` → `radius_surface`; `pad` → `gap_lg`; `width` → `measure(26)`; `stripe_width` → `gap_xs` |
| `ToggleTokens` | `knob` → `icon_size`; `inset` → `gap_xs`; `radius` → a pill. A menu row's `switch` draws the same track as tall as `icon_size`, its knob what the inset leaves |
| `TooltipTokens` | `radius` → `radius_surface`; `pad_x` → `gap_md`; `pad_y` → `gap_sm`; `max_width` → `measure(17)` |
| `TreeViewTokens` | `row_height` → `row_height`; `indent` → `gap_xl`; `radius` → `radius_item`; `pad_x` → `gap_md` |

## One instance: `style`

A single call site restyles a component's principal surface with `@class` or the `style` prop, an
`Option<SurfaceAmendment>`: a closure over the paint the component worked out, run in every state, hover and press
included, after the component's own paint. What every instance should agree on belongs in a role or in the
component's tokens instead. See [plugins](plugins.md#class-and-surfacestyle) for how a component accepts it.

`button` is the reference for all three: it reads the roles through `metrics`, its `ButtonTokens`, and `style`;
its `variant` (`Solid`, `Outline`, `Soft`, `Ghost`) says how its `color` (the theme's primary by default) is carried:

```rsx
button label:"Save"
button label:"Cancel" variant:ButtonVariant::Outline color:$theme.error
button label:"Undo" variant:ButtonVariant::Ghost
```
