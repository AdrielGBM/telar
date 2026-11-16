# Accessibility: names, languages, hiding and state

Most of what a screen reader needs Telar derives on its own: the role of a control comes from what it
does or from `role:`, its state from the widget, and its name from the text it draws. Three things only
the application knows, and it says them with three attributes that any built-in tag takes (`text`, `col`,
`row`, `grid`, `box`, `img`, `svg`, `path`, `canvas`, `scroll`, `input`):

| Attribute | Value | Means |
| --- | --- | --- |
| `a11y_label:` | `"text"`, `t!("key")`, or an expression reading `$state` | The name assistive technology reads for the box. Re-read when what it reads changes, so a translated name follows the locale. |
| `lang:` | a BCP 47 tag, `"ja"`, `"es-CL"`, checked when the `.rsx` is transpiled; or an expression that is a `LocaleTag` | The language of the box and everything under it. The nearest one wins. |
| `a11y:hidden` | — | Assistive technology skips the box and its whole subtree. It is still drawn and still answers the pointer. |

`label` is always text that is seen, never the name a reader hears, so `label:` on a built-in tag is a build
error that points at `a11y_label:`. On a component tag (`button`, `tag`, …) `label` stays the component's own
prop, its visible text, and a call says the same three things about the item the component returns under the
same keys: `a11y_label:`, `lang:` and `a11y:hidden`. Said there, they win over what the component said about
that item itself — a button named by its text can be named for the row it acts on:

```text
button label:"Open" variant:ghost a11y_label:"Open {name} in the canvas"
```

A quoted name or language interpolates `{…}` as text does, wherever it is written.

The language decides how the text under it is shaped, broken and cased, not only what a reader is told, so it
is `lang:` on every tag, `.lang(|| locale_tag!("es"))` from Rust, and never a component's own prop.

A call may also say what that item is with `role:`, but only in words that describe the item from outside and
ask nothing of what it holds:

| `role:` on a component call | Why a call may say it |
| --- | --- |
| `banner` (`header`), `navigation` (`nav`), `main` (`content`), `complementary` (`aside`, `sidebar`), `contentinfo` (`footer`), `search`, `form`, `section` | A landmark: where the item sits on the screen, which the call site knows and the component cannot. `form` and `section` are landmarks only once named (`a11y_label:`); said by a call, each is still a node of its own natively. |
| `article` | A self-contained piece, whatever is inside it. |
| `h1` … `h6` | A heading, at the depth of the outline the call sits in — which the component cannot know. Named by the words drawn in it unless `a11y_label:` names it. |
| `status`, `log` | A live region: what changes in it is announced without taking focus. |
| `alert` | A live region that interrupts: what changes in it is announced at once, over what a reader is saying, without taking focus. |
| `listitem` (`item`) | One item of the list the call sits in. The list is the call site's own box, so the call is where the pair is kept true. |
| `group` | Says nothing: over a region the component made, it takes the landmark away on every target, and it adds no node of its own on any. |

```text
heading text:"Recent changes" role:h3
card role:article a11y_label:"Release notes"
```

Every other role is a build error on the call's line. A role a person operates (`button`, `switch`, `tab`,
`option`, `slider`, …) decides how the keyboard reaches the box and what pressing it does, which only the component
that wires the press can keep true. A `list`, `grid`, `tree`, `menu`, `listbox`, `tablist`, `radiogroup` or `toolbar` promises what
the boxes inside it are, and a `dialog`, `scrollarea` or `drawing` changes how the box is reached or drawn; the
component's insides are not the call's to see. Those stay the component's to say from its own root —
`split_pane`'s `region` takes the landmark its pane is.

The role is laid only over a root the component left a plain box or a region (`Role::annotated`). A root the
component made a control, a heading, a list or anything else keeps its own role on every target, and one the
keyboard reaches keeps it too, so `role:navigation` on a `button` call cannot announce a navigation that answers
Enter. The build cannot see which root a component returns, so this is the runtime's answer rather than an error:
the same rule in the document's element, the snapshot, AccessKit and the terminal's reading.

A call keeps these keys for itself — `a11y_label`, `lang`, `a11y`, `role`, `slot`, and the state keys
`hover_style`, `active_style`, `disabled_style` and `focus_style` — so none of them ever reaches a prop, and
`#[derive(Props)]` refuses a field named after one, saying what the key does on a call.

From Rust the same are methods of `telar::Accessible`, which every widget with a layout node has:
`.a11y_label(|| …)`, `.lang(|| …)`, `.a11y_hidden()` and `.a11y_role(Role::…)`, which in a debug build panics
on a role a call could not say (`Role::is_annotatable`).

## Pictures

An `img` or `svg` without `a11y_label` is decoration and is hidden from assistive technology. With one, it is
an image with that name.

```text
svg src:icon                            // decoration
svg src:icon a11y_label:"Telar logo"    // "Telar logo, image"
```

`telar-icons`' `icon` is the same: decoration until its call names it with `a11y_label:`, which makes it a
picture by that name. It has no `label` of its own, since `label` is always text that is seen.

```text
icon name:"lucide:x"                         // decoration
icon name:"lucide:x" a11y_label:"Remove"     // "Remove, image"
```

From Rust, a component that draws artwork only to decorate unless its caller names it marks its root
`.a11y_decorative()`: hidden from assistive technology while it goes by no name, a picture once
`.a11y_label(…)` gives it one.

