# Accessibility: names, languages, hiding and state

Most of what a screen reader needs Telar derives on its own: the role of a control comes from what it
does or from `role:`, its state from the widget, and its name from the text it draws. Three things only
the application knows, and it says them with three attributes that any built-in tag takes (`text`, `col`,
`row`, `grid`, `box`, `img`, `svg`, `path`, `canvas`, `scroll`, `input`):

| Attribute | Value | Means |
| --- | --- | --- |
| `label:` | `"text"`, `t!("key")`, or an expression reading `$state` | The name assistive technology reads for the box. Re-read when what it reads changes, so a translated name follows the locale. |
| `lang:` | a BCP 47 tag: `"ja"`, `"es-CL"` | The language of the box and everything under it. The nearest one wins. |
| `a11y:hidden` | — | Assistive technology skips the box and its whole subtree. It is still drawn and still answers the pointer. |

On a component tag (`button`, `chip`, …) `label` and `lang` stay the component's own props to declare, so a
call says the same three things about the item the component returns under the names of the methods they call:
`a11y_label:`, `a11y_lang:` and `a11y:hidden`. Said there, they win over what the component said about that item
itself — a button named by its text can be named for the row it acts on:

```text
button label:"Open" variant:ButtonVariant::Ghost a11y_label:"Open {name} in the canvas"
```

A quoted name or language interpolates `{…}` as text does, wherever it is written.

A role is not one of them. What a box is decides how the keyboard reaches it and what pressing it does, so it is
the component's to say from its own root — `split_pane`'s `region` takes the landmark its pane is — and not
something a call can lay over the item it gets back.

From Rust the same three are methods of `telar::Accessible`, which every widget with a layout node has:
`.a11y_label(|| …)`, `.a11y_lang(|| …)`, `.a11y_hidden()`.

## Pictures

An `img` or `svg` without `label` is decoration and is hidden from assistive technology. With one, it is
an image with that name.

```text
svg src:icon                       // decoration
svg src:icon label:"Telar logo"    // "Telar logo, image"
```

## Split letters

A word drawn one box per letter — for a stagger, a per-letter colour — would otherwise be read as five
letters. The parent names the word and each letter is hidden:

```text
row label:"Telar" role:h2
    for letter in letters.clone()
        text "{letter}" a11y:hidden
```

A name does not replace a box's content: a reader that walks into the box still finds what is drawn
there, which is why the letters are hidden rather than left for the name to cover.

## Controls and their state

`role:` on a `box`, `col`, `row` or `grid` says what the box is. A region or a heading (`role:navigation`,
`role:h2`) only describes it. A role a person operates — `button`, `switch` (or `toggle`), `checkbox`,
`radio`, `tab`, `menuitem`, `menuitemcheckbox`, `menuitemradio`, `option`, `slider`, `spinbutton`, `link` —
also makes the box a control, as `StyledContainer::control` does from Rust: a Tab stop that wears the
theme's focus ring when the keyboard reaches it, pressed by Enter and Space the way a tap presses it (Enter
only for a link), and announced with that role. A plain `col` or `row` with such a role becomes a styled box,
since only a styled box can be one.

`toggled:` gives the control an on/off state, re-read whenever what it reads changes. One flag, and the role
says what it means, the way AccessKit models it:

| Role | `toggled:` is |
| --- | --- |
| `switch`, `checkbox`, `radio`, `menuitemcheckbox`, `menuitemradio` | checked |
| `button` | pressed: a toggle button, such as bold in a toolbar |
| `tab`, `option` | selected |

```text
box role:button label:"Add one" on_press:(|| $count += 1)
    text "+1"
box role:switch toggled:$calm label:"Reduce motion" on_press:(|| $calm.set(!$calm.get()))
    text "≈"
box role:button toggled:$bold label:"Bold" on_press:(|| $bold.set(!$bold.get()))
    text "B"
```

Two combinations are build errors: `toggled:` on a box whose role carries no on/off state (no role, a region,
a slider), and an operated role other than `link` together with `to:`, since a box with a destination is a
link whatever else it said. A box that also has `on_key:` answers its own keys, so Enter and Space go to that
handler rather than pressing it.

