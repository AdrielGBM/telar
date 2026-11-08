# telar-embed

Embedding a separately-compiled Telar UI inside a Telar application.

An embedded app is a `cdylib` with its **own** reactive, layout, overlay and motion runtime — every one of those lives in a thread-local, and each dylib statically links its own copy of the crates that own them. The host cannot reach into that runtime, so it *drives* the guest through the entries the guest exports, and the guest lends back its frame as bytes the host decodes into draw commands of its own, then translates, clips and splices into its own frame. Host and guest meet on the same `repr(C)` contract hot reload uses: nothing owned crosses and each side frees only what it allocated, so either side may have its own `#[global_allocator]`. No offscreen texture, no shared GPU device: one renderer paints everything in one pass.

Not the same thing as hot reload, which is dev-only and swaps the *whole* window.

```toml
# In the guest
telar-embed = "0.2.3"

# In the host
telar-embed = { version = "0.2.3", features = ["host"] }
```

The guest implements `EmbeddedApp` and calls `embed!` to export its entries; the host calls `load_embedded`, an `unsafe fn` because loading a library runs foreign code, and drives what it returns. The host must call `LoadedEmbed::on_frame` every frame for guest timers to fire and wake the event loop. A guest that shapes its own text sets it in the faces its host draws it in, whatever features either side was built with: the host serves its font database, the guest loads what that holds the first time it needs a face, and a face the guest adds is added to the host's first.

The guest's surface is part of the host's window, not just its pixels:

- **Draw it marked as the guest's.** `composite(rect, embed.boundary(), embed.paint())` places the guest's frame in the host's and marks it, so a screen reader reads the guest's controls where the host shows them, under ids of their own, and its actions on them reach the guest; the host's inspector reads them too.
- **Give it one focus stop.** `embed.focus_stop(node, placement)` registers the stop in the host box at `node` the guest is drawn in, with `placement` mapping the guest's coordinates into that box's. Tab onto it hands focus to the guest's first control, Shift+Tab to its last; Tab past either end of the guest carries on through the host; losing it clears focus inside; and focus the guest takes on its own, a press or a reader's action gives the stop focus. Exactly one side has a focus owner.
- **Route events through it.** Hand `dispatch_overlays` and then `on_event` the host's events in the guest's coordinates: keys and the input method's text reach the guest only while its stop holds the keys, Tab moves through it, and the input method is told where the guest's field has its caret, as the host shows it. A box event the host's window reports inside the guest's frame (`Event::InSurface`) is passed on as the guest names that surface.
- **Let its drags be the window's.** A drag the guest starts, the drop it accepts and whether the picture following the drag shows are performed for the host's surface as the host's own, during the dispatch that asked for them. Route the host's drag events through it like the pointer's: a drag over the host reaches the guest only over the area it is laid out to, leaving that area reaches it as `Event::DragLeft`, and `Event::DragEnded` reaches it only while a drag it started is in flight, so a host with several guests hands it to each. A host that stops showing a guest mid-drag hands it `Event::DragLeft`. `drain_window_commands` hands the host everything else the guest queued, to apply or ignore.

When the guest panics or is dropped, its stop is withdrawn, its caret is gone and nothing of it is read any more. Each `load_embedded` opens a private copy of the library, so a rebuilt library at the same path never shares statics with one still mapped, and a guest built against another contract or as a hot-reload library is refused before any of its code runs but its initializers. A panic inside a guest never unwinds into the host: `LoadedEmbed::panic` reports what it said and where, and nothing more of that guest runs.

Keep it on the same version as `telar` — the same lockstep `telar` and `telar-macros` already have, and for the same reason.

## License

Licensed under either of [Apache License, Version 2.0](../../../LICENSE-APACHE) or [MIT license](../../../LICENSE-MIT) at your option.
