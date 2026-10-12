# Previews

A preview is one component in one state, rendered on a canvas of its own with no application around it. The same
preview is shown in the component workshop, played and checked by `cargo telar test`, and snapshotted: write it
once and all three read it.

```sh
cargo telar preview              # open the workshop on every preview, or bring the open one forward
cargo telar preview <ID>         # open it on one preview, or move the open workshop there
cargo telar preview --list       # print every preview's id and exit
cargo telar preview --png out/   # render each preview and matrix cell to a PNG, with no window
cargo telar preview --target web # serve the workshop on http://localhost:8080 (or a free port), rebuilt on every change
cargo telar preview --build site/ --target web # export the workshop as a static site
cargo telar test                 # build, play, accessibility-check and snapshot every preview, headlessly
```

Previews exist only under the `telar/previews` feature, which every preview command turns on. A build without it
compiles none of them, so a shipping binary carries no preview code.

## Previews in `.rsx`

A `[previews]` section names the component's page in the workshop, and a `[preview]` section is one variant of it:

```rsx
[previews "Forms/Checkbox" layout:centered matrix:themes]
Use `toggle` for a setting that applies at once, a checkbox for one confirmed with a button.

[preview "Default"]
checkbox label:"I agree to the terms"

[preview "Bound" args(agree:false)]
col gap:8
    checkbox checked:$agree label:"I agree"
    text "agree · {$agree}"

[play]
canvas.click(by_role(Role::CheckBox).named("I agree"))?;
canvas.expect_text("agree · true")?;
```

- **`[previews "Group/Title" …]`** is optional and comes once per file. The title is the path the previews are
  listed under in the sidebar; it defaults to the component's name. The prose under it is the top of the
  component's Docs page, and its header options are inherited by every `[preview]` in the file. A preview's own
  option wins, and `tags` add up.