From Rust, the same box is `StyledContainer::control(role)` followed by `.toggled(|| …)`; the disclosure in
the catalogue's accordion uses the same flag for expanded.

| Target | Role | State |
| --- | --- | --- |
| Browser, document (`web-dom`) | A button is a `<button>`, a link an `<a>`; any other control is a `div` with the ARIA `role`. `tabindex="0"`, and the platform keeps Space and Enter from the page while the box holds focus. | `aria-checked` on a switch, checkbox or radio, `aria-pressed` on a button, `aria-selected` on a tab or an option, `aria-expanded` on a disclosure; only the one the role carries, so a box whose role changes drops the old one. |
| Desktop | The AccessKit role (`Switch`, `Button`, `CheckBox`…), with `Focus` and `Click` actions. | AccessKit `toggled` for checked and pressed (it reads the flag through the role, as Telar does), `selected` for a tab or an option, `expanded` for a disclosure. |
| Terminal | In the plain-text reading (`Reduce motion, switch`). | `checked`/`not checked`, `pressed`/`not pressed`, `selected` (an unselected tab says nothing), `expanded`/`collapsed`. |
| Android | Focus and keyboard activation as on the desktop; no accessibility bridge yet, so the role and state reach no reader. | — |
| Browser, canvas (`web`) and headless | Focus and keyboard activation as on the desktop. The snapshot carries role and state for tests; there is no reader to hand them to. | — |

A link has one state of its own: `current:` marks it as the current one of its set, the page being shown or
the place on the page the reader is at. It is `aria-current` in a document, AccessKit's `aria_current` on the
desktop and `, current` in the terminal's reading; see [docs/links.md](links.md#the-current-link).

## Lists that open from a control

A list that drops from a control is one of two things, and the catalogue says which. A `menu` and a
`context_menu` open one panel, a `menu` of `menuitem`s: actions, run when chosen, and the list takes the keyboard
as it opens. A row with a tick is a `menuitemcheckbox`, one of a set of choices (`radio`) a `menuitemradio`, each
`checked` while it is on. A row for a setting that is on or off is given a `switch` instead of a tick: the same `menuitemcheckbox` with the same state, the switch drawn at its end hidden from readers, and no tick column reserved for the other rows. Committing it leaves the menu open. A `submenu` row is a `menuitem` that has a menu and is `expanded` while it shows it, and
the submenu is a `menu` of its own; the deepest panel open holds the keyboard. The rightward arrow, Enter or Space
open a submenu with its cursor on its first row; the leftward arrow or Escape shut it and hand the keyboard back to
the panel it opened from; Escape on the first panel closes the menu and gives the keyboard back to what held it
before, the menu's button or, for a context menu, whatever had it. A context menu asked for from the keyboard —
the context-menu key or Shift+F10, `is_context_menu_key` — opens under the focused control (`focused_rect`) with
its cursor on its first row. A
`select` is a `combobox` whose list is a `listbox` of `option`s, the select-only combobox of WAI-ARIA: the
keyboard stays on the combobox, and each option is `selected` while it is the bound choice, not while the
cursor is on it. The command palette's results are a `listbox` named "Commands" that its search field
controls; there nothing is chosen until it runs, so the option the cursor is on is the selected one.

Two relations carry the rest, from Rust `StyledContainer::controls(|| …)` and `active_descendant(|| …)`
(a scroll area takes the second, a `text_field` both as props): the list a control opened, and the row its
cursor rests on while focus stays where it is. The row may sit inside the control or inside the list it
controls.

| Target | `controls` | `active_descendant` |
| --- | --- | --- |
| Browser, document (`web-dom`) | `aria-controls`, naming the list by its anchor or by an `id` Telar gives it. | `aria-activedescendant`, naming the row the same way. |
| Desktop | AccessKit `controls`. | AccessKit `active_descendant`. |
| Terminal | Not in the reading. | The reading says `, focused` on the row rather than on the control (`Large, option, selected, focused`). |

Either names nothing when the box it names is not drawn: a shut select controls no list.

A control that opens a list also says which kind, so a reader names it before it is pressed: from Rust
`StyledContainer::has_popup(HasPopup::Menu)` on a menu button or a submenu row, `HasPopup::ListBox` on a
select's combobox. It never changes; whether the list is open now is `expanded(|| …)`. It is
`aria-haspopup="menu"` or `"listbox"` in a document, AccessKit `has_popup` on the desktop, and `, has menu` or
`, has listbox` in the terminal's reading (`File, button, has menu, collapsed`).

