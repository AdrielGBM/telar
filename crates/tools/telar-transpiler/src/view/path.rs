//! Declarative vector-path emitter: a `path d:"…"` tag whose SVG path-data string is parsed at compile time into a `PathData` builder chain.
//!
//! Every `path` is a `Path` widget whose closure is handed the laid-out rect each paint, to fit a `view_box` into it; one whose paint reads the text it is drawn in is `Path::declaring`, handed the inherited text style as well.

use telar_parser::Element;

use crate::style::format_f32;
use crate::theme_access::TokensRead;

use super::{ChildEmit, ViewGen};

/// The keys that paint a `path` rather than shape or place it: what reads the text the path is drawn in.
const PAINT_KEYS: [&str; 3] = ["fill", "stroke", "stroke_width"];

/// Whether `tag`'s `key` may read `$text` as the text a `path` is drawn in.
pub(super) fn paints_in_its_text(tag: &str, key: &str) -> bool {
    tag == "path" && PAINT_KEYS.contains(&key)
}

impl ViewGen<'_> {
    pub(super) fn emit_path(&mut self, el: &Element) -> ChildEmit {
        let var = self.next_variable_name("path");
        let pad = self.indent_str();
        let attr = |key: &str| el.attributes.iter().find(|a| a.key == key);

        let mut errors: Vec<String> = Vec::new();
        // A missing or invalid `d` becomes a `compile_error!` on this element's line, via the source map.
        let (data_chain, extent) = match attr("d") {
            Some(a) => match parse_path_data(a.value.text()) {
                Ok(built) => (built.chain, Some((built.max_x, built.max_y))),
                Err(e) => {
                    errors.push(format!("rsx: path `d` — {e}"));
                    ("PathData::new()".to_string(), None)
                }
            },
            None => {
                errors.push("rsx: a `path` needs a `d:` attribute with SVG path data".to_string());
                ("PathData::new()".to_string(), None)
            }
        };
        let view_box = match attr("view_box").map(|a| parse_view_box(a.value.text())) {
            Some(Ok(view_box)) => Some(view_box),
            Some(Err(e)) => {
                errors.push(e);
                None
            }
            None => None,
        };
        let stroke_calls = match stroke_calls(el) {
            Ok(calls) => calls,
            Err(e) => {
                errors.push(e);
                String::new()
            }
        };
        let path_style = self.path_style_expr(el, &stroke_calls);

        let mut layout = self.make_layout_style("path", &el.classes, &el.attributes);
        layout.push_str(&self.natural_size(el, view_box, extent));

        // Cloned into the paint closure, which re-reads it each frame, leaving the outer binding usable.
        let raw_paint: Vec<&str> = el
            .attributes
            .iter()
            .filter(|a| PAINT_KEYS.contains(&a.key.as_str()))
            .map(|a| a.value.text())
            .collect();
        let (rect, data) = match view_box {
            Some([x, y, width, height]) => (
                "__rect",
                format!(
                    "std::sync::Arc::new(__path_data.fitted(Rect::new({}, {}, {}, {}), __rect))",
                    format_f32(x),
                    format_f32(y),
                    format_f32(width),
                    format_f32(height)
                ),
            ),
            None => ("_", "__path_data.clone()".to_string()),
        };
        // Only a path that reads its text resolves the inherited style, so one that does not neither walks the tree each paint nor repaints when an ancestor declares new text.
        let (constructor, draw) = match crate::style::reads_box_text(&path_style) {
            true => (
                "declaring",
                format!(
                    "move |{rect}: Rect, __text_style: &TextStyle| {{ let {} = Reactive::Const(BoxText::drawn_in(__text_style)); ({data}, {path_style}) }}",
                    crate::style::BOX_TEXT_BINDING
                ),
            ),
            false => ("new", format!("move |{rect}: Rect| ({data}, {path_style})")),
        };
        let widget = format!(
            "Path::{constructor}({layout}, {})",
            self.owning(&raw_paint, &draw)
        );

        let err_lines: String = errors
            .iter()
            .map(|e| format!("{pad}    compile_error!({e:?});\n"))
            .collect();
        let code = format!(
            "{pad}let {var} = {{\n\
             {err_lines}\
             {pad}    let __path_data = std::sync::Arc::new({data_chain});\n\
             {pad}    {widget}?\n\
             {pad}}};"
        );
        ChildEmit::Simple { name: var, code }
    }

    /// The size a path's box takes where neither its attributes nor its classes give one, so it does not collapse to zero: the view box's, keeping its proportions when only one side is given, or else the extent the shape reaches.
    fn natural_size(
        &self,
        el: &Element,
        view_box: Option<[f32; 4]>,
        extent: Option<(f32, f32)>,
    ) -> String {
        let declared = |keys: &[&str]| {
            el.attributes.iter().any(|a| keys.contains(&a.key.as_str()))
                || el.classes.iter().any(|name| {
                    self.classes
                        .iter()
                        .filter(|class| &class.name == name)
                        .any(|class| class.props.iter().any(|p| keys.contains(&p.key.as_str())))
                })
        };
        let (width, height) = (declared(&["width"]), declared(&["height"]));
        let (natural_width, natural_height) = match (view_box, extent) {
            (Some([_, _, w, h]), _) => (w, h),
            (None, Some(extent)) => extent,
            (None, None) => return String::new(),
        };
        if view_box.is_some() && width != height {
            return match declared(&["aspect", "aspect_ratio"]) {
                true => String::new(),
                false => format!(
                    ".aspect_ratio({})",
                    format_f32(natural_width / natural_height)
                ),
            };
        }
        let mut size = String::new();
        if !width && natural_width > 0.0 {
            size.push_str(&format!(".width({})", format_f32(natural_width)));
        }
        if !height && natural_height > 0.0 {
            size.push_str(&format!(".height({})", format_f32(natural_height)));
        }
        size
    }

    /// Builds the `PathStyle { … }` literal from `fill`/`stroke`/`stroke_width`/`fill_rule` attributes, with `stroke_calls` chained onto the stroke. Colours reuse `color_expr` (theme token / `#hex` / `$signal`), matching `box fill:`.
    fn path_style_expr(&self, el: &Element, stroke_calls: &str) -> String {
        let fill = el
            .attributes
            .iter()
            .find(|a| a.key == "fill")
            .map(|a| {
                format!(
                    "Some(Paint::Solid({}))",
                    self.color_expr(a.value.text(), Some(a.value_start))
                )
            })
            .unwrap_or_else(|| "None".to_string());
        let stroke = match el.attributes.iter().find(|a| a.key == "stroke") {
            Some(a) => {
                let color = self.color_expr(a.value.text(), Some(a.value_start));
                let theme_width = TokensRead::Token {
                    name: "border_width_strong",
                    called: false,
                }
                .emit("");
                let width = el
                    .attributes
                    .iter()
                    .find(|a| a.key == "stroke_width")
                    .map(|a| a.value.text().trim().to_string())
                    .filter(|value| !value.is_empty())
                    .map(|value| crate::style::number_or(&value, &theme_width, self.theme_access))
                    .unwrap_or(theme_width);
                format!("Some(Stroke::new({color}, {width}){stroke_calls})")
            }
            None => "None".to_string(),
        };
        let fill_rule = el
            .attributes
            .iter()
            .find(|a| a.key == "fill_rule")
            .and_then(|a| {
                crate::registry::keyword(crate::registry::FILL_RULE_VALUES, a.value.text().trim())
            })
            .unwrap_or("FillRule::Winding");
        format!(
            "PathStyle {{ fill: {fill}, stroke: {stroke}, shadow: None, fill_rule: {fill_rule} }}"
        )
    }
}

