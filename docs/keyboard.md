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
| scroll area | everything that scrolls; a plain one only the keys for the axes it overflows (see [Scroll areas](#scroll-areas)) |
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

## Scroll areas

A scroll area that is not a control is a Tab stop while its content overflows and no Tab stop inside it is
reachable, and for as long as it holds focus (see [accessibility.md](accessibility.md#regions-that-scroll)).
Focused, it scrolls the way a page does, by a line for an arrow and by the view less a line for a page:

| Key | Scrolls |
| --- | --- |
| Up, Down | a line, while the content is taller than the view |
| Left, Right | a line, while the content is wider than the view. The arrows move the view the way they point, in a right-to-left layout too, as a browser scrolls |
| Page Up, Page Down | a page up or down |
| Space, Shift+Space | a page down or up |
| Home, End | to the top or the bottom |

It keeps only the keys for the axes its content overflows, so a block that only scrolls sideways leaves Up,
Down, the paging keys and Space to the page around it, and a key held with Ctrl, Alt or Command is left to the
app's shortcuts. A press inside it focuses it as a tap does, without a ring, which is also what a browser does
with the element, and Tab or the app focusing it shows the ring. A control scroll area (a tree, a list box)
keeps its role's keys, answers them with `on_focused_key` first, and scrolls with the keys it leaves.

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

## Shortcuts

A shortcut is written once, portably, and shown and matched the way the platform it runs on does. `Chord`
(in `telar-platform-core`, re-exported as `telar::Chord`) reads it from text: `"Mod+Z"`, `"Mod+Shift+Z"`,
`"Alt+↑"`, `"F5"`, `"Ctrl++"` (a key that is itself `+` comes last). `Mod` is the primary modifier: Command
on a Mac, Ctrl elsewhere. `Ctrl`, `Cmd` (or `Meta`, `Super`), `Alt` (or `Option`) and `Shift` name one key
everywhere. A Mac's own symbols read back too, so `"⇧⌘Z"` is Shift+Command+Z.

| Written | On a Mac | Elsewhere |
| --- | --- | --- |
| `Mod+Z` | ⌘Z | Ctrl+Z |
| `Mod+Shift+Z` | ⇧⌘Z | Ctrl+Shift+Z |
| `Alt+S` | ⌥S | Alt+S |
| `Ctrl+Shift+D` | ⌃⇧D | Ctrl+Shift+D |
| `Mod+Enter` | ⌘⏎ | Ctrl+Enter |

`Chord`'s `Display` writes it the platform's way: a Mac's symbols in its order (⌃⌥⇧⌘) with nothing between
them, and elsewhere words joined with `+` in the order Ctrl, Alt, Shift, Meta. `keys(convention)` gives the
same one key at a time, which is what `kbd` puts on its caps.

Matching follows the same rule. `Chord::matches(key, modifiers)` asks for exactly the chord's Ctrl, Alt and
Command; a letter matches in either case with exactly the chord's Shift, and any other character matches
whatever Shift it took to type it, so `?` needs no `Shift+`. `telar::chord_pressed(&chord, key)` matches the
press a key handler was given against the modifiers held now, and with Alt held it reads the key with no
modifier applied, since Option on a Mac types `†` for T. `ModifiersState::is_primary()` says whether the
primary modifier is held, for a handler that asks without a chord.

Which convention is in force is the platform's answer to `Platform::keyboard_convention`, installed before
the app is built and read back with `telar::keyboard_convention()`:

| Target | Convention |
| --- | --- |
| desktop | the operating system it was built for: Mac on macOS, PC elsewhere |
| web | the system the browser says it runs on (`navigator.userAgentData.platform`, else `navigator.platform`), so a page shows ⌘ on a Mac and Ctrl+ on Windows. PC when the browser says neither |
| TUI | PC: a terminal keeps Command for itself, so what reaches the app is Ctrl |
| headless, snapshots | PC, so a frame does not depend on the machine that drew it |

A hot-reloaded build and an embedded guest each carry their own copy of the convention, so the host hands
over its own when it loads one, before the guest's app is built: a guest shows a shortcut the way its host
matches it.

The components that show a shortcut take it as that text and show it the platform's way: `item`'s
`shortcut`, `kbd`'s `chord`, `tooltip`'s and `icon_button`'s `shortcut`, a tab's `.shortcut(…)` and a
command palette entry's `with_shortcut(…)`. A reader hears the same text the row shows. Text that names no
chord is shown as written.

```text
menu label:"Edit"
    item label:"Undo" shortcut:"Mod+Z"
    item label:"Redo" shortcut:"Mod+Shift+Z"
```

`item`'s `shortcut` only shows the keys; the application binds them itself, with `chord_pressed` in its key
handler. A test pins what a shortcut shows with `telar::set_keyboard_convention`, which holds for the thread
that calls it.

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
