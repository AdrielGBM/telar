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
base = "/"                    # the path the site is served under; default shown
origin = "https://example.com"          # the scheme and host it is served at; no default
description = "meta.description"        # a catalog key, read in each page's locale
og_image = { es = "og/es.png", en = "og/en.png" }   # or one path or URL for every locale
theme_color = { light = "#f4f4f2", dark = "#101112" } # or one color
host = "static"               # or "cloudflare-pages"; default shown
```

The two paths are relative to the package root. Every key is inherited from a workspace `telar.toml` key by
key, like the rest of `[telar]`. A project with no `[telar.web]` gets the defaults, and a default that does
not exist is fine: no template means the built-in page, no public directory means nothing to copy. A path you
**name** that does not exist is an error. So is a misspelled key, as everywhere else in `telar.toml`, and so is
a value that cannot describe a site: an `origin` with a path, a `base` with `..` in it, an `og_image` of the
site with no `origin` to make its URL from, or `host = "cloudflare-pages"` under a `base` other than `/`.

The keys after `public` describe the site rather than the build; see [Describing the site](#describing-the-site)
and [Host profiles](#host-profiles).

## Output

| File | What it is |
| --- | --- |
| `index.html` | The page, expanded from the template. With `--prerender` and an address that carries a locale, the [negotiating root](#the-root-of-a-site-in-several-locales) instead. |
| `<locale>/<path>/index.html`, `404.html` | With `--prerender`, every page of the app written ahead of time; see [Prerendering](prerender.md). |
| `sitemap.xml` | With `--prerender` and an `origin`: every page written, with its translations; see [The sitemap](#the-sitemap). |
| `app-<hash>.js` | The `wasm-bindgen` glue, pointed at the hashed module. |
| `app_bg-<hash>.wasm` | The module. |
| `fonts/<name>-<hash>.<ext>` | Each face `[[telar.fonts]]` declares; see [Fonts as assets](fonts.md). |
| `images/<hash>.<ext>`, `images/<hash>-<width>w.<ext>` | Each picture an `img src:"…"` bakes, and its smaller copies; see [Pictures](#pictures). |
| `asset-manifest.json` | Logical path → hashed path for every hashed file. |
| everything from `web/public/` | Copied as is, at the same relative path. |
| `*.br`, `*.gz` | Release builds for `host = "static"` only: precompressed copies (see below). |
| `_headers`, `_redirects`, `_worker.js`, `_routes.json` | `host = "cloudflare-pages"` only; see [Host profiles](#host-profiles). |

The output directory is assembled beside `web/` and swapped in whole, so it only ever holds one build:
nothing from a previous build is left behind, and a server reading it mid-build sees the last complete one.

## The page template

The template is plain HTML with `%telar.<name>%` markers. Without one, the build uses the built-in page,
which is itself a template (`crates/tools/cargo-telar/src/runner/package/web/default_page.html`) and a good
starting point to copy into `web/index.html`.

| Marker | Kind | Expands to | Default |
| --- | --- | --- | --- |
| `%telar.lang%` | value | the document language, escaped | `[telar.i18n] default`, or `en` |
| `%telar.dir%` | value | `ltr` or `rtl`, derived from `%telar.lang%` the same way [`Direction::for_locale`](https://docs.rs/telar-layout-core) resolves it at runtime | derived from the default above |
| `%telar.title%` | value | the page title, escaped | the package name |
| `%telar.renderer%` | value | the `--renderer` choice, for `data-telar-renderer` | `auto` |
| `%telar.host%` | attributes | the attributes of a prerendered page's host element, inside its start tag | nothing |
| `%telar.meta%` | block | the `<base>`, `<meta>` and `<link>` tags that describe the page ([Describing the site](#describing-the-site)) | a `description` and the preview tags |
| `%telar.fonts%` | block | a preload and an `@font-face` rule per declared face ([fonts](fonts.md)) | nothing |
| `%telar.bootstrap%` | block | the preloads and the module script that start the app | always present |
| `%telar.prerendered%` | block | prerendered markup for the host element ([prerendering](prerender.md)) | nothing |
| `%telar.state%` | block | `<script type="application/json" id="telar-state">` | nothing |

`%telar.lang%` and `%telar.dir%` are a build-time default: the page a browser first sees, before the wasm
module has run. Once it has, `<html lang>` and `<html dir>` follow the active locale instead — the app can
call `set_locale` with anything, and both attributes move with it, mirroring the writing direction
`follow_locale_direction` already resolves for layout. `[telar.i18n] default` only decides the default this
page opens with.

The rules:

- **Value markers** go inside an attribute or text and may appear any number of times.
- **Block markers** are markup and appear at most once. One alone on its line indents every line it writes
  like its own, and removes the line entirely when it writes nothing.
- **`%telar.bootstrap%` is required.** Without it the page never loads the app, so the build refuses the
  template. Put it in `<head>` (the module script is deferred, so it runs after the document is parsed) or
  just before `</body>`.
- **`%telar.host%`** goes inside the start tag of the element the app mounts on, at most once. It writes its
  own leading space.
- **`%telar.fonts%`, `%telar.host%`, `%telar.prerendered%` and `%telar.state%` are required only when the build
  has content for them.** Leaving one out while the build has something for it would silently break the page, so it is
  an error; leaving one out otherwise is fine.
- **An unknown `%telar.<name>%` is an error** naming the marker and its line, so a typo never ships as text.
  Anything else containing `%`, such as `100%` in CSS, is left alone.
- The app mounts on the element with `id="telar-root"`, or on `<body>` when there is none. The built-in page
  gives that element `data-telar-renderer="%telar.renderer%"`, which lets `--renderer` choose the renderer
  without a rebuild; `?telar-renderer=` on the URL still overrides it, and `%telar.host%` after it.
- The built-in page leaves the document free to scroll (no `overflow: hidden` on `html` or `body`). Under
  the document renderer a root `ScrollPage` is the document's own scroll and the host grows with it, and a
  page scrolled before the module loads keeps its position. A template that fixes the document in place
  takes that away. See [docs/primary-scroll.md](primary-scroll.md).

URLs the page writes for output files start with the site's base path (`/app-….js`, or `/docs/app-….js` under
`base = "/docs/"`), so every page reaches the same files from any depth, and `404.html` from any address a
host serves it at. The bootstrap also writes `<meta name="telar-assets" content="/">`: the app resolves it
once, when it starts, and fetches every file the build shipped from there, so an address it pushes later does
not move them. A site that is not at the root of its domain says where it is with `base`; the build is then
served from that path only, by the dev server too.

Under a `base` other than `/`, `%telar.meta%` starts with `<base href="/docs/">`. That is where the app reads
the path its own addresses live under (see [docs/location.md](location.md)), and what every relative URL in the
document resolves against, so keep `%telar.meta%` above any element of the template that names a URL. A
fragment-only link of the template's own (`href="#main"`) resolves against it too: write it with the page's
path, or let the app draw it with `anchor:`.

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

## Pictures

A browser build does not carry the pixels of an `img src:"…"`. The bake gives every target its own form of
the picture: the decoded pixels on the desktop, a terminal and Android, and on the web an address and the
picture's size, so the module is as heavy as its code and layout still knows the box before a byte arrives.
The packager writes the file at that address, from the crates the app is built from.

- **Names are the content hash** of the source file (`images/<16 hex digits>.<ext>`), so a changed picture is
  a new URL and the files can be cached like the rest of the hashed output. They are not in
  `asset-manifest.json`: the module already knows their names.
- **A format a browser reads** (PNG, JPEG, GIF, WebP, AVIF, BMP, ICO) ships as it is. Any other is
  re-encoded as PNG.
- **Smaller copies** are made at half the width, and half again, while they stay at least 480 pixels wide,
  in the same format (PNG for a converted one). A copy that would not be smaller than the file above it is
  dropped, and GIF, AVIF and ICO get none.

Under the document renderer a picture drawn with `fit:` `contain`, `cover` or `fill` becomes an `<img>`
with `srcset` and `sizes`, so the browser picks the copy that fits the box and the screen's density,
decodes it off the main thread and loads it lazily. `priority` on the `img` marks the picture the page is
about (a hero, a cover): it is fetched at once and first (`fetchpriority="high"`, no `loading="lazy"`). An
`img` with `label:` gets it as its `alt`; one without has `alt=""` and is decoration. Other fills and nine
slices stay part of a drawing and draw the full-size file from its address.

Under the canvas renderer the picture is fetched and decoded by the browser, then drawn like any other
bitmap, choosing the narrowest copy that covers the box. It is left out of the frames before it arrives.

```rsx
img src:"hero.jpg" fit:cover width:100% height:60sh priority label:"The loom at work"
```

## The public directory

`web/public/**` is copied into the output **verbatim**, at the same relative paths, including dotfiles such
as `.well-known/`. These files are reached by URLs something outside the build already knows
(`/robots.txt`, `/favicon.ico`, `/.well-known/security.txt`, an Open Graph image), so their names are never
hashed and they are not in the manifest; serve them with ordinary revalidating cache headers.

A public file cannot replace something the build writes: `index.html`, `asset-manifest.json`, a hashed
file, and with `--prerender` `404.html` and `sitemap.xml`, or under `cloudflare-pages` `_worker.js` and
`_routes.json`, of the same name is an error naming the file to move. A page `--prerender` writes at the path
of a public file is one too. `_headers` and `_redirects` are the exception: the profile extends them.

## Describing the site

What a crawler, a link preview or a browser's own interface reads about a page is not something a
component says: it is derived from `[telar.web]`, the app's catalogs, its routes and the titles the app gives
them (see [docs/surface-title.md](surface-title.md)). `%telar.meta%` writes it, on every page the build
writes, in this order:

| Tag | From | When |
| --- | --- | --- |
| `<base href>` | `base` | `base` is not `/` |
| `<meta name="description">` | the message under `description` in the page's locale, or the locale it negotiates to, or the catalog's default; without the key, `<app>, a Telar application.` | always |
| `<meta name="theme-color">` | `theme_color`; a `{ light, dark }` table writes one per `prefers-color-scheme` | `theme_color` is set |
| `<link rel="canonical">` | `origin` + the page's address | `origin` is set, on every page but `404.html` |
| `<link rel="alternate" hreflang>` | the same page in every locale the address carries, and `x-default` for the base locale's | `origin` is set and the page is in a locale |
| `og:type`, `og:title`, `og:description` | `website`, the page title, the description | always |
| `og:url` | the canonical address | `origin` is set |
| `og:locale`, `og:locale:alternate` | the page's locale and the others the address carries, as `es`, `en_US` | always; the alternates on a page in a locale |
| `og:image` | `og_image` for the page's locale, else the base locale's; a path is a file of the public directory, made a URL with `origin` and `base` | `og_image` is set |
| `twitter:card` | `summary_large_image` with an image, `summary` without | always |

A page's address is its directory with the closing `/` (`https://example.com/es/projects/`), which is what a
static host serves `es/projects/index.html` at. The description key has to be plain text in every locale that
has it, a message with no `{arguments}` and no plural forms, and a key the catalog does not have is an error.
So is an `og_image` path that is not a file of the public directory: name the picture, say
`web/public/og/es.png` as `"og/es.png"`, and it is copied and linked with the rest.

Search engines want full URLs for all of this, so a site with no `origin` gets the description, the theme
color and the preview text, and no canonical, alternate-language or `og:url` tags. A `--prerender` build
without one says so. Until a site has its domain, an `origin` placeholder is fine: it is one key to change.

## The root of a site in several locales

When the app's address carries its locale (`follow_location_locale`, see [docs/location.md](location.md)),
`/` is not a page: every page lives under its locale (`/es/`, `/en/projects/`). With `--prerender`, the build
writes `index.html` as the page that sends a reader to theirs:

- **With scripts**, it picks a locale from `navigator.languages` with the rule `negotiate_locale` uses, the
  one the app uses when an address names no locale: for each preferred tag in order, an exact match ignoring
  case, then the first locale with the same language; the base locale when nothing matches. It then
  `location.replace`s to that locale's root, keeping the query and the fragment, so the root leaves no entry
  in the history. The script is the rule written once in JavaScript and shared with the Cloudflare Pages
  worker below; a test runs both against `negotiate_locale` itself.
- **Without scripts**, a `<noscript>` refresh goes to the base locale's root, and the page lists a link to
  every locale's root, labelled with that root's title in its own language.
- **For a crawler**, it carries `<link rel="alternate" hreflang>` to every locale's root, `x-default` to the
  base locale's, and, with an `origin`, a canonical link to the base locale's root.

The base locale is the one `follow_location_locale(available, base)` names, as the app reports it while the
page is prerendered.

A plain build (no `--prerender`) writes no such page: its `index.html` is the app, which chooses the locale
itself and rewrites the address.

## The sitemap

A `--prerender` build of a site with an `origin` writes `sitemap.xml`: every page it wrote, each with an
`<xhtml:link rel="alternate" hreflang>` to every translation of it and `x-default` to the base locale's. The
negotiating root and `404.html` are not in it. Point crawlers at it from `web/public/robots.txt`:

```text
Sitemap: https://example.com/sitemap.xml
```

## Host profiles

`host` names the static host the output is shaped for. What the build writes for it only means something to
that host and is written beside the site, never into a page.

### `static` (the default)

Any server that maps paths to files. Release builds carry precompressed copies (see
[Precompression](#precompression)), and nothing else is written for the host. Cache headers are the server's
configuration: give the hashed files `Cache-Control: public, max-age=31536000, immutable`, the pages
`no-cache`, and `/` `Vary: Accept-Language` if the server negotiates it.

### `cloudflare-pages`

Cloudflare Pages, deployed from the output directory (with the Git integration or `wrangler pages deploy`).
It serves the site at the root of its domain, so this profile needs `base = "/"`.

- **`_headers`**: `public, max-age=31536000, immutable` for every file in `asset-manifest.json` and everything
  under `images/`; `no-cache` for `/`, `404.html`, and every page (as `/<locale>/*` for each locale, or each
  page's own path when the address carries none); `Vary: Accept-Language` on `/` when it negotiates. Pages
  joins the values of every rule a request matches, so no page rule ever covers a hashed file.
- **`_redirects`**: when the address carries a locale, a `302` from each page's address without one
  (`/projects` and `/projects/`) to the base locale's (`/es/projects/`).
- **`_worker.js`** and **`_routes.json`**: when the address carries a locale, a worker that answers `/` with a
  `302` to the root of the locale `Accept-Language` negotiates to (by `q`, then order; `*` and `q=0` ignored),
  keeping the query, with `Vary: Accept-Language` and `Cache-Control: no-cache`. `_routes.json` sends only `/`
  to it, so every other request is a static file and costs no invocation. The negotiating `index.html` is still
  there for a request the worker does not answer.
- **No precompressed copies.** Cloudflare compresses at its edge by the response's content type, and its list
  includes `application/wasm` along with HTML, JavaScript, CSS, JSON and SVG
  ([Cloudflare docs](https://developers.cloudflare.com/speed/optimization/content/compression/)); `.br` and
  `.gz` files would only be uploaded and never served.
- **A warning for every file over 25 MiB**, the largest Pages serves.

A `_headers` or `_redirects` in the public directory is kept, with the derived rules written after it: every
matching header rule applies, and the first matching redirect wins, so the project's own redirects come
first. The result has to fit Pages' limits (100 header rules, 2,100 redirects) or the build stops and says
which file is over. `_worker.js` and `_routes.json` are the build's; one in the public directory is an error.

The worker is Pages' "advanced mode" rather than a `functions/` directory: Pages reads `functions/` from the
project root, never from the output it deploys, while `_worker.js` and `_routes.json` travel with the output.
`_headers` and `_redirects` do not apply to what the worker answers, which is why it sets its own headers.

The worker can only read `Accept-Language`. A reader who chose another locale in the app (kept in
`localStorage`, see [docs/user-preferences.md](user-preferences.md)) is still sent to the negotiated one at
`/`; their choice holds from the first page they open from there.

## Precompression

A release build for `host = "static"` writes a `.br` (brotli, quality 11) and a `.gz` (gzip, best) beside every file of at least
1 KiB whose format is not already compressed: HTML, JavaScript, CSS, JSON, the module, SVG, XML, plain text,
`.ttf`/`.otf` and similar, in every directory of the output, public files included. PNG, JPEG, WebP, AVIF,
`woff2`, video and audio are skipped, since a second pass over them only costs time. Servers that know the
convention (nginx `brotli_static`/`gzip_static`, Caddy `precompressed`) serve these in place of the
original; those that do not ignore them.

## The dev server

`cargo telar dev --target web` serves the output on `http://localhost:8080/` (under `base`, with `/` redirecting
there, when the site names one) with `cache-control: no-store` and reloads the page after each rebuild. It rebuilds on changes under `src/`, `crates/` and `apps/`, and on
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
