# telar-plugin

Embedding a separately-compiled Telar UI inside a Telar application.

A plugin is a `cdylib` with its **own** reactive, layout, overlay and motion runtime — every one of those lives in a thread-local, and each dylib statically links its own copy of the crates that own them. The host cannot reach into that runtime, so it *drives* the plugin across the FFI boundary through exported shims, and the plugin hands back a self-contained `Vec<DrawCommand>` the host translates, clips and splices into its own frame. No offscreen texture, no shared GPU device: one renderer paints everything in one pass.

Not the same thing as hot reload, which is dev-only and swaps the *whole* window.

```toml
# In the plugin
telar-plugin = "0.1.8"

# In the host
telar-plugin = { version = "0.1.8", features = ["host"] }
```

The plugin implements `EmbeddedApp` and calls `plugin!` to export the shims; the host calls `load_plugin` and drives what it returns.

Keep it on the same version as `telar` — the same lockstep `telar` and `telar-macros` already have, and for the same reason.

## License

Licensed under either of [Apache License, Version 2.0](../../LICENSE-APACHE) or [MIT license](../../LICENSE-MIT) at your option.
