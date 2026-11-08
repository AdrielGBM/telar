use super::*;

#[test]
fn identity_is_noop() {
    let p = Point::new(3.0, 4.0);
    assert_eq!(Transform::IDENTITY.apply(p), p);
}

#[test]
fn then_composes_in_application_order() {
    let translate = Transform {
        e: 10.0,
        ..Transform::IDENTITY
    };
    let scale = Transform {
        a: 2.0,
        d: 2.0,
        ..Transform::IDENTITY
    };
    let t = translate.then(scale);
    assert_eq!(t.apply(Point::new(0.0, 0.0)), Point::new(20.0, 0.0));
}

#[test]
fn rotate_around_keeps_center_fixed() {
    let c = Point::new(5.0, 5.0);
    let r = Transform::rotate_around(90.0, c.x, c.y).apply(c);
    assert!(
        (r.x - c.x).abs() < 1e-4 && (r.y - c.y).abs() < 1e-4,
        "rotating about a point leaves that point where it was: {r:?} vs {c:?}"
    );
}

#[test]
fn scale_around_keeps_center_fixed() {
    let c = Point::new(7.0, 2.0);
    let s = Transform::scale_around(3.0, 3.0, c.x, c.y).apply(c);
    assert!(
        (s.x - c.x).abs() < 1e-4 && (s.y - c.y).abs() < 1e-4,
        "scaling about a point leaves that point where it was: {s:?} vs {c:?}"
    );
}

#[test]
fn from_array_round_trips_to_array() {
    let t = Transform {
        a: 1.5,
        b: 0.5,
        c: -0.25,
        d: 2.0,
        e: 3.0,
        f: -4.0,
    };
    assert_eq!(Transform::from_array(t.to_array()), t);
}

#[test]
fn then_invert_is_identity() {
    let t = Transform {
        a: 1.5,
        b: 0.5,
        c: -0.25,
        d: 2.0,
        e: 3.0,
        f: -4.0,
    };
    let id = t.then(t.invert().unwrap());
    let approx = |x: f32, y: f32| (x - y).abs() < 1e-4;
    assert!(approx(id.a, 1.0), "{id:?}");
    assert!(approx(id.b, 0.0), "{id:?}");
    assert!(approx(id.c, 0.0), "{id:?}");
    assert!(approx(id.d, 1.0), "{id:?}");
    assert!(approx(id.e, 0.0), "{id:?}");
    assert!(approx(id.f, 0.0), "{id:?}");
}

#[test]
fn invert_singular_returns_none() {
    let singular = Transform {
        a: 0.0,
        b: 0.0,
        c: 0.0,
        d: 0.0,
        e: 5.0,
        f: 5.0,
    };
    assert!(
        singular.invert().is_none(),
        "a singular matrix has no inverse"
    );
}
