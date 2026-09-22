# Keyboard: who consumes a key

A key a control does not use still means something to whatever hosts the surface. In a web page, Tab walks
the page's focus order, and the arrows, Space, Page Up/Down, Home and End scroll it. A surface that swallowed
all of those whenever it had focus would break both. So each focusable control declares which keys it keeps,
given its role and its state, and a target that shares the keyboard with a host hands every other key back.

## The declaration

`ConsumedKeys` (in `telar-semantics-core`, re-exported as `telar::ConsumedKeys`) is a set of the keys some
host acts on by default: `TAB`, `SPACE`, `ENTER`, `BACKSPACE`, the four arrows, `PAGE_UP`/`PAGE_DOWN`, `HOME`
and `END`, with groups such as `ARROWS`, `ACTIVATION` (Space and Enter) and `SCROLLING`. Characters are never
in it: no host scrolls or moves focus with one.

Each role has a default, `Role::consumed_keys()`:

| Role | Keeps |
| --- | --- |
| button, link, checkbox, radio, switch, tab, disclosure | Space, Enter |
| menu item, combobox | Space, Enter, Up/Down, Home/End |
| slider | the arrows |
| spin button | the arrows, Enter |
| text field | the arrows, Home/End, Space, Enter, Backspace |
| multi-line editor | a text field's keys, and Tab (it types one) |
| scroll area | everything that scrolls |
| everything else | nothing |

State adjusts it (`ui_core::focus::focusable_of`):

- A control that is disabled, hidden or outside an open modal keeps nothing and is not a Tab stop.
- A control inside a modal that holds focus also keeps Tab, because stepping inside the trap is Telar's.
- A widget whose keys depend on its own state declares them with `StyledContainer::consumes_keys(|| ...)`,
  re-read every render. The dropdown keeps only Down, Space and Enter while closed, and walks rows with
  Up/Down, Home and End while open.

A control that answers a key it did not declare fights the host for that key on web-dom. That is the rule to
remember when writing a custom control with `on_focused_key`.

In `.rsx`, a box declares its keys with `consumes_keys:`. It takes key names separated by commas or spaces
(`up`, `down`, `left`, `right`, `space`, `enter`, `tab`, `backspace`, `pageup`, `pagedown`, `home`, `end`)
or groups (`arrows`, `vertical-arrows`, `horizontal-arrows`, `activation`, `paging`, `edges`, `scrolling`,
`none`). These are the same names `ConsumedKeys::named` reads. A misspelt name is a compile error. A
`$`-reading expression that yields a `ConsumedKeys` is re-read every render:

```text
box role:slider focus_style(stroke:$theme.primary) consumes_keys:arrows on_key:(|key| …)
box focus_style(…) consumes_keys:(up down home end)
box focus_style(…) consumes_keys:(if $open { ConsumedKeys::ARROWS } else { ConsumedKeys::EMPTY })
```

The sandbox's Keyboard page (`apps/sandbox/src/features/keyboard.rsx`) shows a custom control that keeps the
arrows next to plain buttons.

The result reaches the renderer as `Semantics::focusable`: whether Tab stops on the box right now, and the
set it keeps. A box that cannot hold focus has `None`.

## Per target

| Target | Behaviour |
| --- | --- |
| web-dom | The browser owns Tab and scrolling. The reconciler writes each focusable box's set to `data-telar-keys`, its identity to `data-telar-focus`, and `tabindex="0"` for Tab stops (`-1` for other focusables). The platform's `keydown` listener reads the set from the element the key was sent to and calls `preventDefault` only for a key that element keeps. Otherwise the browser's default runs. |
| web (canvas) | Unchanged. Nothing on a canvas is a native control, so the platform keeps Tab, Space, the arrows, Page Up/Down, Home, End and Backspace whenever the surface has focus (`WebPlatformConfig::owns_keyboard`). Telar walks its own Tab order. |
| desktop, Android, TUI, headless | Unchanged. No native default action competes for the keys, so every key reaches the app and Telar walks its own Tab order. The declaration is still built, so a future backend with native focus can read it. |

## Focus on web-dom

The browser walks Tab natively through the `tabindex="0"` boxes, in document order, which is the order Telar
registers them in. It leaves the app past the last one. Telar's focus follows the document:

- The reconciler listens for `focusin` on the host and posts `Event::BoxFocused { box_id }` for a box with
  `data-telar-focus`. The runner answers with `focus::follow_box`, which focuses that box as keyboard focus,
  so the ring shows. A box that already holds focus is left alone, so the echo of a tap keeps its ring off.
- A Tab the focused box does not keep is not delivered to the app as a key. The browser already moved focus,
  and the app hears where it landed through `BoxFocused` instead of stepping a second time. Every other key is
  still delivered, so app-level shortcuts keep working while the page scrolls.
- A text field is typed into through a hidden editable element that sits at the end of the host and carries
  the field's keys. On a Tab the field does not keep, that element hands focus to the field's own box before
  the browser's default runs, so the walk starts where the caret is.
- Focus that leaves the app for content beside it, or for the browser's own interface, posts
  `Event::FocusLeftBoxes`, and the runner clears Telar's focus. No box stays focused and no ring stays drawn.
  When Tab or Shift+Tab brings focus back, `BoxFocused` picks up the box it lands on. A window that only
  loses focus is different: the focused element stays focused, and nothing is cleared.
- The reconciler only takes focus back when it is inside the app or has fallen to `<body>`. Focus a person
  moved elsewhere on the page stays there.

`crates/renderer/renderer-dom/src/keyboard_test.rs` checks this contract in a browser: which keys are
prevented where, which boxes are Tab stops, that an unconsumed Tab never reaches the app, and that a native
focus move is reported, and that focus leaving for the page clears the app's. A synthetic key event is untrusted, so it cannot trigger the browser's own Tab walk or
scroll. Checking those needs trusted input, for example WebDriver actions.
