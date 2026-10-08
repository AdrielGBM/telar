# telar-expression

A small typed expression language for Telar applications: the formulas a user writes against an application's live readings, checked before they run and bound to signals so each recomputes only when something it read moves.

```text
fmt("{}%", round($battery.level))
if($media.playing, mix($theme.accent, #ffffff, 20%), #888)
200 + 10%        → 220
```

**Pure and total.** An expression reads values and computes one: no commands, no I/O, no clocks, so an expression arriving in a shared bundle is safe to evaluate. Every failure, from a stray character to a division by zero at run time, is an error value carrying the byte span it is about, and `render` draws it with a caret under the span.

**Typed before it runs.** `compile` checks an expression against the application's `Environment` (the type of every `$reference`) and a `Registry` of functions. The standard registry has text (`fmt`, `upper`, `replace`, …), maths (the calculator's functions and constants, `round`, `min`, `max`, `pow`), lists (`len`, `at`, `join`) and colour (`mix` in Oklch, `alpha`, `contrast`); an application adds its own families with typed signatures.

**Reactive.** `bind` turns a compiled expression and a resolver that reads the application's signals into a `Memo` whose dependencies are exactly what the last evaluation read: an `if` subscribes only to the branch it took. `bind_held` keeps the last good value through an error.

**A calculator, too.** `calculate("2^3^2")` answers `512`, strictly enough that `2 + 3 firefox` is an error rather than `5`, and `format_number` prints the result without binary noise.

```toml
telar-expression = "0.2.2"
```

Keep it on the same version as `telar` — the same lockstep `telar` and `telar-macros` already have, and for the same reason. `telar` does not re-export it: an application that binds properties to formulas names it as its own dependency.

## License

Licensed under either of [Apache License, Version 2.0](../../LICENSE-APACHE) or [MIT license](../../LICENSE-MIT) at your option.
