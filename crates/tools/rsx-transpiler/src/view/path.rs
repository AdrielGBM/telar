//! Declarative vector-path emitter: a `path d:"…"` tag whose SVG path-data string is parsed at
//! compile time into a `PathData` builder chain and drawn as a `Path` inside a sized `Canvas`.
//!
//! The runtime `Path` widget is not a `LayoutItem` (its points are absolute, not relative to a layout
//! rect), so it cannot be a bare child of a `col`/`row`. We wrap it in a `Canvas` — the same escape hatch
//! the imperative `PathData` demo uses — so the declarative `path` slots into layout like any other widget.

use rsx_parser::Element;

use crate::style::format_f32;

use super::signals::{rust_str, wrap_signal_clones};
use super::{ChildEmit, ViewGen};

impl ViewGen<'_> {
    pub(super) fn emit_path(&mut self, el: &Element) -> ChildEmit {
        let var = self.next_variable_name("path");
        let pad = self.indent_str();

        // Parse the `d:` path-data string into an absolute `PathData` builder chain at compile time. A missing/invalid `d` becomes a `compile_error!` on this element's line (via the source map).
        let (data_chain, extent, parse_err) = match el.attributes.iter().find(|a| a.key == "d") {
            Some(a) => match parse_path_data(&a.value) {
                Ok(built) => (built.chain, Some((built.max_x, built.max_y)), None),
                Err(e) => (
                    "PathData::new()".to_string(),
                    None,
                    Some(format!("rsx: path `d` — {e}")),
                ),
            },
            None => (
                "PathData::new()".to_string(),
                None,
                Some("rsx: a `path` needs a `d:` attribute with SVG path data".to_string()),
            ),
        };

        let path_style = self.path_style_expr(el);

        // Explicit width/height win; otherwise the path's own extent sizes the wrapping canvas so it doesn't collapse to zero.
        let mut layout = self.make_layout_style("path", &el.classes, &el.attributes);
        if let Some((max_x, max_y)) = extent {
            if !el.attributes.iter().any(|a| a.key == "width") && max_x > 0.0 {
                layout.push_str(&format!(".width({})", format_f32(max_x)));
            }
            if !el.attributes.iter().any(|a| a.key == "height") && max_y > 0.0 {
                layout.push_str(&format!(".height({})", format_f32(max_y)));
            }
        }

        // A `$signal` fill/stroke resolves through `color_expr`'s `.get()` branch; clone it into both the outer draw closure (so the outer binding stays reusable) and the inner PathStyle closure (which re-reads it each frame), mirroring how `emit_canvas` clones canvas-child colours.
        let raw_colors: Vec<&str> = el
            .attributes
            .iter()
            .filter(|a| crate::registry::color_attr_keys().contains(&a.key.as_str()))
            .map(|a| a.value.as_str())
            .collect();
        let style_closure = wrap_signal_clones(&raw_colors, format!("move || {path_style}"));
        let path_expr = format!("Path::static_data(__path_data.clone(), {style_closure}).view()");
        let draw_closure = wrap_signal_clones(&raw_colors, format!("move |_r| {path_expr}"));

        let err_line = match parse_err {
            Some(e) => format!("{pad}    compile_error!({});\n", rust_str(&e)),
            None => String::new(),
        };

        let code = format!(
            "{pad}let {var} = {{\n\
             {err_line}\
             {pad}    let __path_data = std::sync::Arc::new({data_chain});\n\
             {pad}    Canvas::new(ctx, {layout}, {draw_closure})?\n\
             {pad}}};"
        );
        ChildEmit::Simple { name: var, code }
    }

    /// Builds the `PathStyle { … }` literal from `fill`/`stroke`/`stroke_width`/`fill_rule` attributes. Colours reuse `color_expr` (theme token / `#hex` / `$signal`), matching `box fill:`.
    fn path_style_expr(&self, el: &Element) -> String {
        let fill = el
            .attributes
            .iter()
            .find(|a| a.key == "fill")
            .map(|a| format!("Some(Paint::Solid({}))", self.color_expr(&a.value)))
            .unwrap_or_else(|| "None".to_string());
        let stroke = match el.attributes.iter().find(|a| a.key == "stroke") {
            Some(a) => {
                let color = self.color_expr(&a.value);
                let width = el
                    .attributes
                    .iter()
                    .find(|a| a.key == "stroke_width" || a.key == "stroke_w")
                    .and_then(|a| a.value.trim().parse::<f32>().ok())
                    .unwrap_or(1.0);
                format!("Some(Stroke::new({color}, {}))", format_f32(width))
            }
            None => "None".to_string(),
        };
        let fill_rule = match el
            .attributes
            .iter()
            .find(|a| a.key == "fill_rule")
            .map(|a| a.value.trim().to_ascii_lowercase())
        {
            Some(v) if v == "even_odd" || v == "even-odd" || v == "evenodd" => "FillRule::EvenOdd",
            _ => "FillRule::Winding",
        };
        format!(
            "PathStyle {{ fill: {fill}, stroke: {stroke}, shadow: None, fill_rule: {fill_rule} }}"
        )
    }
}

