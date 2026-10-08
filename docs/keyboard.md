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
| slider, splitter, toolbar | the arrows |
| tree, tree item | the arrows (Up/Down walk the rows, Right/Left expand, collapse or step between parent and child), Home/End, Space, Enter. Typeahead is characters, which are never in the set |
| status, log | nothing |
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

In `.rsx`, a box becomes a control by its role: `role:button`, `role:switch`, `role:checkbox`, `role:tab`,
`role:slider` and the other roles a person operates make it a Tab stop that keeps that role's keys, wears the
theme's ring, and is pressed by Enter and Space as by a tap (see
[accessibility.md](accessibility.md#controls-and-their-state)). A region (`role:navigation`) stays out of the
Tab order. A box with `on_press` and no role still answers only the pointer: a scrim or a drag surface takes
presses without being a place the keyboard should stop.

A box declares its keys with `consumes_keys:`, in place of what its role keeps. It takes key names separated
by commas or spaces (`up`, `down`, `left`, `right`, `space`, `enter`, `tab`, `backspace`, `pageup`, `pagedown`,
`home`, `end`) or groups (`arrows`, `vertical-arrows`, `horizontal-arrows`, `activation`, `paging`, `edges`,
`scrolling`, `none`). These are the same names `ConsumedKeys::named` reads. A misspelt name is a compile
error. A `$`-reading expression that yields a `ConsumedKeys` is re-read every render:

```text
box role:slider focus_style(stroke:$theme.primary) consumes_keys:arrows on_key:(|key| …)
box focus_style(…) consumes_keys:(up down home end)
box focus_style(…) consumes_keys:(if $open { ConsumedKeys::ARROWS } else { ConsumedKeys::EMPTY })
```

The sandbox's Keyboard page (`apps/sandbox/src/features/keyboard.rsx`) shows a custom control that keeps the
arrows next to plain buttons, and its Controls with state page (`controls.rsx`) a button, a switch and toggle
buttons written as boxes with a role.

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
- Focus that lands inside the app on something that is no box posts `Event::FocusLeftBoxes` too. That is a
  link inside a paragraph, whose `<a>` the browser puts in its own Tab order, or a scroll area Firefox makes
  focusable. Without it the box Telar had focused kept its ring and kept answering keys, and Enter on the link
  also pressed that box. The host itself and the hidden entry a field types through are Telar's own places to
  park focus, and focus on them changes nothing.
- The reconciler only takes focus back when it is inside the app or has fallen to `<body>`. Focus a person
  moved elsewhere on the page stays there.

## What shows where the keyboard is

Every element that can hold focus shows it, on every target:

| Target | Indicator |
| --- | --- |
| desktop, Android, web (canvas) | Telar's ring: the focused control's `focus_style`, or the theme's primary colour 2 px wide for a `control`/`to:` box that named none. It shows when the keyboard (Tab, an arrow, the app focusing something) reached the control, not after a tap, the way `:focus-visible` does. A text entry shows it however it was reached, since a tap on a field is the start of typing. |
| web-dom | The same ring, painted as the box's inset shadow. The document's own outline is removed only from the boxes Telar rings (`data-telar-focus`), so there is one ring and not two. Anything else the browser focuses inside the app (a link inside a paragraph, a scroll area Firefox makes focusable) keeps the browser's `:focus-visible` outline. In forced-colours mode, which drops shadows, every focused element takes the browser's outline back. |
| TUI | The ring becomes a box-drawing frame in the ring's colour, in the cells around the content: a box one row tall keeps only the uprights, and a box a cell or two wide recolours its mark. A box its content fills, such as a link with no padding, has no such cells and its words are drawn over the frame, so the theme's ring also tints the background of a box that has no fill of its own; a cell keeps its background under a glyph. The plain-text reading (`TELAR_TUI_READING`, see [accessibility.md](accessibility.md)) ends the focused line with `, focused`. |
| headless | Nothing is shown. The snapshot's `AccessNode::focused` still says which node holds focus. |

A box that frames a control without being one, such as a field's border around the line it types into,
draws the ring for it with `StyledContainer::frames_focus_of(inner)`. `text_field` and the typed-entry half of
`scrub_field` do. A bare `input` in `.rsx` is the unstyled kernel primitive and shows only its caret, so the
box around it should frame it.

## Tests

`crates/renderer/renderer-dom/src/keyboard_test.rs` checks this contract in a browser: which keys are
prevented where, which boxes are Tab stops, that an unconsumed Tab never reaches the app, that a native focus
move is reported, that focus leaving for the page or for a link inside a paragraph clears the app's, and which
focused elements keep the browser's outline. `plugins/telar-components/src/audit_test.rs` checks that a
keyboard-focused control wears Telar's ring in the document (see [accessibility.md](accessibility.md#audits)).
`crates/renderer/renderer-dom/src/controls_test.rs` sends Space and Enter to a switch and a toggle button
written as boxes with a role, and checks that the page keeps neither, that the platform hands each to the
box, and that its state attribute follows.

None of these press a real key. A synthetic `KeyboardEvent` is untrusted, so the browser runs no default
action for it: no Tab walk, no scroll, no activation. Trusted input in an automated browser comes only from
the WebDriver session (`POST /session/{id}/actions`), and `wasm-bindgen-test` gives a test no way to reach the
session it runs in. The runner keeps the session id and the driver's port to itself, and geckodriver turns
away a request that carries an `Origin` header, which every `fetch` from the test page does. Covering it needs
a host-side test instead: build a small document fixture to wasm, serve it, start geckodriver, and drive the
page with WebDriver key actions (Tab, Shift+Tab, Space, Page Down, Enter) through a WebDriver client, reading
`document.activeElement`, `scrollY` and the app's own state back with `execute/sync`. That harness is not
part of the workspace yet.
