use super::*;

#[test]
fn rect_new_stores_fields() {
    let r = Rect::new(1.0, 2.0, 10.0, 20.0);
    assert_eq!(r.x, 1.0);
    assert_eq!(r.y, 2.0);
    assert_eq!(r.width, 10.0);
    assert_eq!(r.height, 20.0);
}

#[test]
fn rect_default_is_zero() {
    let r = Rect::default();
    assert_eq!(r.x, 0.0);
    assert_eq!(r.y, 0.0);
    assert_eq!(r.width, 0.0);
    assert_eq!(r.height, 0.0);
}

#[test]
fn rect_intersect_overlapping() {
    let a = Rect::new(0.0, 0.0, 10.0, 10.0);
    let b = Rect::new(5.0, 5.0, 10.0, 10.0);
    let result = a.intersect(b).unwrap();
    assert_eq!(result.x, 5.0);
    assert_eq!(result.y, 5.0);
    assert_eq!(result.width, 5.0);
    assert_eq!(result.height, 5.0);
}

#[test]
fn rect_intersect_non_overlapping() {
    let a = Rect::new(0.0, 0.0, 5.0, 5.0);
    let b = Rect::new(10.0, 10.0, 5.0, 5.0);
    assert!(
        a.intersect(b).is_none(),
        "rects that do not overlap have no intersection"
    );
}