/// A parsed path: the `PathData::new()…` builder-chain expression plus the maximum x/y extent reached (including Bézier control points), used to size the wrapping canvas when width/height are omitted.
struct PathBuild {
    chain: String,
    max_x: f32,
    max_y: f32,
}

/// Parses an SVG path-data string (`d`) into an absolute `PathData` builder chain. Supported commands, absolute (uppercase) and relative (lowercase): `M`/`m` (moveto), `L`/`l` (lineto), `H`/`h` (horizontal lineto), `V`/`v` (vertical lineto), `C`/`c` (cubic Bézier), `Q`/`q` (quadratic Bézier), `Z`/`z` (closepath). Relative coordinates and `H`/`V` are resolved to absolute points at parse time so the emitted chain always uses absolute `move_to`/`line_to`/`quad_to`/`cubic_to`. Unsupported commands (`S`/`T`/`A`) and malformed input return an error.
fn parse_path_data(d: &str) -> Result<PathBuild, String> {
    let mut cur = Cursor::new(d);
    let mut chain = String::from("PathData::new()");
    // Current point, and the start of the current subpath (where `Z` returns to).
    let (mut cx, mut cy) = (0.0f32, 0.0f32);
    let (mut sx, mut sy) = (0.0f32, 0.0f32);
    let mut ext = Extent::default();

    cur.skip_sep();
    if cur.peek().is_none() {
        return Err("is empty".to_string());
    }
    if !matches!(cur.peek(), Some(b'M') | Some(b'm')) {
        return Err("must begin with a moveto (`M`) command".to_string());
    }

    loop {
        cur.skip_sep();
        let Some(c) = cur.peek() else { break };
        if !c.is_ascii_alphabetic() {
            return Err(format!("unexpected character `{}`", c as char));
        }
        cur.bump();
        let rel = c.is_ascii_lowercase();
        match c.to_ascii_uppercase() {
            b'M' => {
                let (mut x, mut y) = cur.pair()?;
                if rel {
                    x += cx;
                    y += cy;
                }
                cx = x;
                cy = y;
                sx = x;
                sy = y;
                ext.add(x, y);
                chain.push_str(&format!(".move_to(Point::new({}, {}))", fx(x), fx(y)));
                // Extra coordinate pairs after a moveto are implicit lineto commands.
                while cur.peek_is_number() {
                    let (mut lx, mut ly) = cur.pair()?;
                    if rel {
                        lx += cx;
                        ly += cy;
                    }
                    cx = lx;
                    cy = ly;
                    ext.add(lx, ly);
                    chain.push_str(&format!(".line_to(Point::new({}, {}))", fx(lx), fx(ly)));
                }
            }
            b'L' => {
                let mut drawn = false;
                while cur.peek_is_number() {
                    let (mut x, mut y) = cur.pair()?;
                    if rel {
                        x += cx;
                        y += cy;
                    }
                    cx = x;
                    cy = y;
                    ext.add(x, y);
                    chain.push_str(&format!(".line_to(Point::new({}, {}))", fx(x), fx(y)));
                    drawn = true;
                }
                if !drawn {
                    return Err("`L` needs at least one coordinate pair".to_string());
                }
            }
            b'H' => {
                let mut drawn = false;
                while cur.peek_is_number() {
                    let mut x = cur.number()?;
                    if rel {
                        x += cx;
                    }
                    cx = x;
                    ext.add(cx, cy);
                    chain.push_str(&format!(".line_to(Point::new({}, {}))", fx(cx), fx(cy)));
                    drawn = true;
                }
                if !drawn {
                    return Err("`H` needs at least one coordinate".to_string());
                }
            }
            b'V' => {
                let mut drawn = false;
                while cur.peek_is_number() {
                    let mut y = cur.number()?;
                    if rel {
                        y += cy;
                    }
                    cy = y;
                    ext.add(cx, cy);
                    chain.push_str(&format!(".line_to(Point::new({}, {}))", fx(cx), fx(cy)));
                    drawn = true;
                }
                if !drawn {
                    return Err("`V` needs at least one coordinate".to_string());
                }
            }
            b'C' => {
                let mut drawn = false;
                while cur.peek_is_number() {
                    let (mut x1, mut y1) = cur.pair()?;
                    let (mut x2, mut y2) = cur.pair()?;
                    let (mut x, mut y) = cur.pair()?;
                    if rel {
                        x1 += cx;
                        y1 += cy;
                        x2 += cx;
                        y2 += cy;
                        x += cx;
                        y += cy;
                    }
                    cx = x;
                    cy = y;
                    ext.add(x1, y1);
                    ext.add(x2, y2);
                    ext.add(x, y);
                    chain.push_str(&format!(
                        ".cubic_to(Point::new({}, {}), Point::new({}, {}), Point::new({}, {}))",
                        fx(x1),
                        fx(y1),
                        fx(x2),
                        fx(y2),
                        fx(x),
                        fx(y)
                    ));
                    drawn = true;
                }
                if !drawn {
                    return Err(
                        "`C` needs six coordinates (two control points and an endpoint)"
                            .to_string(),
                    );
                }
            }
            b'Q' => {
                let mut drawn = false;
                while cur.peek_is_number() {
                    let (mut x1, mut y1) = cur.pair()?;
                    let (mut x, mut y) = cur.pair()?;
                    if rel {
                        x1 += cx;
                        y1 += cy;
                        x += cx;
                        y += cy;
                    }
                    cx = x;
                    cy = y;
                    ext.add(x1, y1);
                    ext.add(x, y);
                    chain.push_str(&format!(
                        ".quad_to(Point::new({}, {}), Point::new({}, {}))",
                        fx(x1),
                        fx(y1),
                        fx(x),
                        fx(y)
                    ));
                    drawn = true;
                }
                if !drawn {
                    return Err(
                        "`Q` needs four coordinates (a control point and an endpoint)".to_string(),
                    );
                }
            }
            b'Z' => {
                cx = sx;
                cy = sy;
                chain.push_str(".close()");
            }
            other => {
                return Err(format!(
                    "unsupported command `{}` (supported: M L H V C Q Z, absolute or relative)",
                    other as char
                ));
            }
        }
    }

    Ok(PathBuild {
        chain,
        max_x: ext.max_x.max(0.0),
        max_y: ext.max_y.max(0.0),
    })
}

