//! The page a browser build starts from: a template carrying `%telar.<name>%` markers, and the [`Page`] that fills them.
//!
//! Every marker has a value before any feature feeds it, so the built-in template and a project's own expand the same way whether or not a build prerenders, declares fonts or knows its locale.

use std::fmt;

use layout_core::Direction;
use telar_project::FontDeclaration;

use super::fonts::{FAMILY_ATTRIBUTE, font_face};
use crate::runner::cli::WebRenderer;

/// The page a project gets when it brings no template of its own.
pub(crate) const DEFAULT_TEMPLATE: &str = include_str!("default_page.html");

const MARKER_OPEN: &str = "%telar.";

/// A place in the template the build writes into.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Marker {
    Lang,
    Dir,
    Title,
    Renderer,
    Host,
    Meta,
    Fonts,
    Bootstrap,
    Prerendered,
    State,
}

impl Marker {
    pub(crate) const ALL: [Self; 10] = [
        Self::Lang,
        Self::Dir,
        Self::Title,
        Self::Renderer,
        Self::Host,
        Self::Meta,
        Self::Fonts,
        Self::Bootstrap,
        Self::Prerendered,
        Self::State,
    ];

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Lang => "lang",
            Self::Dir => "dir",
            Self::Title => "title",
            Self::Renderer => "renderer",
            Self::Host => "host",
            Self::Meta => "meta",
            Self::Fonts => "fonts",
            Self::Bootstrap => "bootstrap",
            Self::Prerendered => "prerendered",
            Self::State => "state",
        }
    }

    fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|marker| marker.name() == name)
    }

    /// A value marker sits inside an attribute or text and is escaped; a block marker is markup and appears at most once. `%telar.host%` is neither: attributes, inside the host element's start tag.
    fn is_block(self) -> bool {
        !matches!(
            self,
            Self::Lang | Self::Dir | Self::Title | Self::Renderer | Self::Host
        )
    }

    fn appears_once(self) -> bool {
        self.is_block() || self == Self::Host
    }
}

impl fmt::Display for Marker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{MARKER_OPEN}{}%", self.name())
    }
}

/// Why a template could not be expanded.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum TemplateError {
    Unknown { name: String, line: usize },
    Repeated(Marker),
    Missing(Marker),
}

impl fmt::Display for TemplateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unknown { name, line } => {
                let known: Vec<String> = Marker::ALL.iter().map(Marker::to_string).collect();
                write!(
                    f,
                    "line {line}: `{MARKER_OPEN}{name}%` is not a marker; the markers are {}",
                    known.join(", ")
                )
            }
            Self::Repeated(marker) => write!(f, "`{marker}` appears more than once"),
            Self::Missing(Marker::Bootstrap) => write!(
                f,
                "`{}` is missing, so the page would never load the app; put it in `<head>` or before `</body>`",
                Marker::Bootstrap
            ),
            Self::Missing(marker) => write!(
                f,
                "`{marker}` is missing, and this build has content for it that would be dropped"
            ),
        }
    }
}

fn direction_str(direction: Direction) -> &'static str {
    if direction.is_rtl() { "rtl" } else { "ltr" }
}

/// A void element for `<head>`: `<meta>` or `<link>`, with every attribute value escaped when written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HeadTag {
    element: &'static str,
    attributes: Vec<(String, String)>,
}

impl HeadTag {
    /// `<meta name="…" content="…">`.
    pub(crate) fn meta(name: impl Into<String>, content: impl Into<String>) -> Self {
        Self::bare("meta")
            .attr("name", name)
            .attr("content", content)
    }

    /// `<link rel="…" href="…">`.
    pub(crate) fn link(rel: impl Into<String>, href: impl Into<String>) -> Self {
        Self::bare("link").attr("rel", rel).attr("href", href)
    }

    /// `<meta property="…" content="…">`, the spelling Open Graph reads.
    pub(crate) fn property(property: impl Into<String>, content: impl Into<String>) -> Self {
        Self::bare("meta")
            .attr("property", property)
            .attr("content", content)
    }

    /// `<base href="…">`: what every relative URL in the document, and the address the app reads its locations under, resolve against.
    pub(crate) fn base(href: impl Into<String>) -> Self {
        Self::bare("base").attr("href", href)
    }

    /// A font the page will need, fetched with the page rather than once CSS asks for it. `crossorigin` is required even on the same origin: fonts are always fetched in CORS mode, and a preload in any other mode is fetched twice.
    pub(crate) fn font_preload(href: impl Into<String>, media_type: impl Into<String>) -> Self {
        Self::link("preload", href)
            .attr("as", "font")
            .attr("type", media_type)
            .attr("crossorigin", "")
    }

    /// Another attribute; an empty value writes it bare.
    pub(crate) fn attr(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.attributes.push((name.into(), value.into()));
        self
    }