/// Parses `view_box:"min_x min_y width height"`, separated by spaces or commas as SVG writes it, into its four numbers.
fn parse_view_box(value: &str) -> Result<[f32; 4], String> {
    let numbers = value
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|part| !part.is_empty())
        .map(|part| part.parse::<f32>().ok().filter(|n| n.is_finite()))
        .collect::<Option<Vec<f32>>>();
    match numbers.as_deref() {
        Some(&[x, y, width, height]) if width > 0.0 && height > 0.0 => Ok([x, y, width, height]),
        Some(&[_, _, _, _]) => Err(format!(
            "rsx: `view_box:\"{value}\"` has no area to scale from: its width and height must be more than zero"
        )),
        _ => Err(format!(
            "rsx: `view_box` is four numbers, `\"min_x min_y width height\"`, got `{value}`"
        )),
    }
}

/// What chains onto the stroke: the `.with_dash(…)` call `stroke_dash:` asks for and the `.with_cap(…)` call `stroke_cap:` does, or why one of them cannot be.
fn stroke_calls(el: &Element) -> Result<String, String> {
    let dash = stroke_dash_call(el)?;
    let Some(cap) = el.attributes.iter().find(|a| a.key == "stroke_cap") else {
        return Ok(dash);
    };
    if !el.attributes.iter().any(|a| a.key == "stroke") {
        return Err(
            "rsx: `stroke_cap` shapes the ends of the `stroke`, and this path has none".to_string(),
        );
    }
    // A word that is not a cap is refused with the other values on the line, so it adds nothing here.
    Ok(
        match crate::registry::keyword(crate::registry::STROKE_CAP_VALUES, cap.value.text().trim())
        {
            Some(cap) => format!("{dash}.with_cap({cap})"),
            None => dash,
        },
    )
}

