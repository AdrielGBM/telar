# telar-semantics-core

What a thing in a Telar interface is: the roles a screen reader, a document and a terminal all describe the same box with.

Part of [Telar](https://github.com/AdrielGBM/telar), a modular Rust UI framework with its own template
language, reactive signals and a self-contained renderer.

Besides roles, it defines `ConsumedKeys`, the keys a focused box keeps from its host (Tab, Space, Enter, the arrows,
paging and Home/End), with each role's default in `Role::consumed_keys`. `Semantics::focusable` carries the
set and whether the box is a Tab stop right now. See [docs/keyboard.md](https://github.com/AdrielGBM/telar/blob/main/docs/keyboard.md).

A control's on/off state is one `Toggle` (`Off`, `On`, `Mixed`), `Semantics::toggled`, read through its role:
`Role::toggle_kind` says whether it is checked (checkbox, radio, switch), pressed (a button, which makes it a toggle
button), selected (a tab) or expanded (a disclosure). Its value is a `Value`, a number with optional words or a
text; `description`, `invalid`, `required` and `readonly` say the rest of what a field is. A live region's urgency,
`Live`, is its role's to say (`Role::live`): `status` and `log` are polite, `alert` assertive. See [docs/accessibility.md](https://github.com/AdrielGBM/telar/blob/main/docs/accessibility.md#controls-and-their-state).

A link carries where it goes and whether it is current in its role, `Role::Link(Link)`, so nothing else can;
what a current link is the current one of (`CurrentKind`: a page, a location on the page, or plainly the
current item) is its destination's to say, through `Link::current_kind`. See [docs/links.md](https://github.com/AdrielGBM/telar/blob/main/docs/links.md#the-current-link).

**Applications depend on the [`telar`](https://crates.io/crates/telar) facade, not on this crate.** Telar
is split into small crates so a build carries only the target and the capabilities it named, and every one
of them has to be published for the facade to be. The facade re-exports what an application needs behind
feature flags; reach for this crate directly only if you are writing a frontend or a tool against Telar's
internals.

- API documentation: <https://docs.rs/telar-semantics-core>
- The framework, and where to start: <https://github.com/AdrielGBM/telar>