## Split letters

A word drawn one box per letter — for a stagger, a per-letter colour — would otherwise be read as five
letters. The parent names the word and each letter is hidden:

```text
row a11y_label:"Telar" role:h2
    for letter in letters.clone()
        text "{letter}" a11y:hidden
```

On a box with a role of its own, such as this heading, a name does not replace the box's content: a reader
that walks into the box still finds what is drawn there, which is why the letters are hidden rather than left
for the name to cover.

On a box with no role of its own, the name does replace the words it draws: a counter
`tag label:"3" a11y_label:"3 unread comments"` is read as "3 unread comments" once, and the "3" is not read
again beside it. A control inside such a box keeps its own name and stays reachable; only the text no control
owns is left out, and a paragraph holding a link is kept to be read around the link.

## Controls and their state

`role:` on a `box`, `col`, `row` or `grid` says what the box is. A region or a heading (`role:navigation`,
`role:h2`) only describes it. A role a person operates — `button`, `switch` (or `toggle`), `checkbox`,
`radio`, `tab`, `menuitem`, `menuitemcheckbox`, `menuitemradio`, `option`, `slider`, `spinbutton`, `link` —
also makes the box a control, as `StyledContainer::control` does from Rust: a Tab stop that wears the
theme's focus ring when the keyboard reaches it, pressed by the Enter and Space its role keeps the way a tap
presses it (Enter only for a link, neither for a slider), and announced with that role. A plain `col` or `row` with such a role becomes a styled box,
since only a styled box can be one.

`toggled:` gives the control an on/off state, re-read whenever what it reads changes. One flag, and the role
says what it means, the way AccessKit models it:

| Role | `toggled:` is |
| --- | --- |
| `switch`, `checkbox`, `radio`, `menuitemcheckbox`, `menuitemradio` | checked |
| `button` | pressed: a toggle button, such as bold in a toolbar |
| `tab`, `option` | selected |

What it reads is a `bool` or a `Toggle`: `Off`, `On`, or `Mixed` for a checkbox standing for a set of which some are
ticked, or a toggle button whose selection is partly bold. Only checked and pressed can be mixed; a tab or an option
that reads `Mixed` is said as not selected.

```text
box role:button a11y_label:"Add one" on_press:(|| $count += 1)
    text "+1"
box role:switch toggled:$calm a11y_label:"Reduce motion" on_press:(|| $calm.set(!$calm.get()))
    text "≈"
box role:button toggled:$bold a11y_label:"Bold" on_press:(|| $bold.set(!$bold.get()))
    text "B"
```

Two combinations are build errors: `toggled:` on a box whose role carries no on/off state (no role, a region,
a slider), and an operated role other than `link` together with `to:`, since a box with a destination is a
link whatever else it said. A box that also has `on_key:` hears its keys first: an Enter or Space the handler
takes is the handler's, and one it leaves presses the box. A box with `consumes_keys:` is pressed only by the
Enter or Space it declares.

From Rust, the same box is `StyledContainer::control(role)` followed by `.toggled(|| …)`; the disclosure in
the catalogue's accordion uses the same flag for expanded.

A control inside a box that is disabled (`StyledContainer::disabled`) is disabled with it, the way a form's
fields are inside a disabled fieldset: it is no Tab stop, a reader cannot activate it, and every target says it
is unavailable, as `aria-disabled` in a document, AccessKit's disabled state on the desktop and in the snapshot
the lint reads.

| Target | Role | State |
| --- | --- | --- |
| Browser, document (`web-dom`) | A button is a `<button>`, a link an `<a>`; any other control is a `div` with the ARIA `role`. `tabindex="0"`, and the platform keeps Space and Enter from the page while the box holds focus. | `aria-checked` on a switch, checkbox or radio, `aria-pressed` on a button (either one `mixed` when partly on), `aria-selected` on a tab or an option, `aria-expanded` on a disclosure; only the one the role carries, so a box whose role changes drops the old one. |
| Desktop | The AccessKit role (`Switch`, `Button`, `CheckBox`…), with `Focus`, `Click` and `ScrollIntoView` actions, and `Increment` and `Decrement`, `SetValue` and `SetTextSelection` on a control that answers them. | AccessKit `toggled` for checked and pressed, `Mixed` included (it reads the flag through the role, as Telar does), `selected` for a tab or an option, `expanded` for a disclosure. |
| Terminal | In the plain-text reading (`Reduce motion, switch`). | `checked`/`not checked`/`partially checked`, `pressed`/`not pressed`/`partially pressed`, `selected` (an unselected tab says nothing), `expanded`/`collapsed`. |
| Android | The same AccessKit tree as the desktop, read as the widget class its role maps to (`Button`, `CheckBox`, `RadioButton`, `ToggleButton` for a switch, `SeekBar` for a slider, `EditText` for a field…). A control is clickable and focusable; one that steps is adjustable, the reader's scroll forward and backward reaching it as `Increment` and `Decrement`, and a slider that takes a number whole is also set to one directly. See [The Android tree](#the-android-tree). | `checked` for checked and pressed, a mixed state read as checked, and `selected` for a tab or an option. Expanded is not carried. |
| Browser, canvas (`web`) | Focus and keyboard activation as on the desktop, and the role as the document says it, on the control's element in the [canvas tree](#the-canvas-tree): `tabindex="-1"`, since the app walks its own Tab order. A reader's activation of the element reaches the control as `Activate`. | As the document says it. |
| Headless | Focus and keyboard activation as on the desktop. The snapshot carries role and state for tests; there is no reader to hand them to. | — |

