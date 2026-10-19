//! Thin Loft: the loft hollowed to a wall of constant thickness, its end sections open.

use std::f64::consts::PI;

use super::*;

/// A sketch on the plane z = `z` with a circle of radius `r` (or, with `rect`, a rectangle
/// `2r` × `2h` about the origin).
fn section(s: &mut Session, name: &str, z: f64, r: f64, rect: Option<f64>) {
    run(s, "sketch.create", json!({"plane": {"origin": [0, 0, z], "x_dir": [1, 0, 0], "y_dir": [0, 1, 0]}, "name": name}));
    match rect {
        Some(h) => run(s, "sketch.rectangle.center", json!({"center": [0, 0], "corner": [r, h]})),
        None => run(s, "sketch.circle.center", json!({"center": [0, 0], "radius": r})),
    };
    run(s, "sketch.finish", json!({}));
}

fn frustum(r1: f64, r2: f64, h: f64) -> f64 {
    PI * h / 3.0 * (r1 * r1 + r1 * r2 + r2 * r2)
}

fn feature_error(s: &Session) -> Option<String> {
    s.model.results.iter().find_map(|r| r.error.clone())
}

/// A cone frustum from radius 30 at z = 0 to 15 at z = 50, thin with wall `t` on `side`.
fn cone(side: &str, extra: Value) -> Session {
    let mut s = Session::default();
    section(&mut s, "A", 0.0, 30.0, None);
    section(&mut s, "B", 50.0, 15.0, None);
    let mut p =
        json!({"sections": [{"sketch": "A"}, {"sketch": "B"}], "thin": true, "thickness": "2 mm", "side": side, "name": "Duct", "body_name": "Duct"});
    if let (Some(p), Some(e)) = (p.as_object_mut(), extra.as_object()) {
        p.extend(e.clone());
    }
    run(&mut s, "solid.loft", p);
    s
}

#[test]
fn a_thin_cone_loft_is_a_wall_open_at_both_ends() {
    // The wall is 2 mm across each section, in its plane.
    let k = 2.0;
    for (side, (o1, o2), (i1, i2)) in [
        ("inside", (30.0, 15.0), (30.0 - k, 15.0 - k)),
        ("outside", (30.0 + k, 15.0 + k), (30.0, 15.0)),
        ("both", (30.0 + k / 2.0, 15.0 + k / 2.0), (30.0 - k / 2.0, 15.0 - k / 2.0)),
    ] {
        let mut s = cone(side, json!({}));
        assert_eq!(feature_error(&s), None, "{side}");
        assert_eq!(s.model.state().bodies.len(), 1, "{side}");
        let want = frustum(o1, o2, 50.0) - frustum(i1, i2, 50.0);
        let got = volume(&mut s);
        assert!(rel(got, want) < 1e-2, "{side}: {got} vs {want}");
    }
}

#[test]
fn a_thin_loft_can_keep_one_end_closed() {
    let mut cup = cone("inside", json!({"closed_start": true}));
    assert_eq!(feature_error(&cup), None);
    // The inside starts 2 mm up: a floor as thick as the wall.
    let want = frustum(30.0, 15.0, 50.0) - frustum(28.0, 13.0, 48.0);
    let got = volume(&mut cup);
    assert!(rel(got, want) < 1e-2, "{got} vs {want}");
    // Both ends closed is refused before it is added.
    let mut s = Session::default();
    section(&mut s, "A", 0.0, 30.0, None);
    section(&mut s, "B", 50.0, 15.0, None);
    let p = json!({"sections": [{"sketch": "A"}, {"sketch": "B"}], "thin": true, "thickness": 2, "closed_start": true, "closed_end": true});
    assert!(s.execute("solid.loft", &p).is_err());
}

#[test]
fn a_thin_rectangle_to_circle_transition() {
    let transition = |thin: bool| {
        let mut s = Session::default();
        section(&mut s, "A", 0.0, 30.0, Some(20.0));
        section(&mut s, "B", 50.0, 15.0, None);
        let mut p = json!({"sections": [{"sketch": "A"}, {"sketch": "B"}]});
        if thin {
            p["thin"] = json!(true);
            p["thickness"] = json!(1.5);
        }
        run(&mut s, "solid.loft", p);
        s
    };
    let solid = volume(&mut transition(false));
    let mut s = transition(true);
    assert_eq!(feature_error(&s), None);
    let v = volume(&mut s);
    // The wall: about the side's area times the thickness, far less than the solid.
    assert!(v > 0.0 && v < solid * 0.4, "{v} of {solid}");
}

#[test]
fn a_thin_loft_saves_and_reopens() {
    let mut s = cone("both", json!({"closed_end": true}));
    let v = volume(&mut s);
    let path = std::env::temp_dir().join(format!("thin-loft-{}.json", std::process::id()));
    let path_s = path.to_string_lossy().to_string();
    run(&mut s, "file.save_as", json!({"path": path_s, "format": "json"}));
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(text.contains("\"thin\""), "{text}");
    let mut r = Session::default();
    run(&mut r, "doc.open", json!({"path": path_s}));
    let _ = std::fs::remove_file(&path);
    assert!(rel(volume(&mut r), v) < 1e-9);
}

#[test]
fn a_rectangle_to_circle_duct_is_open_and_its_corners_are_bent_round() {
    let duct = |sharp: bool| {
        let mut s = Session::default();
        section(&mut s, "A", 0.0, 30.0, Some(20.0));
        section(&mut s, "B", 50.0, 15.0, None);
        run(&mut s, "solid.loft", json!({"sections": [{"sketch": "A"}, {"sketch": "B"}], "thin": true, "thickness": 2, "sharp_corners": sharp}));
        assert_eq!(feature_error(&s), None, "sharp {sharp}");
        s
    };
    let (mut round, mut sharp) = (duct(false), duct(true));
    // Rounding the corners takes material off the outside.
    assert!(volume(&mut round) < volume(&mut sharp));
    // Both ends are open: no film across either end.
    for s in [&round, &sharp] {
        let st = s.model.state();
        let m = st.bodies[0].mesh();
        for z in [0.3, 49.7] {
            assert!(!m.contains(Vec3::new(0.0, 0.0, z)), "film at z {z}");
        }
    }
}
