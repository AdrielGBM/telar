use geometry_core::Rect;

pub fn union_rects(a: Rect, b: Rect) -> Rect {
    a.union(b)
}
