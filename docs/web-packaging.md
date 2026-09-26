# Web packaging

`cargo telar build --target web` and `cargo telar dev --target web` write a static site to
`telar-dist/web/` under the workspace's target directory — `target/telar-dist/web/` unless that's been
moved with `--target-dir`, `CARGO_TARGET_DIR`, or `build.target-dir` in `.cargo/config.toml`, which cargo-telar
resolves the same way cargo does. This page covers what goes into it: the page template, the public
directory, content-hashed assets with their manifest, and what the dev server serves.

Everything here only means something to a browser, so it lives in the packaging and in `[telar.web]` in
`telar.toml`, never in `.rsx`.

## Configuration

```toml
[telar.web]
template = "web/index.html"   # the page template; default shown
public = "web/public"         # copied verbatim into the output; default shown
```

Both paths are relative to the package root and inherited from a workspace `telar.toml` key by key, like
the rest of `[telar]`. A project with no `[telar.web]` gets the defaults, and a default that does not exist
is fine: no template means the built-in page, no public directory means nothing to copy. A path you **name**
that does not exist is an error. So is a misspelled key, as everywhere else in `telar.toml`.

## Output

| File | What it is |
| --- | --- |
| `index.html` | The page, expanded from the template. |
| `app-<hash>.js` | The `wasm-bindgen` glue, pointed at the hashed module. |
| `app_bg-<hash>.wasm` | The module. |
| `asset-manifest.json` | Logical path → hashed path for every hashed file. |
| everything from `web/public/` | Copied as is, at the same relative path. |
| `*.br`, `*.gz` | Release builds only: precompressed copies (see below). |

The output directory is assembled beside `web/` and swapped in whole, so it only ever holds one build:
nothing from a previous build is left behind, and a server reading it mid-build sees the last complete one.

## The page template

The template is plain HTML with `%telar.<name>%` markers. Without one, the build uses the built-in page,
which is itself a template (`crates/tools/cargo-telar/src/runner/package/web/default_page.html`) and a good
starting point to copy into `web/index.html`.

| Marker | Kind | Expands to | Default |
| --- | --- | --- | --- |
| `%telar.lang%` | value | the document language, escaped | `en` |
| `%telar.dir%` | value | `ltr` or `rtl` | `ltr` |
| `%telar.title%` | value | the page title, escaped | the package name |
| `%telar.renderer%` | value | the `--renderer` choice, for `data-telar-renderer` | `auto` |
| `%telar.meta%` | block | `<meta>`/`<link>` tags for the document | a `description` |
| `%telar.fonts%` | block | font preloads and `@font-face` rules | nothing |
| `%telar.bootstrap%` | block | the preloads and the module script that start the app | always present |
| `%telar.prerendered%` | block | prerendered markup for the host element | nothing |
| `%telar.state%` | block | `<script type="application/json" id="telar-state">` | nothing |

The rules:

- **Value markers** go inside an attribute or text and may appear any number of times.
- **Block markers** are markup and appear at most once. One alone on its line indents every line it writes
  like its own, and removes the line entirely when it writes nothing.
- **`%telar.bootstrap%` is required.** Without it the page never loads the app, so the build refuses the
  template. Put it in `<head>` (the module script is deferred, so it runs after the document is parsed) or
  just before `</body>`.
- **`%telar.fonts%`, `%telar.prerendered%` and `%telar.state%` are required only when the build has content
  for them.** Leaving one out while the build has something for it would silently break the page, so it is
  an error; leaving one out otherwise is fine.
- **An unknown `%telar.<name>%` is an error** naming the marker and its line, so a typo never ships as text.
  Anything else containing `%`, such as `100%` in CSS, is left alone.
- The app mounts on the element with `id="telar-root"`, or on `<body>` when there is none. The built-in page
  gives that element `data-telar-renderer="%telar.renderer%"`, which lets `--renderer` choose the renderer
  without a rebuild; `?telar-renderer=` on the URL still overrides it.
