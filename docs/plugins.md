# Writing a plugin

`telar` is the mechanism: traits, protocols and registries. What an application opts into on top of it is a
plugin, a crate the application adds to its own `Cargo.toml`. The README says how to
[add one](../README.md#plugins); this page is for the person writing it.

A plugin is either

- a **Rust crate** that depends on `telar` and follows the component protocol below, as `telar-components` and
  `telar-navigate` do, or
- a **`.rsx` library**, a package whose `telar.toml` says `library = true`, published with `cargo telar publish`.

Both reach an application the same way: `cargo add`, then one line in the application's `telar.toml`. Neither
needs anything from `telar` that an application's own components do not have, and that is the test of the
design: a plugin is a component crate that happens to be somebody else's.

## Where a plugin lives

`plugins/` holds every crate built on top of the facade: the ones an application author adds to their own
`Cargo.toml`, and the ones the tooling adds on its own. `telar-devtools` is one of the latter: an application
declares it as an optional dependency (`cargo telar new` writes it), `cargo telar dev` turns its feature on,
and `telar::app!` installs its overlay only under that feature, so no shipping build compiles it.

`crates/` holds `telar`, the kernel crates behind it, and host-side tooling in `crates/tools/`. A crate that a
`telar` feature pulls in and compiles into the target is core and lives there, even when it is opt-in.
Nothing under `crates/` may depend on `plugins/`, in any dependency kind, and
`.github/scripts/check-layering.sh` fails the build when something does: the facade never names a plugin,
and a plugin reaches an application through the application's own manifest. A plugin you write outside this
repo has no such constraint beyond the next two sections.

## Versioning: in lockstep with `telar`

Every kernel type a plugin takes or returns is reached through the facade. Two versions of `telar` in one
build resolve to two copies of the kernel, and a widget built by one stops being the `LayoutItem` the other's
tree accepts: a type error naming one trait twice. So a plugin is released at the version of the `telar` it was
written against and says so in its crate docs, as [`telar-components`](../plugins/telar-components/src/lib.rs)
does. A Rust plugin depends on `telar` by that exact name, because `#[telar::component]`, `t!` and generated
code spell `::telar::…` and `use telar::*`; a renamed dependency is not supported.

A `.rsx` library pins it with `=`:

```toml
[dependencies]
telar = { version = "=0.2.2", default-features = false, features = ["runtime"] }
```

`cargo telar new --lib` writes that line with the version of the `cargo-telar` that ran. The pin matters more
here than for a Rust crate: a library ships the code `.rsx` transpiled to, and that code is accepted only by the
`telar` it was transpiled for.

## A plugin in Rust

### The component protocol

A component is a props struct and a function. The struct derives `Props`, which gives it a builder
(`ButtonProps::props().label("Save").build()`), and the function takes it and the nested children and returns a
layout item. This is [`badge`](../plugins/telar-components/src/badge.rs), nearly in full:

```rust
use telar::{
    BorderRadius, Children, Color, LayoutError, LayoutItem, LayoutStyle, Props, Reactive, RectStyle,
    StyledContainer, SurfaceStyle, Text, amend_surface, box_item, use_theme_tokens,
};
use std::rc::Rc;

#[derive(Props)]
pub struct PillProps {
    #[props(into, default)]
    pub label: Reactive<String>,
    #[props(some, default)]
    pub style: Option<Rc<dyn Fn(RectStyle) -> RectStyle>>,
}

pub fn pill(props: PillProps, _children: Children) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let PillProps { label, style } = props;
    let surface: SurfaceStyle = style;
    let label = Text::declaring(move || label.get(), LayoutStyle::new(), |inherited| {
        inherited.with_color(use_theme_tokens().on_primary())
    })?;
    let container = StyledContainer::new(
        LayoutStyle::new().flex_row(),
        move |_rect| {
            let base = RectStyle::default()
                .with_fill(use_theme_tokens().primary())
                .with_radius(BorderRadius::all(use_theme_tokens().radius()));
            amend_surface(base, &surface)
        },
        vec![box_item(label)],
    )?;
    Ok(box_item(container))
}
```

The tag is the function name, and its props type is that name in PascalCase plus `Props`: `pill …` in a `[view]` calls
`pill(PillProps::props()….build(), children)`, so the two have to be spelled that way. Properties are `#[props(…)]` attributes on the fields: `default`, `into` (accepts anything
convertible, which is how a plain string reaches a `Reactive<String>`) and `some` (stores an `Option`).

A widget that a `[view]` cannot build, because it owns a canvas or a document, can skip the struct: `#[telar::component]` on a
function of named arguments derives the props from the arguments, and an argument named `children` receives the
nested children instead of becoming a prop.

Re-export every component and its props type from `lib.rs`, so a `prelude` glob sees both:

```rust
mod pill;

pub use pill::{PillProps, pill};
```

### `@class` and `SurfaceStyle`

`@class` on a component call restyles the component's **principal surface**, the one a caller means when they
point at it: a button's box, a menu's trigger, a tooltip's bubble. The transpiler compiles the properties the
class names (`fill`, `stroke`, `stroke_width`, `radius`) into a closure over the style the component worked out
and passes it as the `style` prop. A component accepts it by

1. declaring a `style` prop of type `Option<Rc<dyn Fn(RectStyle) -> RectStyle>>`, the expansion of `SurfaceStyle`, and
2. running its finished style for that surface through `amend_surface(style, &surface)` on every state, hover and
   press included.

It takes the finished style rather than naming a property because a component resolves its surface per state,
and an amendment composes with the states where a `radius` prop would have to be threaded through every branch.
A component with no principal surface (a layout, a fragment, something that paints three boxes) declares no
`style` prop, and `@class` on it is a compile error on the author's line saying there is no `style` prop. What an
entire application should agree on belongs in a theme token, not here.

### Theme tokens

A plugin cannot name the application's theme type, so it reads the shared vocabulary instead:
`telar::use_theme_tokens()` returns the installed `ThemeTokens`, whose methods (`primary()`, `on_primary()`,
`surface()`, `border()`, `muted()`, `radius()`, `spacing()`, …) every theme answers, with built-in defaults for
the ones it leaves out. Read it inside a reactive closure, as `pill` does, so a theme switch re-colours the
component. The vocabulary is closed; a token only one plugin needs is that plugin's own, below.

### Plugin-specific tokens

A plugin declares the tokens it needs beyond the shared vocabulary as a type of its own, with its built-in
answers as `Default`, and reads it with `telar::use_theme_extension::<K>()`:

```rust
#[derive(Clone)]
pub struct ChipTokens {
    pub radius: f32,
    pub gap: f32,
}

impl Default for ChipTokens {
    fn default() -> Self {
        Self { radius: 999.0, gap: 4.0 }
    }
}

let radius = telar::use_theme_extension::<ChipTokens>().radius;
```

An application supplies it by giving its theme a field of that type marked `#[theme(extension)]`:

```rust
#[derive(Clone, telar::ThemeTokens)]
pub struct AppTheme {
    pub primary: telar::Color,
    // …the shared tokens…
    #[theme(extension)]
    pub chips: telar_chips::ChipTokens,
}
```

A hand-written theme does the same in `ThemeTokens::register_extensions`, calling `extensions.insert(value)` once
per type. The rules:

- **Absent means default.** With no theme installed, or a theme that supplies no `K`, the read is `K::default()`,
  so the plugin works in an application that has never heard of it.
- **One theme, not a merge.** The value comes from the theme in force, resolved exactly like
  `use_theme_tokens()`: the nearest `theme:`/`ScopedTheme` provider, else the global theme. A nested theme that
  supplies no `K` gives `K::default()`, not the outer theme's `K`, so a component never draws one theme's colours
  with another's metrics.
- **Reactive.** The read subscribes the caller like `use_theme_tokens()`: a mode switch, `set_theme`, or a
  provider's `set` re-runs it. Read it inside the closure that builds the style.
- **Hot reload.** Extension values are not serialised: they belong to the theme value, and the incoming library's
  `setup` re-installs that theme (and the restored mode re-applies its variant), extensions included.

A library's `.rsx` has no syntax for an extension: `$theme.x` stays the shared vocabulary. A `.rsx` library that
needs its own tokens calls `use_theme_extension` from Rust, in a `[logic]` closure or a helper function, and
reads it inside the closure that builds the style like any other theme read.

### Overridable strings

Text a plugin draws itself goes through `telar::i18n::translate_with_override`, with the crate name as the
namespace. This is the whole of `telar-components`' `strings.rs`, abridged:

```rust
use telar::i18n::{Catalog, Entry, Message, translate_with_override};

const NAMESPACE: &str = "telar_components";

static CATALOG: Catalog = Catalog {
    locales: &["en", "es"],
    default_locale: "en",
    entries: &[Entry {
        key: "close",
        messages: &[("en", Message::Plain("Close")), ("es", Message::Plain("Cerrar"))],
    }],
};

pub(crate) fn text(key: &str) -> String {
    translate_with_override(NAMESPACE, &CATALOG, key, &[])
}
```

Call it inside a reactive closure so a locale switch re-renders.

The application overrides a string by keying its own catalog under the plugin's crate name, with dashes as
underscores. In `locales/en.toml`:

```toml
[telar_components]
close = "Dismiss"
```

Resolution, first hit wins: the application's message for the active locale, the plugin's message for the
active locale, the application's default-locale message, the plugin's default-locale message, then the key
itself. An active-locale message always beats a default-locale one, so an application that overrides only its
own default language does not hide a translation the plugin already has for the reader's.

The application does nothing to make that apply. The catalog it bakes from `locales/` is installed as its binary
loads, before `main`, any test or the hot-reload factory runs, so every frontend, `cargo telar test`, a unit test
that mounts a component, and each load of a hot-reload dylib find it. `telar::app!` installs its own
unconditionally; `telar::rsx_modules!` installs the catalog of an application crate only when none is installed
yet, so the one `app!` bakes wins over a helper crate's. A `[telar] library` never installs its catalog, which
holds the strings being overridden. An application with no `locales/` installs nothing, and the plugin's own
catalog is used as it stands.

`telar::set_catalog` replaces the installed catalog at run time, for a language pack loaded after start-up. An
application that wires its own runner from several `rsx_modules!` crates that each bake a catalog names the
one it means with it too.

The notice of the icons the application baked travels the same way. `telar::app!` and `telar::rsx_modules!` compile `.telar/ICONS-LICENSES.txt` into an application crate, when the bake wrote one, and install it from a load-time constructor into the facade, so `telar_icons::licenses()` returns it on every platform, in a test and in each load of a hot-reload dylib, whose own copy of the facade it is installed into. It lists the icons of the `[telar] library` crates the application is built with too, and a library never installs one of its own. An application shows it in an "Open source licences" screen (see [`telar-icons`](../plugins/telar-icons#in-the-app)).

### A prop that names a baked asset

A built-in tag bakes a file named by path (`svg src:"logo.svg"`). A plugin's tag can have a prop the CLI bakes
too, naming its asset by id: [`telar-icons`](../plugins/telar-icons)' `icon name:"mdi:home"` is the one there is.
The protocol is general, and lives in `telar_project::ComponentAsset`: a tag, a prop, and a `[telar.<section>]`
of `telar.toml` that configures where ids resolve and whether they are baked. Where the package bakes them,
`cargo telar bake` resolves every literal the `.rsx` gives that prop and writes it into the package's artifact,
and the transpiler hands the prop the triple `("set:name", Arc<Data>, monochrome)` instead of the string, so the
props type accepts both through `From`. The id is spelled the way the bake keyed it, so a bare name written where
`[telar.icons] default_set` is set arrives read in that set, and `monochrome` is the tint decision the bake made from
the set's Iconify `palette` and the SVG's `currentColor`:

```rust
pub enum IconName {
    Baked { id: &'static str, svg: Arc<SvgData>, monochrome: bool },
    Named(Reactive<String>),
}

impl From<(&'static str, Arc<SvgData>, bool)> for IconName { /* … */ }
impl From<&'static str> for IconName { /* … */ }
```

A value that is not a literal reaches the component as written, unless the section bakes every id, where it is an
error on the `.rsx` line. What is not general is the resolver: it runs inside the CLI at bake time, and the CLI
loads no plugin code, so a new kind is an entry in `telar_project::ASSET_KINDS` and a resolver in `telar-baker`,
the way a new file format is.

## A plugin in `.rsx`

A package that sets `library = true` is a plugin written in the template language. Its `.rsx` files transpile to
Rust that ships inside the package, and an application compiles that Rust without running the CLI on it.

### Scaffold

```sh
cargo telar new --lib my-plugin     # or `cargo telar init --lib`, into the current directory
```

`--lib` cannot be combined with `--target` or `--renderer`. It writes:

```
my-plugin/
  Cargo.toml       # the `include` list and `telar = "=VERSION"`
  telar.toml       # [telar] library = true
  src/lib.rs       # telar::rsx_modules!();
  src/badge.rsx    # one component
```

and transpiles it once, so it opens in an editor without an error.

`telar.toml` is self-contained:

```toml
[telar]
library = true
prelude = []
```

A library inherits nothing from a workspace `telar.toml`, because once published there is no workspace above
it, or there is an application's. What it declares is all it is transpiled against, and it may not declare
`theme`: it cannot name the theme types of the applications that will depend on it.

### What a library's `.rsx` reads

- **`$theme.x`** is `telar::use_theme_tokens().x()`. It reads the `ThemeTokens` vocabulary described above, and
  an unknown token is an error on the `.rsx` line. An application's own `$theme.x` is its theme type; a library's
  is always the shared tokens.
- **`t!`** calls `telar::i18n::translate_with_override` with the package name, dashes as underscores, as the
  namespace, so an application overrides each string the way the previous section describes. The catalog comes
  from the package's own `locales/` directory.
- **Assets**: `svg src:"mark.svg"` is baked into the artifact like an application's, and so is an
  `icon name:"set:name"` resolved through the library's own `[telar.icons]`. A library that bakes icons also ships
  `.telar/icons-library.json`, the record of those icons and the sets and licences they come from: an application
  built with it merges the record into the licence notice it ships, judged against its own
  `[telar.icons.licenses]` (see [`telar-icons`](../plugins/telar-icons#icons-from-libraries)).
- **Components** are reached by path, as in an application. Re-export them from `lib.rs` so a consumer's
  `prelude` glob sees them:

```rust
telar::rsx_modules!();

pub use greeting_card::{GreetingCardProps, greeting_card};
```

`plugins/telar-rsx-fixture` is the complete example: a component that reads `$theme.primary`, translates through
its own catalog, draws a baked SVG and bakes an icon from an Iconify set of its own, packaged, unpacked read-only
and compiled into a generated application by `cargo test -p cargo-telar --test rsx_library -- --ignored`, whose
notice, and `telar_icons::licenses()` in the compiled application, have to credit the library's icon.

```rsx
[logic]
#[derive(Default)]
pub struct Props {
    pub name: &'static str,
}

[view]
row gap:8 pad:12 align:center fill:$theme.primary radius:$theme.radius
    svg src:"mark.svg" width:24 height:24
    icon name:"fixture:dot" size:16
    text "{t!(\"greeting\", name = props.name)}" font_size:14 color:$theme.on_primary
```

An application then adds the library and names it in its `prelude`:

```toml
# Cargo.toml
my-plugin = "0.1.0"

# telar.toml
[telar]
prelude = ["telar_components", "my_plugin"]
```

### Package and publish

```sh
cargo telar package --check      # verify only; exits non-zero when the library is not ready
cargo telar package              # transpile, check, then `cargo package`
cargo telar publish --dry-run    # every check, and cargo's own, without uploading
cargo telar publish
```

Each takes `-p <name>` for one library, or `--workspace` for every workspace member that may be published, plain
crates and libraries alike; the default is the package in the current directory. Arguments after `--` go to cargo
(`cargo telar publish -- --registry my-registry`). `--workspace` leaves out a member whose `Cargo.toml` says
`publish = false`, and `publish -p` refuses one.

`--workspace` ships everything in one cargo call. Cargo verifies each package of a call against the other packages
of that same call and takes every other dependency from the registry, so a plain crate that depends on a library,
or a library that depends on a plain crate, only builds against the in-tree version of the other when both are in
the call. Cargo orders them by their dependencies on each other.

When a library is among what they ship, both transpile and bake first, then check that each library's package
carries the whole artifact and that the artifact answers for the sources on disk. Cargo leaves every
dot-directory out of a package unless `include` names it, and `.telar/` is one, so a library on cargo's defaults publishes a crate no build can compile. The `include`
list is the exact one `cargo telar new --lib` writes: `telar.toml`, `src/**` minus the module trees of the
hot-reload and preview flavours, and the Plain flavour's generated directory, index, baked catalog, baked
assets and icon record under `.telar/`. The icon record also has to answer for the icons the artifact baked. `package --check` names what to add when the list falls behind. `telar_project::library_include()` is where the list is defined.

Cargo refuses a package with uncommitted files, and the gitignored `.telar/` is exactly that, so `package` and
`publish` call cargo with `--allow-dirty` after checking that no packaged source git tracks has uncommitted
changes, in every crate they ship, plain or library. A library's artifact is exempt, since every transpile
regenerates it.

### No `build.rs`, and the git-dependency policy

Nothing transpiles at a consumer's build: the Plain artifact is compiled as shipped. A library is therefore
consumed from crates.io, from the same workspace, or by path. A **git dependency** works only if the author has
committed the Plain artifact, which the scaffold's `.gitignore` (`.telar/`) hides. Either way, a build on a
checkout whose artifact is missing or stale fails with a message naming whoever packages the library, not the
consumer.

### What the macro does, and does not do

`telar::rsx_modules!()` wires what `cargo telar transpile` produced and **never writes**, in any mode. The CLI
owns the module tree: it writes every flavour, the tree with relative `#[path]`s and all cleanup, and the macro
refuses an artifact or module tree that disagrees with `src/`. So adding, moving or removing a hand-written
`.rs` module needs `cargo telar transpile` before the next build (`cargo telar check`, `dev`, `build` and `test`
run it for you); a plain `cargo build` is refused with a message naming the command.

### The `CARGO_PRIMARY_PACKAGE` caveat

A library compiled as a dependency always gets the Plain flavour, whatever `hot-reload` or `preview` asked
for, because that is the only artifact a published library carries. Cargo sets `CARGO_PRIMARY_PACKAGE` for the
package the command selected (`-p`, `--workspace`, the current directory or a default member) and leaves it
unset for every dependency, including a workspace member that another member pulls in; that is how the macro
tells the two apart. Cargo does not fingerprint that variable, so a workspace library built once selected and
once as a dependency, with the same features, keeps whichever expansion ran first until one of its sources
changes. `cargo telar transpile` writes every flavour for a workspace member, so the practical effect is a
stale build to clear by editing a source, not a wrong one.

## Hot reload: what a plugin must not leave behind

`cargo telar dev` builds the application as a `cdylib` and swaps it on every change. A plugin is linked into
that dylib, so two rules follow from the old copy being unmapped (`dlclose`):

- **Thread-locals must not register destructors.** A `thread_local!` holding something that drops (an `Rc`, a
  `RefCell<HashMap<…>>`) leaves a TLS destructor pointing into code that is no longer mapped. Wrap it in
  `ManuallyDrop`; the table is leaked at thread exit instead. `telar-theme-core` does this for its mode table
  and its light/dark pair (`crates/ui/theme-core/src/mode.rs`):

  ```rust
  thread_local! {
      static MODES: ManuallyDrop<RefCell<HashMap<String, ApplyMode>>> =
          ManuallyDrop::new(RefCell::new(HashMap::new()));
  }
  ```

  Anything a plugin keeps that must survive a swap is bridged through the hot-reload snapshot, as the active
  theme mode id is, rather than kept in a plugin-owned static.

- **A `&Catalog` from a dylib is only valid while the dylib is loaded.** A catalog baked into the application's
  dylib lives in that dylib's data. The installed catalog is safe: the dylib links its own copy of the i18n
  runtime, and every load installs the catalog it carries into that copy before anything in it runs, so the two
  are unmapped together (`crates/i18n/i18n-core/src/installed.rs`). A plugin's own `static CATALOG`, as above, is
  safe because the plugin names it on every `translate_with_override` call rather than storing the reference; do
  not store it in a static of your own.

- **A process-wide install reaches only the image that made it.** A slot such as
  `telar::i18n::set_plural_rules` belongs to the copy of the runtime it is called in, and the dylib has its own,
  so rules a host installs from `main` never reach the dylib's widgets. A plugin that installs something exposes
  a function the application calls from `telar::app!`'s setup block, which runs in the host and again on every
  load of the dylib, as [`telar-i18n`](../plugins/telar-i18n)'s `install()` is. What it installs is a `&'static`
  item of the plugin's own, compiled into the same image as the slot, so the two are unmapped together.

## Checklist

- Depends on `telar` by that name, at the exact version it is released with; documented as such.
- Components re-exported from `lib.rs` with their `Props` types; the crate docs show the `prelude` line.
- A `style` prop run through `amend_surface` on the principal surface, where there is one.
- Colours, radii and spacing from `use_theme_tokens()`, read in a reactive closure; tokens only this plugin
  needs from `use_theme_extension::<K>()`, with `K::default()` as the built-in answers.
- Strings through `translate_with_override` (or `t!` in a library) under the crate name.
- No thread-local with a destructor, no stored `&'static Catalog`.
- Anything installed process-wide behind a function the application calls from `telar::app!`'s setup block.
- For a `.rsx` library: `cargo telar package --check` passes, and `cargo telar publish --dry-run` succeeds.