- **`[preview "Name" …]`** is a variant. Its body is a node tree, as in `[view]`. A file may hold any number of them.
- **`[play]`** follows a preview and holds verbatim Rust that drives it; see [Play](#play).
- A `.rsx` file written for a component keeps its previews beside its `[view]`. A **`checkbox.previews.rsx`** file
  holds previews alone, with no `[view]`, for the component named by its file stem: use it when the component is
  written in Rust, or when the previews would drown the component.

Header options are `key:value`, on `[previews]` and on `[preview]` alike. A misspelled one is a build error on its
own line, which lists the keys it takes.

| Option | Value | Sets |
| --- | --- | --- |
| `layout` | `padded`, `centered`, `fullscreen` | How the canvas places the preview: top-start with padding (the default), centred at its own size, or edge to edge with no frame |
| `viewport` | `390x844` | The canvas size, in logical px |
| `bg` | `#202020` | The canvas background |
| `mode` | a registered mode's id | The theme mode the preview opens in |
| `locale` | a BCP 47 tag | The language the preview opens in |
| `dir` | `ltr`, `rtl` | The writing direction, whatever the locale's own |
| `tags` | `[stateful slow]` | Free-form labels the workshop filters by |
| `matrix` | `themes` or `(…)` | The cells the preview is also rendered across; see [Matrices](#matrices) |
| `decorator` | a Rust path | A fn wrapping the preview's root, for a provider or a frame every variant needs |
| `fixture` | a Rust path | A fn called before the preview is built, to seed state |
| `surface`, `animate` | `360x240`, a bare flag | Mount the preview as a window of that size, playing its enter transition with `animate`, instead of as a tree |
| `args` | `none` | Keep the root's literal attributes fixed; see [Args](#args-and-controls) |

The editor completes these keys and explains each on hover.

## Previews in Rust

A component written in Rust previews itself with `preview!`, which returns a `Preview` to configure with setters.
Write previews, and the fn that lists a crate's, inside `telar::__previews!`, which expands to nothing unless
`telar/previews` is on:

```rust
use telar::Role;
use telar::preview::{Layout, Preview, by_role, preview};

telar::__previews! {
    pub fn telar_all_previews() -> Vec<Preview> {
        vec![
            preview!(button: ButtonProps, "Primary", |p| {
                button(
                    ButtonProps::props()
                        .label(p.arg("label", "Save"))
                        .ghost(p.arg("ghost", false))
                        .build(),
                    Children::default(),
                )
            })
            .title("Inputs/Button")
            .layout(Layout::Centered),
            preview!(button: ButtonProps, "Counting presses", |p| {
                let pressed = p.signal("pressed", 0u32);
                button(
                    ButtonProps::props()
                        .label("Press me")
                        .on_press(Rc::new(move || pressed.update(|n| *n += 1)))
                        .build(),
                    Children::default(),
                )
            })
            .play(|canvas| {
                canvas.click(by_role(Role::Button).named("Press me"))?;
                canvas.expect_action("on_press", 1)
            }),
        ]
    }
}
```

- **`button: ButtonProps`** names the component under preview and its props type. The props feed the Docs page and
  refine the control of each arg named after a prop. The type can be left out for a component with no derived
  props. When it is written, every callback the body hands to `button` is logged in the Actions panel.
- **`"Primary"`** names the variant and, slugged, ends its [id](#ids). A name with no letter or digit is a compile
  error.
- **`|p| body`** builds the tree and returns what a component call returns, `Result<impl LayoutItem, LayoutError>`.
  It is compiled as a fn, so it cannot capture anything.
- **Setters** follow the `.rsx` header options: `title`, `docs`, `layout`, `viewport`, `bg`, `mode`, `locale`,
  `dir`, `tags`, `decorate`, `surface`, `matrix` and `play`, plus `a11y_ignore` (see
  [Accessibility checks](#accessibility-checks)).

A crate that writes previews in Rust and also has `.rsx` previews lists both from one `telar_all_previews`, with the
`.rsx` ones through `telar_rsx_previews()`:

```rust
telar::__previews! {
    mod previews;

    pub fn telar_all_previews() -> Vec<telar::preview::Preview> {
        [telar_rsx_previews(), previews::previews()].concat()
    }
}
```

A crate with `.rsx` previews only needs nothing: `telar::app!` and `telar::rsx_modules!` define
`telar_all_previews` for it.

## Args and controls

An arg is a value of a preview the workshop can change while the preview is on screen. There are two kinds:

- **Plain args** are read when the preview is built: `p.arg("label", "Save")` in Rust, and in `.rsx` every literal
  attribute of the root component call (a string, a number, a bool or flag, a hex colour, a `Path::Variant`). They
  are declared with no extra syntax, and changing one builds the preview again. A value containing `$` or a closure
  stays fixed, and `args:none` in the header fixes all of them.
- **Signals** are shared with the tree: `p.signal("pressed", 0u32)` in Rust and `args(agree:false)` in `.rsx`,
  read as `$agree`. They are two-way, so the tree can write what the control shows, and changing one does not
  rebuild the preview.

`p.arg` returns the default's own type, so a setter's `into` coerces it as a literal would. Setting a name both in
`args(…)` and as a literal on the root is a build error: bind the root to the signal with `agree:$agree`.

The type of an arg decides its control, and the prop of the same name refines it:

| Type | Control |
| --- | --- |
| `bool` | A toggle |
| Numbers | A number field, or a slider with `#[props(control = range(0, 1))]` |
| `&'static str` (leaked when overridden, in preview builds only), `String`, `Reactive<String>` | A text field; `control = multiline` grows it |
| `Color`, `Reactive<Color>` | A colour picker |
| An enum with `#[derive(PreviewArg)]` | A menu, or segments for four variants or fewer |
| `Option<T>` | `T`'s control, with a way to leave it unset |
| `RwSignal<T>` | `T`'s control, two-way |
| Callbacks | Not a control: each call is logged in the Actions panel |
| `Children`, slots, `t"…"` | Not a control |

A type without `PreviewArg` is shown read-only. `#[props(control = …)]` on a prop takes `range(min, max)`,
`step(n)`, `multiline`, chained as in `range(0, 1).step(0.1)`, or `read_only`. A control that does not fit its prop's
type leaves the control as it was.

Edited args live in memory for the run. They survive a hot reload, not a restart, and the workshop's address carries
them.

## Matrices

A matrix renders one preview once per combination of values, to check a look across themes, languages and sizes.
Write it in the header, either inline:

```rsx
[preview "Sizes" matrix:(mode:[light dark] locale:[en ar] viewport:[390x844 1280x800] size:[12 16])]
```

or by name, from `telar.toml`:

```toml
[telar.previews.matrices]
themes = { mode = ["light", "dark"] }
rtl    = { dir = ["ltr", "rtl"], locale = ["en", "ar"] }

[telar.previews.viewports]
phone  = "390x844"
laptop = "1280x800"
```

```rsx
[preview "Default" matrix:rtl]
```

- **Environment axes** are `mode`, `locale`, `dir`, `viewport` and `control_size` (`mini`, `small`, `regular`,
  `large`). A viewport value is a size such as `390x844` or a name from `[telar.previews.viewports]`.
- **Any other key names an arg** of the preview, declared in `args(…)` or written as a literal on the root, and
  takes values in their text form: `12`, `true`, `Primary`, `#ff8800`, `"Save"`.
- The cells are the product of the axes, the first outermost. `themes` is built in: every registered mode, side by
  side. A package's own `themes` replaces it.
- In Rust, `.matrix(Matrix::Named("themes"))` or `.matrix(Matrix::Axes(&[Axis::Dir(&[…]), Axis::Arg("size", &["12", "16"])]))`.

`cargo telar test` and the snapshots treat each cell as a preview of its own. The workshop's Matrix view lays the
cells out in a grid, and builds one from the axes you pick for a preview that has no matrix.

## Ids

A preview's id is `<crate>--<component>--<slug(name)>`, where the slug keeps letters and digits, lowercased, and turns
every other run of characters into one `-`: `"Long body"` is `long-body`. A matrix cell appends `--<axis>=<value>` for
each axis, in order. Two previews of one crate with the same id fail the test run.

Ids are stable across builds, so they name the preview on the command line (`cargo telar preview <ID>`,
`cargo telar test --preview '<glob>'`), in workshop links, in the editor, and in the snapshot files. `cargo telar
preview --list` prints them.

## Play

A play drives the mounted preview as a person would, then asserts on what it left. In `.rsx` it is a `[play]` zone
under its preview, and in Rust `.play(|canvas| …)`. The body is Rust, `canvas` is a `telar::preview::Play`, every
step returns a `Result`, and `?` ends the play at the first one that fails. `Role`, `by_role` and `by_text` are in
scope in a `[play]` zone, and an `Ok(())` is added at its end.

| Does | Steps |
| --- | --- |
| Acts | `click`, `hover`, `drag`, `type_text`, `press`, `tab`, `tab_back` |
| Waits | `settle`, `advance` |
| Asserts | `expect`, `expect_absent`, `expect_text`, `expect_no_text`, `expect_focused`, `expect_toggled`, `expect_expanded`, `expect_value`, `expect_action`, `expect_accessible` |

A step finds its target in the accessibility tree, as a screen reader would: `by_role(Role::Button).named("Save")`,
`by_text("Sign in")`, with `.containing(…)` and `.nth(i)` to narrow a match. A control without a name or a role
cannot be found, which is the point. Each step dispatches the events a person's input would and settles the frame
before it returns.

The workshop runs a play on the canvas and lists its steps in the Interactions panel; a failed step is red, with
the frame it left. `cargo telar test` runs it on a headless canvas.

## Accessibility checks

Each preview and cell is checked for what would keep someone from using it, by rule: `unnamed-control`,
`symbol-only-name`, `unnamed-picture`, `contrast`, `hidden-focusable` (errors), and `target-size`, `focus-order`,
`missing-lang` (warnings). The workshop's Accessibility panel shows the findings of the canvas on screen, and
`expect_accessible()` in a play asserts there are none.

A preview that breaks a rule on purpose, such as a dense layout with small targets, says so, and the checks leave
that rule out: `.a11y_ignore(&[Rule::TargetSize])` in Rust.

## Snapshots

A snapshot is two files per preview and per matrix cell, kept under the package's `previews/__snapshots__/`:

| File | Is | Compared |
| --- | --- | --- |
| `<id>.png` | The preview drawn on the CPU rasterizer | Pixel by pixel, within a tolerance |
| `<id>.draw.txt` | The draw commands behind it, one per line | Exactly, so a pull request shows a padding change as a rect that moved |

Text is set only in the fonts the package declares under `[[telar.fonts]]`, so the files are the same on every
machine; declare a font for a project that snapshots text. Commit both files.

`cargo telar test` compares and never writes over a golden. What does not match is written to
`target/telar-snapshots/`: the picture and text captured, a diff picture and a line diff. A preview with no golden
fails like one that drifted, until the goldens are written:

```sh
cargo telar test --update-snapshots
```

In the workshop, the **Snapshot** panel shows what the last `cargo telar test` found for the selected preview, or for
the focused matrix cell: the golden and the picture captured side by side, one over the other or swiped, and the line
diff of the draw text. **Accept** writes both files, as `--update-snapshots` does, and a preview with a failing
snapshot carries a badge in the sidebar. The panel reads `target/telar-snapshots/report.json`, so run
`cargo telar test` first.

The tolerance defaults to two steps of a channel and no differing pixel, because the same rasterizer and fonts draw
the same picture. Loosen it in `telar.toml`:

```toml
[telar.previews.snapshots]
channel_tolerance = 2        # 0-255, the most one channel of a pixel may move
max_differing_pixels = 0     # how many pixels may differ and the picture still match
```

## `cargo telar test`

For each preview, and each cell of its matrix, the run builds and lays it out, plays it, checks it for
accessibility, and compares its snapshots. It prints a line for each and exits non-zero when any failed.

| Flag | Does |
| --- | --- |
| `--preview <GLOB>` | Test only the previews, or cells, whose id matches (`*` any run of characters, `?` one) |
| `--a11y off\|warn\|error` | `off` checks nothing, `warn` (the default) reports findings and fails nothing, `error` fails a preview on an error-level finding |
| `--update-snapshots` | Write the goldens that are missing or no longer match, instead of failing on them |
| `--no-snapshots` | Take no snapshots |
| `--report <json\|junit> <PATH>` | Also write the run's report to a path, as `report.json` or JUnit XML for a CI |

`-p`, `-F`, `--target` and `--release` work as they do for the other commands. `--target web` and `--target
android` are refused: the run renders on the host. The run always writes `target/telar-snapshots/report.json`.

A library has no binary to run. `rsx_modules!` gives it a unit test, `telar_previews`, in preview builds, and
`cargo telar test -p <library>` runs it through `cargo test`. The first-party components are tested this way
through `apps/catalogue`: `cargo telar test -p catalogue`.

### Auditing the exported workshop

`cargo telar test` checks the frame a preview draws. What a browser makes of the document the web target writes
is checked separately, on the [exported workshop](#exporting-the-workshop): an ignored test in `cargo-telar`
serves the export on a loopback port, opens every `preview/<id>` and `matrix/<id>` page in headless Chrome with
`?chrome=0`, and runs [axe-core](https://github.com/dequelabs/axe-core) over the canvas. It also audits the
workshop's own chrome once, as a whole page, so its landmarks are checked too. The rules are WCAG 2.2 A/AA plus
best practices, read from `crates/renderer/renderer-dom/axe.json`, the same file
`plugins/telar-components/src/audit_test.rs` uses; `region` is off for the canvas, which is a fragment of a page,
and on for the chrome. CI's `workshop-audit` job runs it on `apps/catalogue` and keeps the export as the
`catalogue-site` artifact. Inside the dev shell, which provides Chrome, `CHROMEDRIVER` and `TELAR_AXE_CORE`, run:

```sh
cargo telar preview -p catalogue --build target/tmp/catalogue-export --target web
TELAR_WORKSHOP_SITE=target/tmp/catalogue-export \
  cargo test -p cargo-telar --test workshop_audit -- --ignored --nocapture
```

A relative `TELAR_WORKSHOP_SITE` is read from the repository root. Set `TELAR_WORKSHOP_AUDIT_ONLY` to audit only
the pages whose address contains it, such as `button--default`. A failure lists each page with every rule it
breaks, the elements that break it and why. A page that never settles is reported too, with what its console
logged.

The audit waits for each page to settle before it looks. The browser platform keeps a `data-telar-idle`
attribute on the app's host while its loop has no frame coming, and the audit waits until that attribute has
stayed there for a moment and every font has loaded. A browser test of your own can wait the same way. The
browser is asked for reduced motion, which stops looping animations, so an animation that keeps running anyway
fails the audit.

## Previews across crates

An application lists the previews of the crates it depends on after its own, in `telar.toml`:

```toml
[telar.previews]
include = ["telar-components", "my-plugin"]
```

For each entry, `telar::app!` calls `::<crate>::telar_all_previews()`. An entry that is not a dependency of the
package, or is the package itself, is an error on its line, and a crate without the fn fails the build naming the
crate. There is no automatic registry: what is listed is what is shown.

A `.rsx` library carries its previews in the one artifact it ships, since a library compiled as a dependency always
gets the Plain artifact. They compile to nothing unless `telar/previews` is on, so a library's consumers pay for
them only in preview and test builds. See [Shipping previews](plugins.md#shipping-previews).

The first-party suite is the previews of `telar-components`, shown through `apps/catalogue`, an ordinary app that
includes them: `cargo telar preview -p catalogue`.

## The workshop

`cargo telar preview` opens the workshop, provided by [`telar-workshop`](../plugins/telar-workshop), an optional
dependency of your package that `cargo telar new` writes. Without it the command falls back to a plain page that
lists every preview, and says how to add it.

- **Top bar.** The project's name, a search trigger (`/` or Ctrl/Cmd+K), the Canvas, Matrix and Docs views, and the
  environment every canvas is shown in: mode, locale, direction, control size, background, high contrast, reduced
  motion. Below 1280 px the canvas settings fold into one menu.
- **Sidebar.** Search over the tree of group, component and preview, with a badge on a preview whose snapshot
  failed. Below 900 px it becomes a drawer.
- **Canvas.** One preview, mounted alone on a surface of its own, with its size, device frame, zoom, rulers, grid,
  outline and measure.
- **Panels,** along the bottom or the right: Controls, Actions, Interactions, Layout, Accessibility, Snapshot.

Its chords hold Alt, so they never reach a control inside a preview and never meet the devtools' Ctrl+Shift ones.
None fires while a text field has the caret.

| Keys | Action |
| --- | --- |
| `/`, Ctrl/Cmd+K | Command palette: every preview and command |
| `?` | Shortcuts |
| Alt+↑ / ↓ | Previous / next preview |
| Alt+← / → | Previous / next component |
| Alt+1 / 2 / 3 | Canvas / Matrix / Docs |
| Alt+S / A / D | Sidebar / panels / panel position |
| Alt+F | Fullscreen canvas |
| Alt+T / L / R | Cycle mode / locale / direction |
| Alt+0 / = / - | Zoom reset / in / out |
| Alt+G / Alt+Shift+G | Grid / rulers |
| Alt+M / O | Measure / outline |
| Alt+Shift+R | Remount |

A hot reload keeps everything the workshop shows. A restart reopens on the same preview and view, with the same pane
sizes and canvas settings, which are kept in `<workspace>/.telar/workshop/state`. Edited args and a preview's own
preferences are not stored. **Copy link** in the canvas header copies a link that carries the preview, its edited
args and the canvas settings; `cargo telar preview '<link>'` opens it again.

`cargo telar preview --target web` serves the workshop as a page instead, and reloads it after every rebuild. The
page keeps the same state in the browser's `localStorage`, and its address carries the preview, the view, the edited
args and the Matrix view's axes and focused cell, so a reload, or the address pasted elsewhere, comes back to the
same place. Run again while the page is served, it prints the page's address, and the preview's when given one,
instead of building a second; run while the first is still building, it says so and leaves the build to it.

### Exporting the workshop

`cargo telar preview --build <DIR> --target web` writes the workshop as a static site, for any host that serves files:
the first-party one is `cargo telar preview -p catalogue --build site/ --target web`.

- **The build** is the page `--target web` serves, built for release like `cargo telar build --target web` (it needs
  the project's `[profile.web]`), into `<DIR>`. The directory is replaced whole; one holding anything but an earlier
  web build is refused rather than deleted.
- **Every address is a file.** The previews are listed first, by running the package once on this machine as
  `--list` does. The page is then written again at `preview/<id>/` and `matrix/<id>/` for each preview and
  `docs/<group>/<title>/` for each title, and as `404.html`, so opening any of those addresses afresh, a reload
  included, loads the workshop there. The query a link carries (`args`, `axes`, `cell`, `globals`, `chrome=0`) needs
  no page of its own. See [The workshop as a site](web-packaging.md#the-workshop-as-a-site).
- **`--base <PATH>`** serves the site under a path, in place of `[telar.web] base`: the files are written for it, and
  the workshop reads its addresses from below it. The rest of `[telar.web]`, the host profile included, applies as
  to any web build.
- `--target` may be left out, since a site is only ever for the browser; any other target is refused. `-p`, `-F` and
  `--renderer` work as they do for the other commands.

The exported workshop keeps its state in the browser's `localStorage`, per site, as the served one does.

[`plugins/telar-workshop`](../plugins/telar-workshop/README.md) describes each view and panel in full.

## In the editor

The [VS Code extension](../editors/vscode/telar/README.md) works on previews from the `.rsx` file:

- A **▶ Open in workshop** lens above each `[preview]`, and the command **Telar: Show preview at cursor in
  workshop**, move the running workshop that lists the preview to it, or start one on it. That is the package's own
  workshop, or for a library, the workshop of an application in the workspace whose `[telar.previews] include`
  names it.
- A **▶ Show in panel** lens, and the command **Telar: Show preview at cursor in panel**, show the preview's canvas
  in a panel beside the editor, which follows the cursor from preview to preview and reloads with every rebuild. The
  panel is the page `cargo telar preview --target web` serves, which it starts when none is running.
- **Telar: Open workshop** brings that workshop forward, or starts it.
- `telar.workshop.followCursor`, off by default, moves the running workshop window to each preview the cursor
  enters. It never starts a workshop.
- A **▶ Run play** lens above each `[play]` runs `cargo telar test --preview <id> --no-snapshots` in a terminal in
  the package, so a play can be run without a golden being compared.
- Completion and hover for the header options, `args(…)` and the names of the matrices `telar.toml` defines.