/// The `.with_dash(…)` call `stroke_dash:` and `stroke_dash_offset:` ask for, empty when there is no pattern, or why the pattern cannot be one.
fn stroke_dash_call(el: &Element) -> Result<String, String> {
    let value = |key: &str| {
        el.attributes
            .iter()
            .find(|a| a.key == key)
            .map(|a| a.value.text().trim().to_string())
    };
    let Some(pattern) = value("stroke_dash") else {
        return match value("stroke_dash_offset") {
            Some(_) => Err("rsx: `stroke_dash_offset` needs a `stroke_dash` pattern".to_string()),
            None => Ok(String::new()),
        };
    };
    if value("stroke").is_none() {
        return Err("rsx: `stroke_dash` dashes the `stroke`, and this path has none".to_string());
    }
    let lengths = parse_dash_pattern(&pattern)?;
    let offset = match value("stroke_dash_offset") {
        Some(offset) => offset
            .parse::<f32>()
            .ok()
            .filter(|n| n.is_finite())
            .ok_or_else(|| format!("rsx: `stroke_dash_offset` must be a number, got `{offset}`"))?,
        None => 0.0,
    };
    let lengths: Vec<String> = lengths.into_iter().map(format_f32).collect();
    Ok(format!(
        ".with_dash(&[{}], {})",
        lengths.join(", "),
        format_f32(offset)
    ))
}

/// The most lengths a pattern may hold once an odd list is repeated: `Dash::CAPACITY` in renderer-core, which this crate does not depend on.
const DASH_CAPACITY: usize = 16;

