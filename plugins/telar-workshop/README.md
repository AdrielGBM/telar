# telar-workshop

The component workshop `cargo telar preview` opens: every preview an application carries, listed in a sidebar, rendered one at a time in a canvas of its own, with panels to edit its args live.

Part of [Telar](https://github.com/AdrielGBM/telar), a modular Rust UI framework with its own template
language, reactive signals and a self-contained renderer.

An application declares it as an optional dependency, beside `telar-devtools`, which `cargo telar new` writes for you:

```sh
cargo add telar-workshop --optional
```

`cargo telar preview` turns that feature on, and `telar::app!` starts `WorkshopApp` only under it, so the workshop is in a preview build and in nothing else. Without the dependency, `cargo telar preview` falls back to the plain preview page.

The workshop runs in the same runtime as the previews it shows, so a hot reload keeps the selected preview, its args, the pane sizes and the canvas toolbar's settings. Each preview renders in a canvas with a surface of its own: its overlays, focus and size stay inside the canvas, and it draws in the application's theme while the workshop's own chrome draws in the workbench theme `telar-devtools` defines.

`telar` itself never depends on this crate. It reads `telar::preview` and builds its chrome from `telar-components`, the same public API an application's own code uses.

## Opening a preview

Writing previews, their args, matrices, plays and snapshots are covered in [docs/previews.md](../../docs/previews.md).

`cargo telar preview <ID>` opens on the preview of that id, as `cargo telar preview --list` prints it (`telar_components--button--primary`). With the package's workshop already open, the command moves that window to the preview instead of opening another. `--component <name>` narrows the sidebar to the previews whose title matches, starting the search with it.

Each page of the workshop has an address: `/preview/<id>` and `/docs/<title>`, such as `/docs/Inputs/Button`. **Copy link**, in the canvas header, copies the address of the preview shown with its edited args and the canvas toolbar's settings, as `/preview/<id>?args=label:"Save";disabled:true&globals=mode:dark;locale:ar`. `cargo telar preview '<link>'` opens it again, args and settings included. `?chrome=0` shows the canvas alone, with no sidebar, top bar or panels, for embedding.

## What it remembers

A hot reload keeps everything the workshop shows. A restart under `cargo telar preview` reopens on the same preview and view, with the same pane sizes, canvas toolbar settings and choice of whether plays run on mount, which the workshop keeps in `<workspace>/.telar/workshop/state`. Edited args start again at their defaults: they last the run, or travel in a copied link. The previews' own preferences, such as a scheme a preview stores, stay in memory.

The preview it opens on is the one a link or `<ID>` names, else the one it showed when it last closed, else the first; with `--component`, the last two are chosen among the previews the filter keeps.

## The canvas toolbar

The top bar sets the environment every canvas is shown in: its mode, from the modes the application registers, its locale, writing direction, control size, background and high contrast, which reaches the canvases and leaves the workshop around them as it is. It also switches reduced motion, which is one switch for the whole application. The canvas header sets the canvas's size: the preview's own, the whole stage, a preset, one of the package's `[telar.previews.viewports]`, a device frame with its safe area, or a custom size from the resize handles. It also rotates the canvas and zooms it. A canvas larger than the stage is zoomed to fit unless a zoom is chosen.

A setting the toolbar leaves unset takes what the preview's own environment asks for. A locale also sets the canvas's writing direction, unless a direction is chosen. Only the canvas changes: the workshop's chrome keeps its theme, its locale and its direction.

## Panels

The panels beside the canvas show one tab at a time: Controls, Actions, Interactions, Layout, Accessibility and Snapshot. **Controls** lists every arg of the selected preview with an editor for its value, its type, its default and its doc. **Actions** lists the calls the preview's callbacks received, newest first, each with the `Debug` text of its arguments; a filter keeps the calls whose name or arguments contain it, and Clear empties the log. The Actions tab counts the calls since the log was last emptied, whichever tab is shown, and the log starts empty each time a preview mounts. **Interactions** lists the steps the preview's play took on the canvas, in order, each passed, failed with why, or not run yet; the play runs on the canvas itself once it has mounted, so the canvas is left where the play left it. Picking a step shows the frame it left in place of the live canvas, read-only, until **Back to live** or Escape; the arrow keys, Home and End move through the steps. A failed step is red, with its frame beside it. **Replay** mounts the preview afresh and runs its play again, and **Run on mount**, kept between runs, decides whether a play runs as its canvas mounts. **Layout** and **Accessibility** are the devtools' inspection panels over the canvas: the components it mounted with the selected one's box model, an outline and measurements drawn over the canvas, and what the accessibility checks found with what a screen reader is told. They show what the devtools overlay shows of the same canvas.

## Snapshot

The **Snapshot** panel shows what the last `cargo telar test` found of the selected preview's snapshot, or of the focused matrix cell's, against its golden: the golden and the picture the run captured side by side, one over the other with an opacity, or swiped between, and the line diff of the draw text. **Accept** writes both files, as `cargo telar test --update-snapshots` does, and **Reload report** reads the run again. The panel reads `target/telar-snapshots/report.json`, so it has nothing to show until `cargo telar test` has run; its tab counts the selected preview's failing snapshots, and the sidebar badges each preview whose snapshot failed.

## Matrix

The Matrix view (Alt+2) shows the selected preview once per cell of its matrix, side by side under a heading for each axis: the last axis across, the others down. A preview with no matrix, or one switched to **Custom axes**, gets a matrix built from the axes picked in the view's header: mode, locale, direction, control size, viewport, and each arg with a fixed set of values. Each cell mounts the first time it scrolls into view, on a canvas of its own in the cell's environment over the toolbar's, runs the preview's play there and checks what it left; its badges say how the play and the accessibility checks came out, and how its snapshot did in the last `cargo telar test`. The view mounts at most 64 cells and says how many it left out, and warns past 48.

Pressing a cell focuses it: the Layout and Accessibility panels, and the outline (Alt+O) and measure (Alt+M), follow its canvas. **Open in canvas** shows the preview on the Canvas view with the toolbar and args set to the focused cell's.

## Docs

The Docs view shows a page for the selected preview's component: the doc of its props, its first preview with that preview's controls and source, a table of its props with their types, defaults and docs, and every other preview of the component over its source. Each of those previews mounts the first time it scrolls into view, on a canvas as wide as the page and as tall as what it draws, in the environment the toolbar sets. Their callbacks log in the Actions panel.

## Strings

The workshop's own text is in English, under the `telar_workshop` namespace. An application translates or overrides any of it by adding `telar_workshop.<key>` to its own catalog.

## License

Licensed under either of [Apache License, Version 2.0](../../LICENSE-APACHE) or [MIT license](../../LICENSE-MIT) at your option.
