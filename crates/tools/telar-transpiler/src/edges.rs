//! The per-edge halves of the two box properties that carry one number per edge: `stroke_width` across the
//! four sides, and `radius` across the four corners.
//!
//! One module for both, because an author asking for a rule under a header and an author asking for a panel
//! rounded only at the top are asking the same question in the same grammar — a CSS shorthand, or a name per
//! edge. Only which edge a name lands on differs, which is the one thing each caller passes in.

use telar_parser::Attr;

use crate::style::format_number;

/// Where a suffixed attribute puts its value.
pub enum EdgeTarget {
    /// Indices into the property's own four slots.
    Slots(&'static [usize]),
    /// The edge the text comes from, resolved against the writing direction at paint time.
    Start,
    /// The edge the text runs towards.
    End,
}

impl EdgeTarget {
    /// How specifically this name picks its edges. A name for one edge outranks a name for a pair, which
    /// outranks the shorthand — so `radius:8 radius_top:0` is the same shape however the author ordered it.
    fn specificity(&self) -> u8 {
        match self {
            EdgeTarget::Slots(s) => 4 - s.len() as u8,
            EdgeTarget::Start | EdgeTarget::End => u8::MAX,
        }
    }
}

/// The suffixes of `stroke_*`, over sides ordered `[top, right, bottom, left]`.
pub fn side_target(suffix: &str) -> Option<EdgeTarget> {
    Some(match suffix {
        "top" => EdgeTarget::Slots(&[0]),
        "right" => EdgeTarget::Slots(&[1]),
        "bottom" => EdgeTarget::Slots(&[2]),
        "left" => EdgeTarget::Slots(&[3]),
        "x" => EdgeTarget::Slots(&[1, 3]),
        "y" => EdgeTarget::Slots(&[0, 2]),
        "start" => EdgeTarget::Start,
        "end" => EdgeTarget::End,
        _ => return None,
    })
}

/// The suffixes of `radius_*`, over corners ordered `[top_left, top_right, bottom_right, bottom_left]` — the
/// order `BorderRadius` declares them in, and the one CSS writes its shorthand in.
///
/// `top`/`start` and their opposites name a *side* here rather than a corner, since that is the shape the
/// property is actually asked for: a panel meeting a rail is rounded on the two corners facing away from it.
pub fn corner_target(suffix: &str) -> Option<EdgeTarget> {
    Some(match suffix {
        "top" => EdgeTarget::Slots(&[0, 1]),
        "bottom" => EdgeTarget::Slots(&[2, 3]),
        "left" => EdgeTarget::Slots(&[0, 3]),
        "right" => EdgeTarget::Slots(&[1, 2]),
        "top_left" => EdgeTarget::Slots(&[0]),
        "top_right" => EdgeTarget::Slots(&[1]),
        "bottom_right" => EdgeTarget::Slots(&[2]),
        "bottom_left" => EdgeTarget::Slots(&[3]),
        "start" => EdgeTarget::Start,
        "end" => EdgeTarget::End,
        _ => return None,
    })
}

/// A property's four edges as the author left them, with the two the writing direction still has to resolve
/// kept apart.
#[derive(Default)]
pub struct Edges {
    /// The value expression for each slot, or `None` where the author named no edge.
    pub slots: [Option<String>; 4],
    pub start: Option<String>,
    pub end: Option<String>,
    /// Set when the property still reduces to its one-value form — a bare shorthand of a single token and no
    /// edge named separately. This is what keeps `radius:8` emitting `BorderRadius::all(8.0)` and a plain
    /// `stroke_width:1` emitting no per-side data at all.
    pub uniform: Option<String>,
}

impl Edges {
    /// Whether the author named any edge at all, so a property with nothing written can fall back to its
    /// default rather than to four explicit zeroes.
    pub fn is_empty(&self) -> bool {
        self.uniform.is_none()
            && self.start.is_none()
            && self.end.is_none()
            && self.slots.iter().all(Option::is_none)
    }

    /// The four slots with `fallback` standing in for every edge the author did not name.
    ///
    /// Zero is the right fallback for both properties, and it is what CSS resolves to as well: a
    /// `border-bottom` on its own leaves the other three sides with no style and therefore no width.
    pub fn resolved(&self, fallback: &str) -> [String; 4] {
        std::array::from_fn(|i| {
            self.slots[i]
                .clone()
                .unwrap_or_else(|| fallback.to_string())
        })
    }

    /// `Some(expr)` / `None` rendered as the `Option<f32>` argument the logical helpers take.
    pub fn logical_args(&self) -> (String, String) {
        let arg = |v: &Option<String>| match v {
            Some(e) => format!("Some({e})"),
            None => "None".to_string(),
        };
        (arg(&self.start), arg(&self.end))
    }