/// Parses a dash pattern the way SVG writes `stroke-dasharray`: lengths separated by spaces or commas, drawn first. The same patterns `Dash::new` accepts, so a pattern that would silently draw a solid stroke is a build error instead.
fn parse_dash_pattern(pattern: &str) -> Result<Vec<f32>, String> {
    let lengths = pattern
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|part| !part.is_empty())
        .map(|part| {
            part.parse::<f32>()
                .ok()
                .filter(|n| n.is_finite() && *n >= 0.0)
                .ok_or_else(|| {
                    format!("rsx: `stroke_dash` lengths are numbers of zero or more, got `{part}`")
                })
        })
        .collect::<Result<Vec<f32>, String>>()?;
    if lengths.is_empty() {
        return Err(
            "rsx: `stroke_dash` needs at least one length, like `stroke_dash:\"1 4\"`".to_string(),
        );
    }
    if lengths.iter().all(|n| *n == 0.0) {
        return Err("rsx: `stroke_dash` draws nothing when every length is zero".to_string());
    }
    let repeated = if lengths.len() % 2 == 1 {
        lengths.len() * 2
    } else {
        lengths.len()
    };
    if repeated > DASH_CAPACITY {
        return Err(format!(
            "rsx: `stroke_dash` holds at most {DASH_CAPACITY} lengths (an odd list counts twice), got {}",
            lengths.len()
        ));
    }
    Ok(lengths)
}

/// A parsed path: the `PathData::new()…` builder-chain expression plus the maximum x/y extent reached (including Bézier control points), used to size the path's box when neither width/height nor a view box does.
struct PathBuild {
    chain: String,
    max_x: f32,
    max_y: f32,
}

