# telar-embed

Embedding a separately-compiled Telar UI inside a Telar application.

An embedded app is a `cdylib` with its **own** reactive, layout, overlay and motion runtime — every one of those lives in a thread-local, and each dylib statically links its own copy of the crates that own them. The host cannot reach into that runtime, so it *drives* the guest through the entries the guest exports, and the guest lends back its frame as bytes the host decodes into draw commands of its own, then translates, clips and splices into its own frame. Host and guest meet on the same `repr(C)` contract hot reload uses: nothing owned crosses and each side frees only what it allocated, so either side may have its own `#[global_allocator]`. No offscreen texture, no shared GPU device: one renderer paints everything in one pass.

Not the same thing as hot reload, which is dev-only and swaps the *whole* window.

```toml
# In the guest
telar-embed = "0.2.2"

# In the host
telar-embed = { version = "0.2.2", features = ["host"] }
```

The guest implements `EmbeddedApp` and calls `embed!` to export its entries; the host calls `load_embedded`, an `unsafe fn` because loading a library runs foreign code, and drives what it returns. The host must call `LoadedEmbed::on_frame` every frame for guest timers to fire and wake the event loop. Each `load_embedded` opens a private copy of the library, so a rebuilt library at the same path never shares statics with one still mapped, and a guest built against another contract, with other features than its host or as a hot-reload library is refused before any of its code runs but its initializers. A panic inside a guest never unwinds into the host: `LoadedEmbed::panic` reports what it said and where, and nothing more of that guest runs.

Keep it on the same version as `telar` — the same lockstep `telar` and `telar-macros` already have, and for the same reason.

## License

Licensed under either of [Apache License, Version 2.0](../../../LICENSE-APACHE) or [MIT license](../../../LICENSE-MIT) at your option.
