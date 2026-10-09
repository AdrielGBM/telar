# telar-embed

Embedding a separately-compiled Telar UI inside a Telar application.

An embedded app is a `cdylib` with its **own** reactive, layout, overlay and motion runtime — every one of those lives in a thread-local, and each dylib statically links its own copy of the crates that own them. The host cannot reach into that runtime, so it *drives* the guest across the FFI boundary through exported shims, and the guest hands back a self-contained `Vec<DrawCommand>` the host translates, clips and splices into its own frame. No offscreen texture, no shared GPU device: one renderer paints everything in one pass.

Not the same thing as hot reload, which is dev-only and swaps the *whole* window.

```toml
# In the guest
telar-embed = "0.2.2"

# In the host
telar-embed = { version = "0.2.2", features = ["host"] }
```

The guest implements `EmbeddedApp` and calls `embed!` to export the shims; the host calls `load_embedded` and drives what it returns. The host must call `LoadedEmbed::on_frame` every frame for guest timers to fire and wake the event loop.

Keep it on the same version as `telar` — the same lockstep `telar` and `telar-macros` already have, and for the same reason.

## License

Licensed under either of [Apache License, Version 2.0](../../../LICENSE-APACHE) or [MIT license](../../../LICENSE-MIT) at your option.