- The built-in page leaves the document free to scroll (no `overflow: hidden` on `html` or `body`). Under
  the document renderer a root `ScrollPage` is the document's own scroll and the host grows with it, and a
  page scrolled before the module loads keeps its position. A template that fixes the document in place
  takes that away. See [docs/primary-scroll.md](primary-scroll.md).

URLs the page writes for output files start with `./`, which resolves against a page at the output root.

A `web/index.html` written before templates existed, loading `./app.js` directly, is refused with a message
naming `%telar.bootstrap%`: the glue now has a hashed name, so a hand-written reference to it cannot work.

## Hashed assets and `asset-manifest.json`

Every file the build itself delivers carries a content hash in its name, `name-<12 hex digits>.ext`. The
same bytes keep the same URL across builds, and different bytes never share one, so a host can serve these
files with `Cache-Control: public, max-age=31536000, immutable`. The module is hashed first and the glue is
pointed at the module's hashed name before it is hashed itself, so a new module is always a new glue URL.

`asset-manifest.json` maps each logical path to its hashed path, sorted by key:

```json
{
  "app.js": "app-3f1c0a9d2b7e.js",
  "app_bg.wasm": "app_bg-8e41d07c55aa.wasm"
}
```

Anything that needs to find a built file (a host profile writing cache headers, a script measuring the
module) reads this file rather than guessing names. Debug and release builds are hashed the same way.

## The public directory

`web/public/**` is copied into the output **verbatim**, at the same relative paths, including dotfiles such
as `.well-known/`. These files are reached by URLs something outside the build already knows
(`/robots.txt`, `/favicon.ico`, `/.well-known/security.txt`, an Open Graph image), so their names are never
hashed and they are not in the manifest; serve them with ordinary revalidating cache headers.

A public file cannot replace something the build writes: `index.html`, `asset-manifest.json` or a hashed
file of the same name is an error naming the file to move.

## Precompression

A release build writes a `.br` (brotli, quality 11) and a `.gz` (gzip, best) beside every file of at least
1 KiB whose format is not already compressed: HTML, JavaScript, CSS, JSON, the module, SVG, XML, plain text,
`.ttf`/`.otf` and similar, in every directory of the output, public files included. PNG, JPEG, WebP, AVIF,
`woff2`, video and audio are skipped, since a second pass over them only costs time. Servers that know the
convention (nginx `brotli_static`/`gzip_static`, Caddy `precompressed`) serve these in place of the
original; those that do not ignore them.

## The dev server

`cargo telar dev --target web` serves the output on `http://localhost:8080/` with `cache-control: no-store`
and reloads the page after each rebuild. It rebuilds on changes under `src/`, `crates/` and `apps/`, and on
changes to the template's directory and the public directory.

- **Media types** follow the file extension: HTML, JavaScript, `wasm` (`application/wasm`, which streaming
  instantiation needs), CSS, JSON, web manifests, SVG, PNG, JPEG, GIF, WebP, AVIF, ICO, `woff`/`woff2`,
  `ttf`/`otf`, MP4, WebM, Ogg, MP3, WAV, plain text, CSV, XML, VTT, PDF, glTF and more. Anything unknown is
  `application/octet-stream`.
- **`Range` requests** are supported for a single byte range (`bytes=a-b`, `bytes=a-`, `bytes=-n`), answered
  with `206 Partial Content` and `Content-Range`, which is what `<video>` and `<audio>` need to seek. A range
  past the end is `416`. Several ranges, another unit or a malformed header get the whole file, as HTTP
  allows.
- **`HEAD`** is answered like `GET` without the body; other methods get `405`.
- Compressible files are gzipped on the fly when the request accepts it, except for range requests.
- Request paths are percent-decoded, and a directory serves its `index.html`. A path that would leave the
  output directory is a `404`.