    fn bare(element: &'static str) -> Self {
        Self {
            element,
            attributes: Vec::new(),
        }
    }

    pub(super) fn html(&self) -> String {
        let mut html = format!("<{}", self.element);
        for (name, value) in &self.attributes {
            html.push(' ');
            html.push_str(&escape(name));
            if !value.is_empty() {
                html.push_str(&format!("=\"{}\"", escape(value)));
            }
        }
        html.push_str(" />");
        html
    }
}

/// The two files that start the app, as paths inside the output.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Bootstrap {
    pub(crate) glue: String,
    pub(crate) module: String,
}

/// One declared face as a page links it: the hashed file under the output root, the media type it is preloaded as when this entry is the first to name the file, and the rule it is declared with.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PageFont {
    pub(crate) declaration: FontDeclaration,
    pub(crate) path: String,
    pub(crate) preload: Option<String>,
}

/// Everything the template's markers expand to. Each field starts at what a build that knows nothing more should write, and a later step overwrites what it knows.
#[derive(Clone, Debug)]
pub(crate) struct Page {
    /// `%telar.lang%`.
    pub(crate) lang: String,
    /// `%telar.dir%`.
    pub(crate) dir: Direction,
    /// `%telar.title%`, escaped.
    pub(crate) title: String,
    /// `%telar.meta%`, one tag per line.
    pub(crate) meta: Vec<HeadTag>,
    /// Markup `%telar.meta%` writes after its tags: what a prerendered page's content needs in `<head>` to look as it will once the app runs.
    pub(crate) head: Vec<String>,
    /// `%telar.fonts%`, each URL written against this page's base.
    pub(crate) fonts: Vec<PageFont>,
    /// `%telar.host%`: the attributes a prerendered page's host element carries, already escaped.
    pub(crate) host: Vec<(String, String)>,
    /// `%telar.bootstrap%`.
    pub(crate) bootstrap: Bootstrap,
    /// `%telar.renderer%`, what `data-telar-renderer` says; `None` writes `auto`.
    pub(crate) renderer: Option<WebRenderer>,
    /// `%telar.prerendered%`, inserted verbatim inside the host element.
    pub(crate) prerendered: String,
    /// `%telar.state%`, the `telar-state` script when there is state to carry.
    pub(crate) state: Option<serde_json::Value>,
    /// What every URL the page writes for an output file starts with: the path the site is served under, so a page at any depth, and one served at any address, reaches the same files.
    pub(crate) base: String,
}

impl Page {
    pub(crate) fn new(title: impl Into<String>, bootstrap: Bootstrap) -> Self {
        Self {
            lang: "en".to_string(),
            dir: Direction::default(),
            title: title.into(),
            meta: Vec::new(),
            head: Vec::new(),
            fonts: Vec::new(),
            host: Vec::new(),
            bootstrap,
            renderer: None,
            prerendered: String::new(),
            state: None,
            base: "/".to_string(),
        }
    }