/// Formats a coordinate as a valid `f32` literal (`0` -> `0.0`).
fn fx(n: f32) -> String {
    format_f32(n)
}

/// Running maximum x/y over every point (endpoints and control points) seen while parsing.
#[derive(Default)]
struct Extent {
    max_x: f32,
    max_y: f32,
}

impl Extent {
    fn add(&mut self, x: f32, y: f32) {
        self.max_x = self.max_x.max(x);
        self.max_y = self.max_y.max(y);
    }
}

/// A byte cursor over the path-data string with SVG-flavored number scanning (signs, decimals, and
/// exponents; commas and whitespace are interchangeable separators).
struct Cursor<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Cursor<'a> {
    fn new(s: &'a str) -> Self {
        Self {
            b: s.as_bytes(),
            i: 0,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.b.get(self.i).copied()
    }

    fn bump(&mut self) {
        self.i += 1;
    }

    fn skip_sep(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r' | b',')) {
            self.i += 1;
        }
    }

    /// Whether the next non-separator byte begins a number (digit, sign, or a leading decimal point).
    fn peek_is_number(&self) -> bool {
        let mut j = self.i;
        while matches!(self.b.get(j), Some(b' ' | b'\t' | b'\n' | b'\r' | b',')) {
            j += 1;
        }
        matches!(self.b.get(j), Some(c) if c.is_ascii_digit() || *c == b'+' || *c == b'-' || *c == b'.')
    }

    /// Reads one number (skipping leading separators). SVG allows a decimal point or a sign to begin a new number with no separator, which this handles because scanning stops at the next sign/second dot.
    fn number(&mut self) -> Result<f32, String> {
        self.skip_sep();
        let start = self.i;
        if matches!(self.peek(), Some(b'+' | b'-')) {
            self.bump();
        }
        let mut saw_digit = false;
        while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
            self.bump();
            saw_digit = true;
        }
        if self.peek() == Some(b'.') {
            self.bump();
            while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                self.bump();
                saw_digit = true;
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.bump();
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.bump();
            }
            while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                self.bump();
            }
        }
        if !saw_digit {
            return Err("expected a number".to_string());
        }
        let text = std::str::from_utf8(&self.b[start..self.i]).unwrap_or("");
        text.parse::<f32>()
            .map_err(|_| format!("invalid number `{text}`"))
    }

    fn pair(&mut self) -> Result<(f32, f32), String> {
        let x = self.number()?;
        let y = self.number()?;
        Ok((x, y))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn triangle_absolute() {
        let built = parse_path_data("M0,0 L100,0 L50,80 Z").unwrap();
        assert_eq!(
            built.chain,
            "PathData::new().move_to(Point::new(0.0, 0.0)).line_to(Point::new(100.0, 0.0)).line_to(Point::new(50.0, 80.0)).close()"
        );
        assert_eq!(built.max_x, 100.0);
        assert_eq!(built.max_y, 80.0);
    }

    #[test]
    fn implicit_lineto_after_moveto() {
        // `M` followed by extra pairs treats the extras as `line_to`.
        let built = parse_path_data("M0 0 10 0 10 10").unwrap();
        assert_eq!(
            built.chain,
            "PathData::new().move_to(Point::new(0.0, 0.0)).line_to(Point::new(10.0, 0.0)).line_to(Point::new(10.0, 10.0))"
        );
    }

    #[test]
    fn relative_commands_resolve_to_absolute() {
        let built = parse_path_data("m10,10 l10,0 l0,10 z").unwrap();
        assert_eq!(
            built.chain,
            "PathData::new().move_to(Point::new(10.0, 10.0)).line_to(Point::new(20.0, 10.0)).line_to(Point::new(20.0, 20.0)).close()"
        );
    }

    #[test]
    fn horizontal_and_vertical() {
        let built = parse_path_data("M0,0 H50 V50 h-50 v-50 Z").unwrap();
        assert_eq!(
            built.chain,
            "PathData::new().move_to(Point::new(0.0, 0.0)).line_to(Point::new(50.0, 0.0)).line_to(Point::new(50.0, 50.0)).line_to(Point::new(0.0, 50.0)).line_to(Point::new(0.0, 0.0)).close()"
        );
    }

    #[test]
    fn cubic_and_quadratic() {
        let built = parse_path_data("M0,0 C10,0 20,10 20,20 Q30,30 40,20").unwrap();
        assert!(built.chain.contains(
            ".cubic_to(Point::new(10.0, 0.0), Point::new(20.0, 10.0), Point::new(20.0, 20.0))"
        ));
        assert!(
            built
                .chain
                .contains(".quad_to(Point::new(30.0, 30.0), Point::new(40.0, 20.0))")
        );
    }

    #[test]
    fn negative_and_decimal_no_separator() {
        // `-` and `.` start a new number with no separator (`1.5.5` -> 1.5, .5).
        let built = parse_path_data("M1.5.5L-3-4").unwrap();
        assert_eq!(
            built.chain,
            "PathData::new().move_to(Point::new(1.5, 0.5)).line_to(Point::new(-3.0, -4.0))"
        );
    }

    #[test]
    fn missing_moveto_errors() {
        assert!(parse_path_data("L10,10").is_err());
    }

    #[test]
    fn unsupported_command_errors() {
        // Smooth curves / arcs are not supported and must report, not silently drop.
        assert!(parse_path_data("M0,0 A10,10 0 0 1 20,20").is_err());
    }
}