## What a modal layer shuts out

A layer that holds the keyboard inside itself shuts the rest of its surface out for a reader as it does for Tab:
the catalogue's `modal`, `drawer` and command palette, and also a `menu`, a `context_menu`, a submenu and the list a
`select` opens. From Rust it is an `Overlay::new` or `Overlay::toggleable`, and its rule is the one `focus`
already walks Tab by: only the topmost layer that is up is reachable, and a layer opened from a field inside a
dialog shuts the dialog out too.

Menus are included on purpose. WAI-ARIA's menu button does not make the page inert, and a browser's own menus do
not, but a Telar menu traps focus (Tab stays in it) and its backdrop takes every press outside it. What the
keyboard and the pointer cannot reach while it is open, a reader is not offered either: one rule for every layer
that traps, rather than a page a reader can wander through and nothing else can.

The control a layer was opened from stays (`Overlay::owned_by`): a menu button keeps saying its menu is expanded,
and a select's combobox keeps the keyboard and points at the option its cursor is on. A tooltip, a toast and a
layer fixed over the page hold nothing and shut nothing out.

| Target | While a modal layer is up |
| --- | --- |
| Browser, document (`web-dom`) | `inert` on every box of the surface beside the layer and the control it was opened from, written as high up as it can go: the page's outermost box, or the boxes beside the path down to the control. A `dialog` inside the layer says `aria-modal="true"`. Both go when it closes. A layer in a surface drawn inside a frame shuts out only that surface, since each surface holds its own keyboard. |
| Desktop | The controls behind it were already left out of the AccessKit tree; the text and the named boxes behind it are left out too, and a dialog inside it is AccessKit `modal`. |
| Terminal | The reading is the same snapshot, so what is behind it is not in the reading. |