A reader's request reaches a control through the focus order, not through the keyboard, as one of a closed set
of `AccessAction`s carried out by `focus::perform`: `Focus` moves the keyboard to it; `Activate` (a desktop
reader's `Click`) runs the control's activation, the code a tap on it runs, which the control registered as it
was built (`focus::set_activation`), so a control whose key handler answers Enter its own way is still
pressed, and one that is disabled, by itself or by a box around it, or out of reach is not; `ScrollIntoView` brings it into view in every scroll
it sits in. A value control steps by `Increment` and `Decrement` through the same code as its arrow keys
(`focus::set_stepping`, `StyledContainer::stepped`), as the catalogue's `slider`, `handle` and `scrub_field` do,
and one that takes a value whole registers `SetValue` (`focus::set_assignment`, `StyledContainer::assigned`),
as the `slider` does for a number. A field takes `SetValue` for its text and `SetSelection` for its caret or
selection, counted in the characters it shows. A row that a list, tree, table, menu, toolbar or palette walks
with a cursor while it keeps the keyboard (`StyledContainer::presented`) never takes focus: `Focus` on it moves
its owner's cursor onto it and the owner keeps the keyboard, and `Activate` runs the row's press, so the next
arrow key carries on from that row.

A link has one state of its own: `current:` marks it as the current one of its set, the page being shown or
the place on the page the reader is at. It is `aria-current` in a document, AccessKit's `aria_current` on the
desktop and `, current` in the terminal's reading; see [docs/links.md](links.md#the-current-link).

A control can also say what it shows and how it stands. Its value is a `Value`: `Number` for a slider, a spin
button or a splitter, with the words a reader says for it where the bare number would not ("Medium" for 2 of 3),
or `Text` for what a field holds and the choice a select has made. A box the keyboard never stops at carries
one the same way (`valued:` in a view, `StyledContainer::valued` from Rust): the catalogue's `progress` is a progress bar standing at its
reading out of 100, and its `spinner` a progress bar with no value, which says the work is under way and not
how far along it is; each is called what its call's `a11y_label:` names it, and `cargo telar test` reports one left unnamed. A `description` is what a reader says after
the name, role and state, as help rather than identity: a field's hint, why it is invalid, a tooltip's text.
`invalid`, `required` and `readonly` say that what a field holds fails its checks, that it must be filled in,
and that it can be read but not changed. A control declares them with `focus::set_description`,
`set_invalid`, `set_required` and `set_readonly` (or `StyledContainer::described`, `invalid`, `required` and
`readonly`), each read again whenever what it reads changes, and every target reads them from there. A
`text_field` sets them from its `error`, `required` and `hint`, and a `tooltip` describes the controls in its
trigger by what it says, showing for the keyboard as it does for the pointer and put away with Escape.

| Target | Value | Description and validity |
| --- | --- | --- |
| Browser, document (`web-dom`) | `aria-valuenow`, `aria-valuemin`, `aria-valuemax`, and `aria-valuetext` for its words on a slider, spin button, progress bar or splitter. A text value writes no attribute: the text the element holds is its value. | `aria-description`, `aria-invalid`, `aria-required`, `aria-readonly`. |
| Desktop | AccessKit's numeric value and bounds, and its `value` for the words or the text; a field's text is also a text run a line under it, and the caret and the selection of the field holding the keyboard are its text selection in those runs. | AccessKit's `description`, `invalid`, `required` and `read_only`. |
| Terminal | After the role: the words where it has them, the number otherwise (`Volume, slider, 40`). | `, required`, `, invalid`, `, read only`, then the description. |
| Android | The node's range where the value is a number with no words, its text otherwise: the words, or what a field holds, whose caret and selection are the field's text selection, which a reader moves by character, word, line and paragraph. | A read-only field is not editable; description, invalid and required are not carried. |

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
(a scroll area takes the second, a `text_field` both as props): the list a control opened, or the panel a
chosen tab shows (see [Tabs and radio groups](#tabs-and-radio-groups)), and the row its cursor rests on while
focus stays where it is. The row may sit inside the control or inside the list it controls.

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

## Tabs and radio groups

A set of tabs, a `radio_group` and a row of `swatches` are each read as one set and reached as one Tab stop, on
the chosen item or the first while none is chosen; the arrows move focus from item to item and choose the one
they reach, so a reader announces each as it is reached (see
[keyboard.md](keyboard.md#groups-that-are-one-tab-stop)). Every item is a control of its own that holds focus,
unlike the rows a cursor walks, and a reader moving to an item gives it focus without choosing it.

| Set | Its box | Its items |
| --- | --- | --- |
| `tabs`, `tab_list` | `tablist`, named by `label` | `tab`, `selected` while chosen |
| `radio_group` | `radiogroup`, named by `label` | `radio`, named by its item, `checked` while chosen |
| `swatches` | `radiogroup` | `radio`, named by its entry in `names`, `checked` while chosen |

The content a tab shows sits in a `tab_panel`, joined to its row by a `TabPanelLink` handed to both
(`TabsProps::panel`, `tab_list`'s `panel`, `TabPanelProps::link`). The panel is a `tabpanel` called by the
chosen tab's name, holding what it shows, and the chosen tab `controls` it, so a reader can go from the tab to
what it shows; a tab not chosen controls nothing, since its content is not built. The panel is no Tab stop:
Tab goes from the chosen tab to the first control inside it, and a reader asking to move to the panel takes the
keyboard there too.

| Target | The tab and its panel |
| --- | --- |
| Browser, document (`web-dom`) | `aria-controls` on the chosen tab naming the panel's `id`; the panel a `role="tabpanel"` element with `aria-label` and `tabindex="-1"`. |
| Desktop | AccessKit `controls` on the chosen tab; the panel a `TabPanel` node holding its content. |
| Terminal | The panel is read as `Pricing, tabpanel`, its content inside it; the relation is not in the reading. |

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

A live region stays too: a box whose role is `status`, `log` or `alert`, and what it says, are read whatever layer
is up, so a notice arriving while a dialog is open is still announced. Only what it says stays: the controls
inside it, a notice's action and dismiss buttons among them, are out of reach with the rest of the page, as Tab
already has them.

| Target | While a modal layer is up |
| --- | --- |
| Browser, document (`web-dom`) | `inert` on every box of the surface beside the layer, the control it was opened from and the live regions, written as high up as it can go: the page's outermost box, or the boxes beside the path down to them. Inside a live region only its controls go inert. A `dialog` inside the layer says `aria-modal="true"`. Both go when it closes. A layer in a surface drawn inside a frame shuts out only that surface, since each surface holds its own keyboard. |
| Desktop | The controls behind it were already left out of the AccessKit tree; the text and the named boxes behind it are left out too, but for those inside a live region, and a dialog inside it is AccessKit `modal`. |
| Terminal | The reading is the same snapshot, so what is behind it is not in the reading, but for what a live region says. |

## Notices

A `toaster`'s stack is a `status`, named by the localized `Notifications`, there before any notice arrives so a
reader hears each one as it comes. A notice's `ToastTone` is a closed set: `Info` and `Success` are heard when the
reader is next quiet, as part of the stack; `Warning` and `Error` make the notice an `alert`, heard at once over
what the reader is saying (`Role::live` reads assertive). The tone is part of what is heard, not only the colour of
the stripe: the notice is read as its name, the tone's localized word then its title and message, as in
`Error: Save failed. The file could not be written.`

A notice with an action (`ToastEnd::Action`) never puts itself away: reaching its button can take a reader longer
than any timeout. One on a clock (`ToastEnd::After`) holds it while the pointer rests on the stack, while the
keyboard is on one of its buttons, and while a modal layer is up (`focus::modal_layer_up`), so a notice that
arrives behind a dialog is still there when the dialog closes.

In a play the page behind a modal layer is not in the tree either; a tap that puts the layer away is
`click_outside` (see [previews.md](previews.md#play)).

## Where an item sits

A list or a tree that builds only the rows on screen leaves a reader counting a screenful, so each row it builds
says where it sits among them all: from Rust `StyledContainer::positioned(SetPosition { level, position, size })`,
each a `NonZeroU32` counted from one, or `positioned_by(|| …)` for a box that is no control and whose place
changes, such as the header row of a table. A target says only the numbers the row's role takes (`Role::positioning`):

| Role | Browser, document (`web-dom`) | Desktop | Terminal reading |
| --- | --- | --- | --- |
| `treeitem`, `listitem` | `aria-level`, `aria-posinset`, `aria-setsize` | AccessKit level and position in set, from zero | `level 2, 3 of 7` |
| `option`, `menuitem`, `menuitemcheckbox`, `menuitemradio`, `radio`, `tab`, `article` | `aria-posinset`, `aria-setsize` | Position in set, and the set's size on the node holding it | `3 of 7` |
| `row` | `aria-rowindex`, and the `grid` it is in `aria-rowcount`: the row's `size`, its header row included | Row index, and the grid it is in its row count | `1 of 501` |

Any other role says none of them. `tree_view` says it for its rows, `virtual_list` for its options and `data_table`
for its rows, its header row first.

A box that says where it sits is a node of the desktop tree and the terminal's reading even when it is no control
and has no name, such as a table's header row: its place is something only it can tell a reader, so it is read as
its role and its place (`row, 1 of 501`), with an empty name. The controls and the text inside it are read inside
it and keep their own names; none of their words becomes its name. Only a place its role takes earns that: an
unnamed box with no place, or with a place its role does not take (`Group`), says nothing a reader would hear and
is left out, so a list of plain `listitem` wrappers does not fill the tree with empty items (a structure such as
a `list` or a `grid` is the exception: see [The desktop tree](#the-desktop-tree)). On a raster frame the place travels on
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
the table its header, so the control still has a box to hold focus in. With no row for the keyboard's cursor to rest
on, a list or a tree wears the ring on its own box, and a table on its header, so the keyboard still shows where it is.
A menu, a select's list and the command palette mark the row their cursor is on with the keyboard's ring as well as
the pointer's wash while the keyboard shows focus there; a menu a press opened shows no ring, as a press that hands
focus on shows none, and the list a select opens is called what the select is called.

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

A reader landing on it has to hear what it scrolls. `a11y_label:` on `scroll` (`LayoutScrollArea::stop_label(|| …)`
from Rust) names it while it is a stop and leaves it nameless otherwise, so a region that fits is not announced
a second time inside the box that already names it. The catalogue's `modal` names its body after its title and
`drawer` after its label. A scroll area that is a control is named the way any box is, with `.a11y_label`; markup
has no way to make a `scroll` one, so there `a11y_label:` always means the stop's name.

| Target | A scroll area that is a stop |
| --- | --- |
| Browser, document (`web-dom`) | `tabindex="0"`, with `role="group"` and `aria-label` once it is named, and `data-telar-keys` holding only the keys for the axes it overflows. The browser scrolls the element itself, the way Telar asks it to. |
| Desktop | An AccessKit `ScrollView` with `Focus`, named by its name, with the text it draws read inside it, or, without one, named by that text. |
| Browser, canvas (`web`) | A stop in Telar's own Tab order, ringed when focused, and in the [canvas tree](#the-canvas-tree) an element with `role="group"` and `aria-label` once it is named, which takes the document's focus while the app's is on it. |
| Android | A stop in Telar's own Tab order, ringed when focused; to a reader, the desktop's `ScrollView` read as a list (`ListView`) named by its name, which the reader does not stop on itself. |
| Terminal and headless | A stop in Telar's own Tab order, ringed when focused, and a scroll area with its name in the snapshot. |

## Shortcuts

A control that runs the same command as a key says which, so a reader can tell the key before it is used:
from Rust `StyledContainer::keyboard_shortcut(move || Some(redo.clone()))` on the control the key activates
(never on a tooltip, which only describes it), where `redo` is the `Chord` parsed from `"Mod+Shift+Z"` where
it was declared, its parse error reported there. The reader is told
`Chord::aria` of it under the keyboard convention in force: the ARIA `aria-keyshortcuts` grammar with every
key spelled as UI Events names it, so `Mod+Shift+Z` is `Control+Shift+Z` on a PC and `Shift+Meta+Z` on a
Mac, and `Ctrl++` is `Control+Plus`. It is `aria-keyshortcuts` in a document, AccessKit `keyboard_shortcut`
on the desktop, and `, shortcut Control+Z` in the terminal's reading; `None` says nothing.
The catalogue sets it on a menu row with `shortcut`, on an `icon_button` and a tab with `shortcut`, and on a
command palette row. Those props are a `KeySequence`, which may be a sequence (`"Mod+K S"`) to show; a reader
is told the keys only of a sequence of one chord, since `aria-keyshortcuts` has no form for a sequence.

The visible shortcut stays in the control's name (`Undo, Ctrl+Z`; a palette row is `Open file, Ctrl+O`).
`aria-keyshortcuts` is a hint on top of the name, not a replacement for it: VoiceOver and TalkBack do not
speak it, and WAI-ARIA gives no way to ask a reader to, so dropping the drawn keys from the name would take
the only announcement away from them. A reader that does speak the attribute says the keys twice; that
redundancy is the price of not leaving the others without. A tooltip's shortcut is neither: it is drawn for
the pointer and belongs to the control, not to the bubble.

## What each target does with them

| Target | `a11y_label` | `lang` | `a11y:hidden` |
| --- | --- | --- | --- |
| Browser, document (`web-dom`) | `aria-label`. A named box with no role of its own that draws words outside its controls says its name instead as visually hidden text, its first child (`<span data-telar-spoken>`), and its words are `aria-hidden`, its controls left as they are; one that draws none, such as a group of radios, gets `role="group"`, since a plain `div` may not carry a name. A picture is an `<svg role="img">`, where `aria-label` is its `alt`. | `lang` | `aria-hidden="true"` |
| Desktop | AccessKit node label. A named control is called that instead of by the text it draws; a named box that is not a control is a label, or an image when it draws only artwork, and a label's name replaces the text drawn in it outside its controls. | AccessKit `language` | The subtree, controls included, is left out of the tree. |
| Terminal | Used in the plain-text reading (below). | Not in the reading: plain text has no voice to switch. | Left out of the reading. |
| Android | The node's `contentDescription`, from the same tree as the desktop, so a control, a label and an image are called what they are on the desktop. | Not carried: the adapter sets no locale on a node, so the reader speaks it in the device's language. | The subtree, controls included, is left out of the tree, as on the desktop. |
| Browser, canvas (`web`) | The name of the node's element in the [canvas tree](#the-canvas-tree): its `aria-label`, or its own text where it is static text. | `lang` on the node's element | Left out of the snapshot, so no element stands for it. |
| Headless | No reader. The snapshot (`ui_core::accessibility::snapshot`) still resolves them, which is what the tests read. | — | — |

`lang:` is per subtree. The surface's own language — the root the subtrees differ from — is the active
locale's (`use_locale()`), and each target says it in its own idiom:

| Target | The surface's own language |
| --- | --- |
| Browser, document and canvas (`web-dom`, `web`) | `<html lang>`, kept in sync with the active locale at runtime; `<html dir>` follows it the same way `i18n_core::Direction::for_locale` resolves the writing direction, and both start from `[telar.web]`'s packaged page (see [web-packaging.md](web-packaging.md)). |
| Desktop | The AccessKit root node's language. A node under it that carries no `lang:` of its own inherits this — the nearest one wins, root included. |
| Terminal | Nothing to set: the reading is plain text, and a reading has no voice to switch (as above). |
| Android | Not carried, as for `lang:` itself: the root's language is in the tree, and the adapter gives a reader no locale to read it in, so the device's language is spoken. |
| Headless | No reader, so nothing to carry it to. |

## The terminal's plain-text reading

A terminal screen reader reads the cells, and cells carry neither a name the application gave nor what it
hid. Set `TELAR_TUI_READING` to a file path (or `TuiPlatformConfig::reading`) and the terminal frontend
keeps that file as a reading of the screen, rewritten whenever it changes: one line per thing on screen,
in reading order, named the way a screen reader would name it (`Close, button`, `Telar logo, image`), and
indented two spaces for every node it sits in, so a grid's rows read under the grid and a row's cells under the row
(see [The desktop tree](#the-desktop-tree)).
Following it from a second terminal gives a reader what the cells cannot. Off by default, so a frame costs
nothing when nobody reads it.

```sh
TELAR_TUI_READING=/tmp/reading.txt cargo telar dev --target tui
tail -F /tmp/reading.txt
```

The reading ends the line of whatever holds the keyboard with `, focused` (`Save, button, focused`), since a
reader following the file has no ring to look at. A link marked `current:` says so after its role (`Web, link, current`).
An item that says where it sits says it after its role, in the numbers its role takes (`src, treeitem, level 2, 3 of 7`;
see [Where an item sits](#where-an-item-sits)), a heading says its level after its role (`Settings, heading, level 1`),
and a node with no name reads as its role alone (`row, 1 of 501`).

## Names the catalogue gives

A `text_field` is named by its `label`, or by its `placeholder` when it has no label: the caption drawn above
the box is not inside the line a reader lands on, so without the name a field is announced as an edit box
with no name. `a11y_label:` with an empty string is no name at all: the box goes by what it draws, as if
nothing had been said.

In the document, a combobox, a slider, a spin button or a splitter that nothing named is named by the text
drawn inside it, the name every other target gives it. A browser takes that text as the value of those roles
and their name only from a label, so a `select` showing "Small" was nameless there; every target reads the
choice a select shows as its value too. A field is not named by what it draws on any target: what it draws is
what was typed into it, which is its value, so a field nothing named stays unnamed.

## The document's text entry

A field is typed into through an element the document renderer parks, invisible, at the field's caret: an
`<input>` for a field of one line (`type="password"` for a secret one) and a `<textarea>` for a text area. It
holds the keyboard, so it is the element a screen reader reads, and it mirrors the field: its value is what the
field shows (the mask, for a secret field, never the value) and its selection is the field's selection, both
written again after every change to the field. It goes by the field's own name, and by none when the app gave
the field none. Every edit the browser reports on it is prevented and handed to the field as the keys that
make it, so the field's next frame is what changes the element; a composition is the one thing the browser
writes into it, over the field's selection, and it reaches the field as `Ime` steps and one commit. A caret or
a selection a screen reader or a selection handle moves in it reaches the field as `Event::SelectionMoved`,
which the field takes as it takes a reader's `SetSelection`.

## Contrast of the defaults

The tokens a theme leaves to `ThemeTokens` meet WCAG AA (4.5:1) for the text the catalogue draws with them, and
3:1 (WCAG 1.4.11) for what tells a control from the page. `primary` and `on_primary` follow the light/dark mode,
since one blue cannot carry white text and also read as text on a dark surface; `muted` and the status colours
(`success`, `warning`, `info`, `error`) are opaque and mode-following, so a caption, a hint, a code token or a
counter over its limit reads on `surface` and on `surface_alt` over it; and a field's placeholder keeps 70% of the
field's ink rather than half.

`border` is the decorative hairline, a divider or a panel's edge, and may stay faint. What identifies a control at
rest — a field's frame, an unchecked checkbox, an unselected radio or filter, an off switch's track, a select's
trigger, an unchosen swatch — is drawn in `control_border`, which by default is the first step from `surface`
toward `muted` that stands 3:1 from `surface` and from `surface_alt` over it, so a theme that sets its own surfaces
and `muted` gets control boundaries that follow them. A theme that overrides a token owns its contrast: theme-core
holds the defaults to every pair the catalogue draws, and `contrast_shortfalls(theme)` checks a theme of one's own
against the same table, at the regular contrast and at the high contrast derived from it (see
[themes](themes.md#high-contrast)); the audit below checks what a screen actually draws.

## Forced colors

A browser in `forced-colors: active` replaces the colours a page sets with the user's palette, and it drops what it
cannot recolour: a `box-shadow`, and a background that says a control is on. The document renderer therefore draws
every frame as a real CSS line. An even frame is an `outline` pulled inward by its own width, so it follows the
radius and takes no room from the box; an uneven one, such as a rule on one side, is a `border` of those widths. A
frame the theme wants invisible is still declared, in a transparent colour, so forced colors has a line to paint,
and a gradient frame stays a background layer with a transparent line beside it. Only the drop shadow remains a
`box-shadow`.

The document's stylesheet then says what each part becomes under `forced-colors: active`:

| Part | Painted as |
| --- | --- |
| A frame | `CanvasText` |
| A box that is pressed, checked (or mixed) or selected | a `Highlight` fill with `HighlightText` for the text in it |
| The focus ring, both bands | `Highlight`: the focused element's own outline, and the paint inside it |
| A picture (`img`, `svg`) | its own colours: the one place `forced-color-adjust: none` is set |

Nothing else opts out of forced colors, so no part of the page keeps a colour the user did not choose.

## Audits

Two tests hold every target to the same fixture: a heading, a button, a link to an external address and one
to a route, a labelled field, a checkbox, a switch and a toggle button built the way `role:` boxes are, a
paragraph with a link run, a word named with `a11y_label:` over hidden letters, a quotation in another `lang:` and
a box under `a11y:hidden`.

| Test | What it checks |
| --- | --- |
| `plugins/telar-components/src/audit.test.rs` | The document those widgets render to, audited by [axe-core](https://github.com/dequelabs/axe-core) with its WCAG 2.2 A/AA and best-practice rules (`region` off: the fixture is a fragment of a page). Run with nothing focused, with the field focused and with the button focused, and any violation fails the test with the rule, the element and the reason. It also checks that the keyboard's box wears Telar's ring in the document, and that a dialog whose body scrolls makes the body a named Tab stop that Page Down scrolls. The page holds a list and a table with more rows than they show, which say where each built row sits, and an open menu leaves them and the rest of the page inert until it closes. |
| `crates/platform/platform-accesskit/src/lib.test.rs` (`from_a_screen`) | The AccessKit tree a desktop or Android window publishes for the same screen, built from `ui-core` primitives in place of the catalogue's button, field and checkbox: each control's role and name, that it takes `Focus` and `Click`, the URL of each link and link run, the state of the checkbox, the switch and the toggle button, that Space and Enter press the last two, that a link to the section under a bar is the current location as the page scrolls, the language and the hidden boxes, that the heading is a heading named by its text at its level, that a role said from outside a widget is published over a plain box and never over a control, and that the tree's focus follows the Tab order. |
| `crates/renderer/renderer-dom/src/controls.test.rs` | A switch and a toggle button in the document: their role, `aria-checked` and `aria-pressed`, that each is a Tab stop, and that Space or Enter sent to the element is kept from the page, reaches the box through the platform and flips the state the next frame writes. `semantics.test.rs` checks that a box whose role changes drops the state attribute of the old one. |
| `crates/renderer/renderer-dom/src/mirror.test.rs` | The canvas tree: that a snapshot's roles, names, values and states are said on the elements standing for its nodes, nested as the snapshot nests them and none a Tab stop; that a click on a control's element activates the control through the platform; and that focus follows between the elements and the app both ways. |
| `crates/tools/cargo-telar/src/workshop_audit.test.rs` | The exported `apps/catalogue` workshop in headless Chrome: axe-core over the canvas of every preview and matrix page, with the same rules (`crates/renderer/renderer-dom/axe.json`), and over the workshop's chrome once with `region` on. See [previews.md](previews.md#auditing-the-exported-workshop). |

axe-core is MPL-2.0 and is not vendored into the repository. The flake fetches the published npm tarball by its
integrity hash and puts `axe.min.js` and its `LICENSE` in the store, and the dev shell names the script in
`TELAR_AXE_CORE`, which the test reads at compile time. Moving to another release is a change to the URL and the
hash in `flake.nix`. Run it in Firefox:

```sh
nix develop -c nix shell nixpkgs#firefox nixpkgs#geckodriver -c sh -c \
  'export GECKODRIVER=$(which geckodriver); unset CHROMEDRIVER; cargo test -p telar-components --target wasm32-unknown-unknown --test audit'
```

Where the keyboard is shown on each target is in [keyboard.md](keyboard.md#what-shows-where-the-keyboard-is).

## The desktop tree

The desktop tree has the structure a reader steps into. The snapshot (`ui_core::accessibility::snapshot`) is a
tree listed in reading order: each `AccessNode` names the node it sits in by `parent`, and comes after it and
before that node's next sibling, so reading the list in order is reading the tree depth first. A node sits in the
nearest box around it, along the frame's element nesting, that is a node itself: a control, a box the application
named, a box that says where it sits, or a box given no name whose role is a structure — a `tablist`, `tabpanel`, `radiogroup`,
`menu`, `listbox`, `tree`, `grid`, `row`, `toolbar`, `list`, `dialog`, `status`, `log`, `alert`, `article` or a landmark
(`banner`, `navigation`, `main`, `complementary`, `contentinfo`, `search`) — as long as it holds a node; holding
none, it is left out. A box whose role an application said with `role:` on a call (`a11y_role`) is such a node for any role a call may say but `group` — a `listitem`, an unnamed `section` or `form` included — since a role said on purpose is never scaffolding; one a widget gave its own wrapper is a node only as above. The boxes between them, plain groups and scroll areas, are not nodes. AccessKit gets each landmark as its own role (`search` as `Search`), `status` and `log` as polite live regions and `alert` as an assertive one. A box named as a group
reads as a label beside what it groups rather than around it. A heading given no name is a node too, as the
document's `<h2>` is: a heading of its level named by the words drawn in it, and by the names of the boxes with no
role of their own inside it, holding the link runs and controls among them; its words name it and are not read
again as text of their own. AccessKit gets its level (`level`, counted from zero) and the terminal says it
(`Settings, heading, level 1`). A surface nested in the frame joins the tree inside the node it is drawn in.

AccessKit gets the same tree: every node is a child of the one it sits in, and the window holds the rest. A node is named by its label, except a line of text (`Label`), whose words are its value, where every AccessKit adapter reads them. A grid
holds its rows and each row its cells; the grid says its row count — from the `size` its rows say, so a reader
hears every row of a table that builds a screenful — and its column count, each row its index, and each cell its
column and its row's index. A set whose items agree on its size, such as the options of a list box, says it on the
list, where AccessKit reads it; a tree whose rows sit at several levels leaves each row's level and place to say it.
A control is named by its `AccessId` wherever it sits — its focus id, with the key of the nested surface that holds
it, if any — and AccessKit is handed the one integer `AccessId::to_bits` makes of it, so focus and the row a tree's
cursor rests on stay the same nodes between updates.

Siblings are in reading order, by where they show — of two at the same corner the larger first — except that a box
held at the edge of a scrolled view (`sticky`) is read where the flow puts it rather than where it shows. A table's
header row is read before the rows built above the view and the rows scrolled under it, as the document reads it.

The snapshot is laid out in logical pixels and a reader works in the screen's own, so the window node scales the
whole tree by the window's scale factor: on a dense screen the reader's highlight sits on the control and a touch
to explore lands on the one drawn under it.

## The Android tree

Android publishes the desktop's tree: one mapping, `telar-platform-accesskit`, turns the snapshot into an
AccessKit tree for both, and a request coming back is the same `AccessAction` either way, so a control reads and
answers alike on both. On Android the tree hangs on the activity's decor view as its virtual views, through
AccessKit's Android adapter, whose view delegate is loaded from the dex the adapter embeds, since an APK built
from Telar carries no Java of its own. That loader is API 26 and later; on an older device nothing is published
and the app says so in the log.

The adapter is attached each time the activity's window comes back and dropped when it goes, so it always hangs on
the activity that is showing. Nothing is built until a service first asks for the tree; from then on the tree is
published after every frame, since Android never says the service stopped listening; while accessibility is off
the adapter keeps the tree and raises no event. A request arrives on Android's UI thread and is carried to the
thread the UI lives on, where it takes the same path a tap or a key does.

What the adapter hands a reader is a subset of the tree: each node's role as a widget class, its name, its text or
range, checked and selected, enabled, focusable and focused, editable, the text selection of a field, its link,
its live region, whose changes are announced, and click, focus and text navigation. A control that steps is
adjustable: the reader's scroll forward and backward, the gesture it adjusts with, reach it as `Increment` and
`Decrement`, and one whose value is a number with no words, and that takes a value whole, is also set to a value
directly (set progress), as `SetValue`. Expanded, description, validity, required, a heading and its level, the
language, `controls`, the active descendant, a node's place in its set and a grid's counts are in the tree and not
carried on.

Touch to explore does not reach the tree yet. The system sends it as hover events through the activity's input
queue, and the event loop takes them there without passing them on to the view, so they never reach the adapter.
Swiping from node to node, a double tap to activate, and adjusting a slider all go through the tree and work.

## The canvas tree

A canvas is pixels, so the browser has nothing in it to read. After each frame the canvas runner hands the same
snapshot the desktop publishes to `CanvasSurface::describe`, which keeps a document nobody sees beside the canvas
(`renderer_dom::AccessMirror`): an element for each node, nested as the snapshot nests them and placed on the rect
the node is drawn on, so a reader that outlines what it reads, or finds what is under a finger, finds the drawing.
The mirror is transparent and takes no pointer; the canvas under it gets every click and touch.

Each element says what its node is in the document renderer's words, through the same mapping (`Described` in
`renderer-dom`): the same tag or `role`, `aria-label` for the name, the same state attributes, `aria-valuenow`,
`-valuemin`, `-valuemax` and `-valuetext` on a range, `aria-invalid`, `aria-required`, `aria-readonly`,
`aria-disabled`, `aria-description`, `lang`, `aria-level`/`-posinset`/`-setsize`, a grid's `aria-rowcount` and the
`id`s `aria-activedescendant` and `aria-controls` point at. Static text is the element's own text, and a field or a
select holds the text it shows as its value, as a document's would. A control keeps its element between frames,
so a reader resting on it stays there.

What a reader does to an element comes back as the platform's own event, `Event::AccessAction`, which the runner
carries out through `focus::perform` as it does a desktop reader's request: focus landing on an element asks its
control to take focus, and a click on one (a reader's activation) activates it, giving it focus first as a tap
does. The other way, while the keyboard is in the surface, the element of the control holding the app's focus holds
the document's, which is what a reader announces as the app's Tab moves. No element is a Tab stop
(`tabindex="-1"`): the app walks its own Tab order, and a Tab it releases at an end of that order (see
[keyboard.md](keyboard.md#where-tab-goes-at-the-ends)) moves the browser's focus out of the surface rather than
onto the next element. Enter and Space on an element reach the app as keys, never also as a click of the
element's own.

A prerendered page stays what a reader reads until the mirror has something to say. From then on the page the
host was served with is `inert` and `aria-hidden`, and it is taken away once the first frame is drawn over it.

A link run, which the snapshot names but gives no id, is read as a link and cannot be followed from the mirror,
as on the desktop; a link box, which is a control, can.
