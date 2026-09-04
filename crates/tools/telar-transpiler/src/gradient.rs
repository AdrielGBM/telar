//! The `linear(…)` / `radial(…)` value a `fill:` may take.
//!
//! One value where six keys used to be — `gradient` naming the shape, `from`/`mid`/`to` the colours, `mid_pos` one of their positions and `radial_radius` the size. Three of those sat among the colour keys where they read as independent properties a box might have, and none of them meant anything without the other five.
//!
//! The shape is CSS's: an optional leading modifier, then the stops. `linear(a, b)` runs top to bottom the way `linear-gradient` does; `linear(horizontal, a, b)` names the axis; `radial(70, a, b)` names the radius. A stop is a colour, or a colour and where it sits — `linear(a, b 0.3, c)` — and a stop with no position of its own takes its share of the run, which is what `mid_pos` defaulting to `0.5` was.

/// A gradient value, resolved but for its colours: the caller still has to put each stop's colour through `color_expr`, which is the one thing this module cannot do without the view's scope.
pub(crate) struct Gradient<'a> {
    pub(crate) shape: Shape,
    /// `(position, the colour as written)`, in order.
    pub(crate) stops: Vec<(f32, &'a str)>,
}

pub(crate) enum Shape {
    /// The two endpoints of the run, as an expression over the paint closure's rect `r`.
    Linear(String),
    /// The radius, as an expression over `r`.
    Radial(String),
}

/// Splits `linear(…)` or `radial(…)` into its name and its argument text, or `None` for anything else — a plain colour, or a call that is the author's own Rust.
pub(crate) fn split_call(value: &str) -> Option<(&str, &str)> {
    let v = value.trim();
    let open = v.find('(')?;
    let name = &v[..open];
    if name != "linear" && name != "radial" {
        return None;
    }
    Some((name, v.strip_suffix(')')?.get(open + 1..)?))
}

pub(crate) fn parse<'a>(kind: &str, args: &'a str) -> Option<Gradient<'a>> {
    let mut parts: Vec<&str> = split_top_level(args);
    if parts.len() < 2 {
        return None;
    }
    let shape = match kind {
        "linear" => Shape::Linear(match parts[0] {
            "horizontal" => {
                parts.remove(0);
                "Point::new(r.x, r.y + r.height * 0.5), Point::new(r.x + r.width, r.y + r.height * 0.5)"
            }
            "diagonal" => {
                parts.remove(0);
                "Point::new(r.x, r.y), Point::new(r.x + r.width, r.y + r.height)"
            }
            "vertical" => {
                parts.remove(0);
                VERTICAL
            }
            _ => VERTICAL,
        }
        .to_string()),
        // A bare number leading the stops is the radius; without one the run reaches half the shorter side.
        "radial" => Shape::Radial(match parts[0].parse::<f32>() {
            Ok(px) => {
                parts.remove(0);
                crate::style::format_f32(px)
            }
            Err(_) => "r.width.min(r.height) * 0.5".to_string(),
        }),
        _ => return None,
    };
    if parts.len() < 2 {
        return None;
    }
    Some(Gradient {
        shape,
        stops: stops(&parts)?,
    })
}

const VERTICAL: &str =
    "Point::new(r.x + r.width * 0.5, r.y), Point::new(r.x + r.width * 0.5, r.y + r.height)";

/// Each stop's colour and where it sits. A stop that names no position of its own takes an even share of the run, which is what two colours at 0 and 1 — or three at 0, 0.5 and 1 — always were.
fn stops<'a>(parts: &[&'a str]) -> Option<Vec<(f32, &'a str)>> {
    let last = parts.len() - 1;
    parts
        .iter()
        .enumerate()
        .map(|(i, part)| match position(part) {
            Some((color, pos)) => Some((pos, color)),
            None => Some((i as f32 / last as f32, *part)),
        })
        .collect()
}

/// A stop's trailing position, when it has one. The space it splits on has to be outside any parentheses, or a colour that is itself a call — `chip(a, b)` — would be read as a colour and a position.
fn position(stop: &str) -> Option<(&str, f32)> {
    let (color, pos) = stop.rsplit_once(' ')?;
    let balanced =
        color.chars().filter(|c| *c == '(').count() == color.chars().filter(|c| *c == ')').count();
    balanced.then(|| pos.trim().parse::<f32>().ok().map(|p| (color.trim(), p)))?
}

/// Splits on commas outside parentheses, so a colour that is itself a call reads whole.
fn split_top_level(args: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    for (i, c) in args.char_indices() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(args[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(args[start..].trim());
    parts.retain(|p| !p.is_empty());
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stops_of(value: &str) -> Vec<(f32, String)> {
        let (kind, args) = split_call(value).expect("a gradient call");
        parse(kind, args)
            .expect("a gradient")
            .stops
            .into_iter()
            .map(|(p, c)| (p, c.to_string()))
            .collect()
    }

    /// Two colours run the whole way, which is what `from`/`to` were.
    #[test]
    fn two_stops_sit_at_the_ends() {
        assert_eq!(
            stops_of("linear(#fff, #000)"),
            vec![(0.0, "#fff".into()), (1.0, "#000".into())]
        );
    }

    /// Three colours evenly spaced is `mid` with `mid_pos` left at its default, spelled by saying nothing.
    #[test]
    fn an_unpositioned_stop_takes_an_even_share() {
        assert_eq!(
            stops_of("linear(a, b, c)"),
            vec![(0.0, "a".into()), (0.5, "b".into()), (1.0, "c".into())]
        );
    }

    #[test]
    fn a_stop_may_name_where_it_sits() {
        assert_eq!(
            stops_of("linear(a, b 0.45, c)"),
            vec![(0.0, "a".into()), (0.45, "b".into()), (1.0, "c".into())]
        );
    }

    /// The modifier is the first argument or absent, and a colour is never mistaken for one.
    #[test]
    fn a_leading_modifier_is_not_a_stop() {
        assert_eq!(stops_of("linear(horizontal, a, b)").len(), 2);
        assert_eq!(stops_of("radial(70, a, b)").len(), 2);
        assert_eq!(stops_of("radial(a, b)").len(), 2);
    }

    /// A colour that is itself a call has commas of its own, and they are not stop separators.
    #[test]
    fn a_call_valued_stop_reads_whole() {
        assert_eq!(
            stops_of("linear(chip(a, b), #000)"),
            vec![(0.0, "chip(a, b)".into()), (1.0, "#000".into())]
        );
    }

    #[test]
    fn a_gradient_needs_two_stops() {
        let (kind, args) = split_call("linear(#fff)").expect("a call");
        assert!(parse(kind, args).is_none());
        assert!(split_call("chip_fill($snap, id)").is_none());
    }
}