/// Parses an SVG path-data string (`d`) into an absolute `PathData` builder chain. Supported commands, absolute (uppercase) and relative (lowercase): `M`/`m` (moveto), `L`/`l` (lineto), `H`/`h` (horizontal lineto), `V`/`v` (vertical lineto), `C`/`c` (cubic Bézier), `Q`/`q` (quadratic Bézier), `A`/`a` (elliptical arc), `Z`/`z` (closepath). Relative coordinates, `H`/`V` and arcs are resolved to absolute points at parse time so the emitted chain always uses absolute `move_to`/`line_to`/`quad_to`/`cubic_to`. Unsupported commands (`S`/`T`) and malformed input return an error.
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
            b'A' => {
                let mut drawn = false;
                while cur.peek_is_number() {
                    let radii = cur.pair()?;
                    let rotation = cur.number()?;
                    let large_arc = cur.flag()?;
                    let sweep = cur.flag()?;
                    let (mut x, mut y) = cur.pair()?;
                    if rel {
                        x += cx;
                        y += cy;
                    }
                    let arc = EllipticalArc {
                        from: (cx, cy),
                        radii,
                        rotation,
                        large_arc,
                        sweep,
                        to: (x, y),
                    };
                    for segment in arc.segments() {
                        match segment {
                            Segment::Line((x, y)) => {
                                ext.add(x, y);
                                chain.push_str(&format!(
                                    ".line_to(Point::new({}, {}))",
                                    fx(x),
                                    fx(y)
                                ));
                            }
                            Segment::Cubic((x1, y1), (x2, y2), (x, y)) => {
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
                            }
                        }
                    }
                    cx = x;
                    cy = y;
                    drawn = true;
                }
                if !drawn {
                    return Err(
                        "`A` needs seven values (two radii, a rotation, the large-arc and sweep flags, and an endpoint)"
                            .to_string(),
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
                    "unsupported command `{}` (supported: M L H V C Q A Z, absolute or relative)",
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

/// An elliptical arc as SVG's `A` command writes it: from the current point to `to`, on an ellipse of `radii` turned by `rotation` degrees, the flags choosing which of the four arcs through both points is meant.
struct EllipticalArc {
    from: (f32, f32),
    radii: (f32, f32),
    rotation: f32,
    large_arc: bool,
    sweep: bool,
    to: (f32, f32),
}

/// One absolute piece of a drawn arc.
#[derive(Debug, PartialEq)]
enum Segment {
    Line((f32, f32)),
    Cubic((f32, f32), (f32, f32), (f32, f32)),
}

impl EllipticalArc {
    /// The arc as cubic Béziers of at most a quarter turn each, worked out from its centre as SVG's implementation notes do: a zero radius draws a straight line and an arc that ends where it starts draws nothing, and radii too small to reach the endpoint are scaled up until they do.
    fn segments(&self) -> Vec<Segment> {
        use std::f64::consts::{FRAC_PI_2, TAU};
        let (x1, y1) = (f64::from(self.from.0), f64::from(self.from.1));
        let (x2, y2) = (f64::from(self.to.0), f64::from(self.to.1));
        if x1 == x2 && y1 == y2 {
            return Vec::new();
        }
        let (mut rx, mut ry) = (f64::from(self.radii.0).abs(), f64::from(self.radii.1).abs());
        if rx == 0.0 || ry == 0.0 {
            return vec![Segment::Line(self.to)];
        }
        let (sin, cos) = f64::from(self.rotation).to_radians().sin_cos();
        let (hx, hy) = ((x1 - x2) / 2.0, (y1 - y2) / 2.0);
        let (x1p, y1p) = (cos * hx + sin * hy, -sin * hx + cos * hy);
        let reach = (x1p * x1p) / (rx * rx) + (y1p * y1p) / (ry * ry);
        if reach > 1.0 {
            rx *= reach.sqrt();
            ry *= reach.sqrt();
        }
        let spare = rx * rx * ry * ry - rx * rx * y1p * y1p - ry * ry * x1p * x1p;
        let spread = rx * rx * y1p * y1p + ry * ry * x1p * x1p;
        let side = if self.large_arc == self.sweep {
            -1.0
        } else {
            1.0
        };
        let coef = side * (spare / spread).max(0.0).sqrt();
        let (cxp, cyp) = (coef * rx * y1p / ry, -coef * ry * x1p / rx);
        let centre = (
            cos * cxp - sin * cyp + (x1 + x2) / 2.0,
            sin * cxp + cos * cyp + (y1 + y2) / 2.0,
        );
        let start = ((y1p - cyp) / ry).atan2((x1p - cxp) / rx);
        let end = ((-y1p - cyp) / ry).atan2((-x1p - cxp) / rx);
        let mut turn = end - start;
        if self.sweep && turn < 0.0 {
            turn += TAU;
        } else if !self.sweep && turn > 0.0 {
            turn -= TAU;
        }
        // Rounding can leave an exact quarter turn a hair over, which would split it in two.
        let count = ((turn.abs() / FRAC_PI_2) - 1e-9).ceil().max(1.0) as usize;
        let step = turn / count as f64;
        let handle = 4.0 / 3.0 * (step / 4.0).tan();
        let on_ellipse = |ux: f64, uy: f64| {
            (
                (centre.0 + rx * cos * ux - ry * sin * uy) as f32,
                (centre.1 + rx * sin * ux + ry * cos * uy) as f32,
            )
        };
        (0..count)
            .map(|i| {
                let (s1, c1) = (start + step * i as f64).sin_cos();
                let (s2, c2) = (start + step * (i + 1) as f64).sin_cos();
                let end = match i + 1 == count {
                    true => self.to,
                    false => on_ellipse(c2, s2),
                };
                Segment::Cubic(
                    on_ellipse(c1 - handle * s1, s1 + handle * c1),
                    on_ellipse(c2 + handle * s2, s2 - handle * c2),
                    end,
                )
            })
            .collect()
    }
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

/// A byte cursor over the path-data string with SVG-flavored number scanning (signs, decimals, and exponents; commas and whitespace are interchangeable separators).
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

    /// Reads an arc flag. A flag is always one digit, so SVG lets the flags and the number after them run together (`a5 5 0 1010 10`).
    fn flag(&mut self) -> Result<bool, String> {
        self.skip_sep();
        let flag = match self.peek() {
            Some(b'0') => false,
            Some(b'1') => true,
            _ => return Err("an arc's large-arc and sweep flags are each `0` or `1`".to_string()),
        };
        self.bump();
        Ok(flag)
    }
}

#[cfg(test)]
#[path = "path.test.rs"]
mod tests;