    pub fn has_logical(&self) -> bool {
        self.start.is_some() || self.end.is_some()
    }
}

/// Expands a CSS-style shorthand into its four values, with CSS's own arities: one value for all four, two
/// for the two axes, three with the last axis' pair split, four verbatim. `None` for no tokens or more than
/// four, which leaves the value for the caller to reject.
pub fn expand_shorthand(value: &str) -> Option<[&str; 4]> {
    match value.split_whitespace().collect::<Vec<_>>().as_slice() {
        [a] => Some([a, a, a, a]),
        [a, b] => Some([a, b, a, b]),
        [a, b, c] => Some([a, b, c, b]),
        [a, b, c, d] => Some([a, b, c, d]),
        _ => None,
    }
}

/// Reads one property's edges out of an element's attributes: the bare shorthand under `base`, then every
/// `prefix`-suffixed name `target` recognises, applied most-general first.
pub fn collect(
    attrs: &[Attr],
    base: &str,
    prefix: &str,
    target: fn(&str) -> Option<EdgeTarget>,
    theme: Option<&str>,
) -> Edges {
    let mut named: Vec<(EdgeTarget, &str)> = attrs
        .iter()
        .filter_map(|a| {
            let suffix = a.key.strip_prefix(prefix)?;
            Some((target(suffix)?, a.value.text()))
        })
        .collect();
    named.sort_by_key(|(t, _)| t.specificity());

    let bare = attrs
        .iter()
        .find(|a| a.key == base)
        .map(|a| a.value.text().trim());
    let mut edges = Edges::default();

    // The one-value form survives only if it is the whole story: a single token, and no edge named apart from it.
    if let Some(value) = bare
        && named.is_empty()
        && value.split_whitespace().count() == 1
    {
        edges.uniform = Some(format_number(value, theme));
        return edges;
    }

    if let Some(values) = bare.and_then(expand_shorthand) {
        for (slot, value) in edges.slots.iter_mut().zip(values) {
            *slot = Some(format_number(value, theme));
        }
    }
    for (t, value) in named {
        let value = format_number(value.trim(), theme);
        match t {
            EdgeTarget::Slots(indices) => {
                for i in indices {
                    edges.slots[*i] = Some(value.clone());
                }
            }
            EdgeTarget::Start => edges.start = Some(value),
            EdgeTarget::End => edges.end = Some(value),
        }
    }
    edges
}

#[cfg(test)]
mod tests {
    use super::*;
    use telar_parser::Value;

    fn attr(key: &str, value: &str) -> Attr {
        Attr {
            key: key.to_string(),
            value: Value::Bare(value.to_string()),
            value_start: 0,
        }
    }

    #[test]
    fn shorthand_follows_the_css_arities() {
        assert_eq!(expand_shorthand("1"), Some(["1", "1", "1", "1"]));
        assert_eq!(expand_shorthand("1 2"), Some(["1", "2", "1", "2"]));
        assert_eq!(expand_shorthand("1 2 3"), Some(["1", "2", "3", "2"]));
        assert_eq!(expand_shorthand("1 2 3 4"), Some(["1", "2", "3", "4"]));
        assert_eq!(expand_shorthand(""), None);
        assert_eq!(expand_shorthand("1 2 3 4 5"), None);
    }

    /// The case the whole feature exists for, and the one that has to stay one attribute long.
    #[test]
    fn a_single_side_leaves_every_other_side_at_nothing() {
        let edges = collect(
            &[attr("stroke_width", "0 0 1 0")],
            "stroke_width",
            "stroke_",
            side_target,
            None,
        );
        assert!(
            edges.uniform.is_none(),
            "four tokens is not the uniform form"
        );
        assert_eq!(
            edges.resolved("0.0"),
            ["0.0", "0.0", "1.0", "0.0"].map(String::from)
        );
    }

    /// A plain width has to keep emitting nothing per-side, or every box in every existing app grows four
    /// numbers it never asked for.
    #[test]
    fn a_plain_width_stays_uniform() {
        let edges = collect(
            &[attr("stroke_width", "2")],
            "stroke_width",
            "stroke_",
            side_target,
            None,
        );
        assert_eq!(edges.uniform.as_deref(), Some("2.0"));
    }

    /// `stroke_width` itself starts with the side prefix, and `width` is not a side.
    #[test]
    fn the_base_key_is_not_mistaken_for_one_of_its_own_sides() {
        let edges = collect(
            &[attr("stroke_width", "2"), attr("stroke_bottom", "1")],
            "stroke_width",
            "stroke_",
            side_target,
            None,
        );
        assert_eq!(
            edges.resolved("0.0"),
            ["2.0", "2.0", "1.0", "2.0"].map(String::from),
            "the shorthand seeds all four and the named side overrides its own"
        );
    }

    /// Written the other way round, the result is the same: specificity decides, not source order.
    #[test]
    fn a_named_edge_beats_the_shorthand_whichever_came_first() {
        let written_backwards = collect(
            &[attr("radius_top", "0"), attr("radius", "8")],
            "radius",
            "radius_",
            corner_target,
            None,
        );
        assert_eq!(
            written_backwards.resolved("0.0"),
            ["0.0", "0.0", "8.0", "8.0"].map(String::from)
        );
    }

    #[test]
    fn a_pair_beats_the_shorthand_and_a_single_corner_beats_the_pair() {
        let edges = collect(
            &[
                attr("radius", "8"),
                attr("radius_top", "4"),
                attr("radius_top_left", "0"),
            ],
            "radius",
            "radius_",
            corner_target,
            None,
        );
        assert_eq!(
            edges.resolved("0.0"),
            ["0.0", "4.0", "8.0", "8.0"].map(String::from)
        );
    }

    #[test]
    fn logical_edges_are_kept_apart_for_the_direction_to_resolve() {
        let edges = collect(
            &[attr("stroke_end", "1")],
            "stroke_width",
            "stroke_",
            side_target,
            None,
        );
        assert!(edges.has_logical());
        assert_eq!(edges.logical_args(), ("None".into(), "Some(1.0)".into()));
    }

    #[test]
    fn nothing_written_is_empty() {
        let edges = collect(
            &[attr("fill", "ink")],
            "radius",
            "radius_",
            corner_target,
            None,
        );
        assert!(edges.is_empty());
    }
}