    /// The URL of an output file, as this page should write it.
    pub(crate) fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }

    /// Expands every marker in `template`.
    ///
    /// A block marker alone on its line takes the line's indentation onto each line it writes, and takes the line away entirely when it writes nothing.
    pub(crate) fn render(&self, template: &str) -> Result<String, TemplateError> {
        let found = markers_in(template)?;
        self.check(&found)?;

        let mut out = String::with_capacity(template.len() * 2);
        let mut cursor = 0;
        for (start, marker) in found {
            out.push_str(&template[cursor..start]);
            let after = start + marker.to_string().len();
            cursor = after;
            let expansion = self.expand(marker);
            if !marker.is_block() {
                out.push_str(&expansion);
                continue;
            }
            let line_start = out.rfind('\n').map_or(0, |i| i + 1);
            let indent = &out[line_start..];
            let rest_of_line = template[after..].split('\n').next().unwrap_or_default();
            let alone = indent.trim().is_empty() && rest_of_line.trim().is_empty();
            if alone && expansion.is_empty() {
                out.truncate(line_start);
                cursor = (after + rest_of_line.len() + 1).min(template.len());
                continue;
            }
            let indent = if indent.trim().is_empty() {
                indent.to_string()
            } else {
                String::new()
            };
            out.push_str(&expansion.replace('\n', &format!("\n{indent}")));
        }
        out.push_str(&template[cursor..]);
        Ok(out)
    }

    fn check(&self, found: &[(usize, Marker)]) -> Result<(), TemplateError> {
        for marker in Marker::ALL {
            let count = found.iter().filter(|(_, m)| *m == marker).count();
            if marker.appears_once() && count > 1 {
                return Err(TemplateError::Repeated(marker));
            }
            if count == 0 && self.must_appear(marker) {
                return Err(TemplateError::Missing(marker));
            }
        }
        Ok(())
    }

    /// Whether leaving `marker` out of a template would lose something the page cannot work without: the app itself, a font it draws with, or the markup and state hydration adopts.
    fn must_appear(&self, marker: Marker) -> bool {
        match marker {
            Marker::Bootstrap => true,
            Marker::Fonts => !self.fonts.is_empty(),
            Marker::Host => !self.host.is_empty(),
            Marker::Prerendered => !self.prerendered.is_empty(),
            Marker::State => self.state.is_some(),
            _ => false,
        }
    }

    fn expand(&self, marker: Marker) -> String {
        match marker {
            Marker::Lang => escape(&self.lang),
            Marker::Dir => direction_str(self.dir).to_string(),
            Marker::Title => escape(&self.title),
            Marker::Renderer => self
                .renderer
                .map_or("auto", WebRenderer::as_str)
                .to_string(),
            Marker::Host => self
                .host
                .iter()
                .map(|(name, value)| format!(" {name}=\"{value}\""))
                .collect(),
            Marker::Meta => {
                let mut parts = vec![lines(&self.meta)];
                parts.extend(self.head.iter().cloned());
                parts.retain(|part| !part.is_empty());
                parts.join("\n")
            }
            Marker::Fonts => self.fonts(),
            Marker::Bootstrap => self.bootstrap(),
            Marker::Prerendered => self.prerendered.clone(),
            Marker::State => self
                .state
                .as_ref()
                .map(|state| {
                    format!(
                        "<script type=\"application/json\" id=\"telar-state\">{}</script>",
                        script_json(state)
                    )
                })
                .unwrap_or_default(),
        }
    }

    fn fonts(&self) -> String {
        let preloads: Vec<HeadTag> = self
            .fonts
            .iter()
            .filter_map(|font| {
                let media_type = font.preload.as_ref()?;
                Some(
                    HeadTag::font_preload(self.url(&font.path), media_type.clone())
                        .attr(FAMILY_ATTRIBUTE, font.declaration.family.clone()),
                )
            })
            .collect();
        let faces: String = self
            .fonts
            .iter()
            .map(|font| font_face(&font.declaration, &self.url(&font.path)))
            .collect();
        let mut parts = vec![lines(&preloads)];
        if !faces.is_empty() {
            parts.push(format!("<style>\n{}\n</style>", faces.trim_end()));
        }
        parts.retain(|part| !part.is_empty());
        parts.join("\n")
    }

    /// Both files are requested with the page rather than one after the other: the module is only reached through an import inside the glue, so without the preload the browser learns it exists after fetching and parsing the glue, two round trips in series on the largest file here.
    ///
    /// The meta says where the build's files are, relative to this page, for the app to resolve once at start: an address it resolved later would resolve against wherever it had navigated to.
    fn bootstrap(&self) -> String {
        let glue = self.url(&self.bootstrap.glue);
        let module = self.url(&self.bootstrap.module);
        let preloads = lines(&[
            HeadTag::meta("telar-assets", self.base.clone()),
            HeadTag::link("modulepreload", glue.clone()),
            HeadTag::link("preload", module)
                .attr("as", "fetch")
                .attr("type", "application/wasm")
                .attr("crossorigin", ""),
        ]);
        format!(
            "{preloads}\n<script type=\"module\">\n  import init from {};\n  const wasm = await init();\n  wasm.telar_start();\n</script>",
            script_json(&serde_json::Value::String(glue))
        )
    }
}

fn markers_in(template: &str) -> Result<Vec<(usize, Marker)>, TemplateError> {
    let mut found = Vec::new();
    for (start, _) in template.match_indices(MARKER_OPEN) {
        let rest = &template[start + MARKER_OPEN.len()..];
        let name_len = rest
            .find(|c: char| !(c.is_ascii_lowercase() || c == '_'))
            .unwrap_or(rest.len());
        if name_len == 0 || !rest[name_len..].starts_with('%') {
            continue;
        }
        let name = &rest[..name_len];
        let marker = Marker::from_name(name).ok_or_else(|| TemplateError::Unknown {
            name: name.to_string(),
            line: template[..start].matches('\n').count() + 1,
        })?;
        found.push((start, marker));
    }
    Ok(found)
}

fn lines(tags: &[HeadTag]) -> String {
    tags.iter()
        .map(HeadTag::html)
        .collect::<Vec<_>>()
        .join("\n")
}

/// JSON that is safe inside a `<script>`: `<` only ever occurs inside a JSON string, where `<` means the same thing and cannot close the element.
pub(super) fn script_json(value: &serde_json::Value) -> String {
    value.to_string().replace('<', "\\u003c")
}

pub(super) fn escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            c => escaped.push(c),
        }
    }
    escaped
}

#[cfg(test)]
#[path = "page_test.rs"]
mod tests;