In a play the page behind a modal layer is not in the tree either; a tap that puts the layer away is
`click_outside` (see [previews.md](previews.md#play)).

## Where an item sits

A list or a tree that builds only the rows on screen leaves a reader counting a screenful, so each row it builds
says where it sits among them all: from Rust `StyledContainer::positioned(SetPosition { level, position, size })`,
counted from one, or `positioned_by(|| …)` for a box that is no control and whose place changes, such as the header
row of a table. A target says only the numbers the row's role takes (`Role::positioning`):

| Role | Browser, document (`web-dom`) | Desktop | Terminal reading |
| --- | --- | --- | --- |
| `treeitem`, `listitem` | `aria-level`, `aria-posinset`, `aria-setsize` | AccessKit level and position in set, from zero | `level 2, 3 of 7` |
| `option`, `menuitem`, `menuitemcheckbox`, `menuitemradio`, `radio`, `tab`, `article` | `aria-posinset`, `aria-setsize` | Position in set | `3 of 7` |
| `row` | `aria-rowindex`, and the `grid` it is in `aria-rowcount`: the row's `size`, its header row included | Row index | `1 of 501` |

Any other role says none of them. `tree_view` says it for its rows, `virtual_list` for its options and `data_table`
for its rows, its header row first.

A box that says where it sits is a node of the desktop tree and the terminal's reading even when it is no control
and has no name, such as a table's header row: its place is something only it can tell a reader, so it is read as
its role and its place (`row, 1 of 501`), with an empty name. The controls and the text inside it keep their own
names and none of their words becomes its name. Only a place its role takes earns that: an unnamed box with no
place, or with a place its role does not take (`Group`), says nothing a reader would hear and is left out, so a
list of plain `listitem` wrappers does not fill the tree with empty items. On a raster frame the place travels on
the box's element beside its role, as the snapshot reads both along the frame's nesting.

## Lists that take several rows

A `virtual_list` or a `data_table` given `selection_mode: SelectionMode::Multiple` takes any number of rows and says
so, from Rust `StyledContainer::multiselectable()` or `LayoutScrollArea::multiselectable()` on the list box, grid or
tree. Every option or row still says whether it is chosen with `toggled`, the ones left out included.

| Target | A control that takes several rows |
| --- | --- |
| Browser, document (`web-dom`) | `aria-multiselectable="true"` on the `listbox` or `grid`, beside `aria-selected` on each `option` or `row`. |
| Desktop | AccessKit `multiselectable`. |
| Terminal | `, multiselectable` after the role (`Recipients, listbox, multiselectable`). |

A press or Enter chooses a row on its own; Ctrl with a press (Command on a Mac) or Space adds the row or takes it
away; Shift with a press, an arrow, Home, End, Page Up, Page Down or Space chooses the run from the row last
pressed, toggled or walked to without Shift; Ctrl+A (Command+A) chooses every row, or none once all are.

A list or a table with no rows says so in a `status` beside the list box or the grid rather than inside it, which
holds nothing but its rows: its `empty_label`, or the children it was given, else the localized "Nothing to show".
The status is there all along, so a reader hears the message as the last row goes; the list keeps a row's height,
the table its header, so the control still has a box to hold focus in.

A `data_table` column that is `resizable` holds a splitter on its header's inline-end edge, the catalogue's
`resize_handle`, named for the column ("Resize Name column") and reading its width and its narrowest: a Tab stop
after the table's own, stepped with the arrow keys and taken to its narrowest with Home. The header is named by its
column alone, so a document does not read the splitter's name into it.

## Regions that scroll

A scroll area whose content overflows and holds nothing the keyboard can stop on (a dialog's long text, a log,
a code block wider than its frame) is a Tab stop of its own, or no key could ever move it. It is one exactly
while its content overflows along an axis it scrolls and no Tab stop inside it is reachable, and it stays one
while it holds focus, so content that grows a control or shrinks to fit never takes focus from under the
keyboard. It follows its content: a control that is built into it, shown or hidden in it, or a resize, is
reflected the next frame, without moving focus. A region that holds controls is scrolled by moving through
them and adds no stop. Rows a cursor walks while focus stays on another control are not Tab stops and do not
count. The keys it scrolls with are in [keyboard.md](keyboard.md#scroll-areas).

The page's own scroll is never one: it scrolls with nothing focused. A scroll area that is a control
(`LayoutScrollArea::control`, a tree or a list box) is a stop whether or not it overflows, and one that is
only presented (`LayoutScrollArea::presented`, the list a select opens) never is.

A reader landing on it has to hear what it scrolls. `label:` on `scroll` (`LayoutScrollArea::stop_label(|| …)`
from Rust) names it while it is a stop and leaves it nameless otherwise, so a region that fits is not announced
a second time inside the box that already names it. The catalogue's `modal` names its body after its title and
`drawer` after its label. A scroll area that is a control is named the way any box is, with `.a11y_label`; markup
has no way to make a `scroll` one, so there `label:` always means the stop's name.

| Target | A scroll area that is a stop |
| --- | --- |
| Browser, document (`web-dom`) | `tabindex="0"`, with `role="group"` and `aria-label` once it is named, and `data-telar-keys` holding only the keys for the axes it overflows. The browser scrolls the element itself, the way Telar asks it to. |
| Desktop | An AccessKit `ScrollView` with `Focus`, named by its name or, without one, by the text it draws. |
| Terminal, browser canvas (`web`), Android and headless | A stop in Telar's own Tab order, ringed when focused, and a scroll area with its name in the snapshot. |

## Shortcuts

A control that runs the same command as a key says which, so a reader can tell the key before it is used:
from Rust `StyledContainer::keyboard_shortcut(|| "Mod+Shift+Z".to_string())` on the control the key activates
(never on a tooltip, which only describes it). The text is a `Chord`, and the reader is told
`Chord::aria` of it under the keyboard convention in force: the ARIA `aria-keyshortcuts` grammar with every
key spelled as UI Events names it, so `Mod+Shift+Z` is `Control+Shift+Z` on a PC and `Shift+Meta+Z` on a
Mac, and `Ctrl++` is `Control+Plus`. It is `aria-keyshortcuts` in a document, AccessKit `keyboard_shortcut`
on the desktop, and `, shortcut Control+Z` in the terminal's reading. Text that names no chord says nothing.
The catalogue sets it on a menu row with `shortcut`, on an `icon_button` and a tab with `shortcut`, and on a
command palette row.

The visible shortcut stays in the control's name (`Undo, Ctrl+Z`; a palette row is `Open file, Ctrl+O`).
`aria-keyshortcuts` is a hint on top of the name, not a replacement for it: VoiceOver and TalkBack do not
speak it, and WAI-ARIA gives no way to ask a reader to, so dropping the drawn keys from the name would take
the only announcement away from them. A reader that does speak the attribute says the keys twice; that
redundancy is the price of not leaving the others without. A tooltip's shortcut is neither: it is drawn for
the pointer and belongs to the control, not to the bubble.

## What each target does with them

| Target | `label` | `lang` | `a11y:hidden` |
| --- | --- | --- | --- |
| Browser, document (`web-dom`) | `aria-label`. A plain `div` may not carry a name, so a named box with no role of its own gets `role="group"`; a picture is an `<svg role="img">`, where `aria-label` is its `alt`. | `lang` | `aria-hidden="true"` |
| Desktop | AccessKit node label. A named control is called that instead of by the text it draws; a named box that is not a control is a label, or an image when it draws only artwork. | AccessKit `language` | The subtree, controls included, is left out of the tree. |
| Terminal | Used in the plain-text reading (below). | Not in the reading: plain text has no voice to switch. | Left out of the reading. |
| Android | Not yet: Telar has no Android accessibility bridge. The snapshot already carries all three, and a bridge maps them to `contentDescription`, the locale of the node and `importantForAccessibility="no"`. | — | — |
| Browser, canvas (`web`) and headless | The canvas has no accessibility tree to hand them to; headless has no reader. The snapshot (`ui_core::accessibility::snapshot`) still resolves them, which is what the tests read. | — | — |

`lang:` is per subtree. The surface's own language — the root the subtrees differ from — is the active
locale's (`use_locale()`), and each target says it in its own idiom:

| Target | The surface's own language |
| --- | --- |
| Browser, document and canvas (`web-dom`, `web`) | `<html lang>`, kept in sync with the active locale at runtime; `<html dir>` follows it the same way `layout_core::Direction::for_locale` resolves the writing direction, and both start from `[telar.web]`'s packaged page (see [web-packaging.md](web-packaging.md)). |
| Desktop | The AccessKit root node's language. A node under it that carries no `lang:` of its own inherits this — the nearest one wins, root included. |
| Terminal | Nothing to set: the reading is plain text, and a reading has no voice to switch (as above). |
| Android | Not yet: same as `lang:` itself, there is no accessibility bridge to carry it. The intended target is the view's own locale (`Configuration.locale` via JNI) once one exists. |
| Headless | No reader, so nothing to carry it to. |

## The terminal's plain-text reading

A terminal screen reader reads the cells, and cells carry neither a name the application gave nor what it
hid. Set `TELAR_TUI_READING` to a file path (or `TuiPlatformConfig::reading`) and the terminal frontend
keeps that file as a reading of the screen, rewritten whenever it changes: one line per thing on screen,
in reading order, named the way a screen reader would name it (`Close, button`, `Telar logo, image`).
Following it from a second terminal gives a reader what the cells cannot. Off by default, so a frame costs
nothing when nobody reads it.

```sh
TELAR_TUI_READING=/tmp/reading.txt cargo telar dev --target tui
tail -F /tmp/reading.txt
```

The reading ends the line of whatever holds the keyboard with `, focused` (`Save, button, focused`), since a
reader following the file has no ring to look at. A link marked `current:` says so after its role (`Web, link, current`).
An item that says where it sits says it after its role, in the numbers its role takes (`src, treeitem, level 2, 3 of 7`;
see [Where an item sits](#where-an-item-sits)), and a node with no name reads as its role alone (`row, 1 of 501`).

## Names the catalogue gives

A `text_field` is named by its `label`, or by its `placeholder` when it has no label: the caption drawn above
the box is not inside the line a reader lands on, so without the name the document's entry announced only
"Text field" and the desktop read out whatever had been typed. `label:` (`a11y_label`) with an empty string is
no name at all: the box goes by what it draws, as if nothing had been said.

In the document, a field, a combobox, a slider, a spin button or a splitter that nothing named is named by the
text drawn inside it, the name every other target gives it. A browser takes that text as the value of those
roles and their name only from a label, so a `select` showing "Small" was nameless there.

## Contrast of the defaults

The tokens a theme leaves to `ThemeTokens` meet WCAG AA (4.5:1) for the text the catalogue draws with them.
`primary` and `on_primary` follow the light/dark mode, since one blue cannot carry white text and also read as
text on a dark surface; `muted` is opaque and mode-following, so a caption or a hint reads on `surface` and on
`surface_alt` over it; and a field's placeholder keeps 70% of the field's ink rather than half. A theme that
overrides a token owns its contrast, and the audit below is the way to check it.

## Audits

Two tests hold every target to the same fixture: a heading, a button, a link to an external address and one
to a route, a labelled field, a checkbox, a switch and a toggle button built the way `role:` boxes are, a
paragraph with a link run, a word named with `label:` over hidden letters, a quotation in another `lang:` and
a box under `a11y:hidden`.

| Test | What it checks |
| --- | --- |
| `plugins/telar-components/src/audit_test.rs` | The document those widgets render to, audited by [axe-core](https://github.com/dequelabs/axe-core) with its WCAG 2.2 A/AA and best-practice rules (`region` off: the fixture is a fragment of a page). Run with nothing focused, with the field focused and with the button focused, and any violation fails the test with the rule, the element and the reason. It also checks that the keyboard's box wears Telar's ring in the document, and with `overlays` that a dialog whose body scrolls makes the body a named Tab stop that Page Down scrolls. With `advanced` the page holds a list and a table with more rows than they show, which say where each built row sits, and with both an open menu leaves them and the rest of the page inert until it closes. |
| `crates/platform/platform-desktop/src/accessibility_test.rs` (`from_a_screen`) | The AccessKit tree a desktop window publishes for the same screen, built from `ui-core` primitives in place of the catalogue's button, field and checkbox: each control's role and name, that it takes `Focus` and `Click`, the URL of each link and link run, the state of the checkbox, the switch and the toggle button, that Space and Enter press the last two, that a link to the section under a bar is the current location as the page scrolls, the language and the hidden boxes, and that the tree's focus follows the Tab order. |
| `crates/renderer/renderer-dom/src/controls_test.rs` | A switch and a toggle button in the document: their role, `aria-checked` and `aria-pressed`, that each is a Tab stop, and that Space or Enter sent to the element is kept from the page, reaches the box through the platform and flips the state the next frame writes. `semantics_test.rs` checks that a box whose role changes drops the state attribute of the old one. |
| `crates/tools/cargo-telar/src/workshop_audit_test.rs` | The exported `apps/catalogue` workshop in headless Chrome: axe-core over the canvas of every preview and matrix page, with the same rules (`crates/renderer/renderer-dom/axe.json`), and over the workshop's chrome once with `region` on. See [previews.md](previews.md#auditing-the-exported-workshop). |

axe-core is MPL-2.0 and is not vendored into the repository. The flake fetches the published npm tarball by its
integrity hash and puts `axe.min.js` and its `LICENSE` in the store, and the dev shell names the script in
`TELAR_AXE_CORE`, which the test reads at compile time. Moving to another release is a change to the URL and the
hash in `flake.nix`. Run it in Firefox:

```sh
nix develop -c nix shell nixpkgs#firefox nixpkgs#geckodriver -c sh -c \
  'export GECKODRIVER=$(which geckodriver); unset CHROMEDRIVER; cargo test -p telar-components --target wasm32-unknown-unknown --test audit'
```

Where the keyboard is shown on each target is in [keyboard.md](keyboard.md#what-shows-where-the-keyboard-is).

The desktop tree is still flat and carries no structure: the snapshot is built from the controls and the text a
frame draws, and a raster frame does not say which box is a heading or a landmark, so the fixture's heading
reaches a desktop reader as a label. The document has the `<h1>`.

A grid is flat on the desktop too: its rows, header row included, are siblings of their cells under the window
rather than their parents, and the grid does not publish a row count, so a reader's table commands have no
structure to walk. The nodes are in reading order, by where they show — of two at the same corner the larger
first, so a row reads before its first cell — which puts the rows built above a scrolled view, and those scrolled
under a header held at the top, ahead of that header row. The document reads its rows in the order they are built.
