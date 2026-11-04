//! `joint.constrain`: Mate, Flush, Angle, Tangent and Insert picked on faces and edges, each a
//! joint that moves the part no further than it needs.

use serde_json::{Value, json};
use solidcraft_geom::Vec3;

use super::tests::{occ, place, run};
use crate::Session;

/// A grounded 100 x 60 x 10 plate (component Base) with a Ø10 hole through it at (50, 30), a
/// 20 mm cube (Block) at (200, 100, 50)..(220, 120, 70), and a Ø10 x 30 pin (Pin) standing at
/// (300, 0, 0)..(300, 0, 30).
fn parts() -> Session {
    let mut s = Session::default();
    run(&mut s, "component.create", json!({"name": "Base"}));
    run(&mut s, "solid.box", json!({"length": 100, "width": 60, "height": 10, "corner": [0, 0, 0]}));
    run(&mut s, "sketch.create", json!({"plane": {"origin": [0, 0, 10], "x_dir": [1, 0, 0], "y_dir": [0, 1, 0]}, "name": "Hole"}));
    run(&mut s, "sketch.circle.center", json!({"center": [50, 30], "radius": 5}));
    run(&mut s, "sketch.finish", json!({}));
    run(&mut s, "solid.extrude", json!({"sketch": "Hole", "through_all": true, "direction": "symmetric", "operation": "cut"}));
    run(&mut s, "component.activate", json!({"component": "root"}));
    run(&mut s, "component.create", json!({"name": "Block"}));
    run(&mut s, "solid.box", json!({"length": 20, "width": 20, "height": 20, "corner": [200, 100, 50]}));
    run(&mut s, "component.activate", json!({"component": "root"}));
    run(&mut s, "component.create", json!({"name": "Pin"}));
    run(&mut s, "sketch.create", json!({"plane": "XY", "name": "PinSketch"}));
    run(&mut s, "sketch.circle.center", json!({"center": [300, 0], "radius": 5}));
    run(&mut s, "sketch.finish", json!({}));
    run(&mut s, "solid.extrude", json!({"sketch": "PinSketch", "distance": 30}));
    run(&mut s, "component.activate", json!({"component": "root"}));
    let base = occ(&s, "Base");
    run(&mut s, "occurrence.ground", json!({"occurrence": base, "grounded": true}));
    s
}

fn pick(component: &str, key: &str, at: [f64; 3]) -> Value {
    json!({"occurrence": component, key: at})
}

fn dof(r: &Value, s: &Session, component: &str) -> Option<u64> {
    let o = occ(s, component);
    r["dof"].as_array()?.iter().find(|d| d["occurrence"] == json!(o))?["dof"].as_u64()
}

fn clean(r: &Value) -> bool {
    r["conflicts"] == json!([]) && r["errors"] == json!([])
}

fn near(a: Vec3, b: Vec3) -> bool {
    a.dist(b) < 1e-6
}

/// Mate: the block's bottom face lands on the plate's top face where the block is (it does not
/// slide to the plate's centre), with 3 freedoms left; an offset leaves that gap.
#[test]
fn a_mate_puts_one_face_on_the_other_where_the_part_is() {
    let mut s = parts();
    let r = run(
        &mut s,
        "joint.constrain",
        json!({"type": "mate", "a": pick("Base", "face", [20.0, 20.0, 10.0]), "b": pick("Block", "face", [210.0, 110.0, 50.0])}),
    );
    assert!(clean(&r), "{r}");
    assert_eq!(r["kind"], "planar");
    assert_eq!(r["name"], "Mate:1");
    assert_eq!(dof(&r, &s, "Block"), Some(3), "{r}");
    let bottom = place(&s, "Block", Vec3::new(210.0, 110.0, 50.0));
    assert!((bottom.z - 10.0).abs() < 1e-6, "on the plate's top: {bottom:?}");
    assert!((bottom.x - 210.0).abs() < 1e-6 && (bottom.y - 110.0).abs() < 1e-6, "where it was across the face: {bottom:?}");
    let top = place(&s, "Block", Vec3::new(210.0, 110.0, 70.0));
    assert!((top.z - 30.0).abs() < 1e-6, "standing on it, not under it: {top:?}");
    run(&mut s, "joint.delete", json!({"joint": "Mate:1"}));
    // The block now stands on the plate: its bottom face is picked where it is.
    let at = place(&s, "Block", Vec3::new(210.0, 110.0, 50.0));
    run(
        &mut s,
        "joint.constrain",
        json!({"type": "mate", "offset": 5, "a": pick("Base", "face", [20.0, 20.0, 10.0]), "b": pick("Block", "face", [at.x, at.y, at.z])}),
    );
    assert!((place(&s, "Block", Vec3::new(210.0, 110.0, 50.0)).z - 15.0).abs() < 1e-6, "5 mm off");
}

/// Flush: the block's top face lies in the plate's top face's plane, pointing the same way.
#[test]
fn a_flush_lines_two_faces_up_the_same_way() {
    let mut s = parts();
    let r = run(
        &mut s,
        "joint.constrain",
        json!({"type": "flush", "a": pick("Base", "face", [20.0, 20.0, 10.0]), "b": pick("Block", "face", [210.0, 110.0, 70.0])}),
    );
    assert!(clean(&r), "{r}");
    let top = place(&s, "Block", Vec3::new(210.0, 110.0, 70.0));
    let bottom = place(&s, "Block", Vec3::new(210.0, 110.0, 50.0));
    assert!((top.z - 10.0).abs() < 1e-6 && (bottom.z + 10.0).abs() < 1e-6, "top flush at 10, the block below it: {top:?} {bottom:?}");
}

/// Insert: the pin's bottom rim goes into the hole's top rim: on the hole's axis, standing on
/// the plate, free only to turn.
#[test]
fn an_insert_puts_a_pin_in_a_hole_against_its_face() {
    let mut s = parts();
    let r = run(
        &mut s,
        "joint.constrain",
        json!({"type": "insert", "a": pick("Base", "circle", [55.0, 30.0, 10.0]), "b": pick("Pin", "circle", [305.0, 0.0, 0.0])}),
    );
    assert!(clean(&r), "{r}");
    assert_eq!(r["kind"], "revolute");
    assert_eq!(dof(&r, &s, "Pin"), Some(1), "{r}");
    let (foot, head) = (place(&s, "Pin", Vec3::new(300.0, 0.0, 0.0)), place(&s, "Pin", Vec3::new(300.0, 0.0, 30.0)));
    assert!(near(foot, Vec3::new(50.0, 30.0, 10.0)), "its foot on the hole's rim: {foot:?}");
    assert!(near(head, Vec3::new(50.0, 30.0, 40.0)), "standing up out of the plate: {head:?}");
}

/// Mate of two axes: a round face picked on the pin and on the hole's wall puts the pin on the
/// hole's axis, free to slide and turn.
#[test]
fn a_mate_of_two_round_faces_puts_them_on_one_axis() {
    let mut s = parts();
    let r = run(
        &mut s,
        "joint.constrain",
        json!({"type": "mate", "a": pick("Base", "face", [55.0, 30.0, 5.0]), "b": pick("Pin", "face", [305.0, 0.0, 15.0])}),
    );
    assert!(clean(&r), "{r}");
    assert_eq!(r["kind"], "cylindrical");
    assert_eq!(dof(&r, &s, "Pin"), Some(2), "{r}");
    let (foot, head) = (place(&s, "Pin", Vec3::new(300.0, 0.0, 0.0)), place(&s, "Pin", Vec3::new(300.0, 0.0, 30.0)));
    for p in [foot, head] {
        assert!((p.x - 50.0).abs() < 1e-6 && (p.y - 30.0).abs() < 1e-6, "on the hole's axis: {p:?}");
    }
}

/// Angle: 90° between the plate's top and the block's side face stands that face up flat.
#[test]
fn an_angle_turns_one_face_to_the_other() {
    let mut s = parts();
    let r = run(
        &mut s,
        "joint.constrain",
        json!({"type": "angle", "angle": 90, "a": pick("Base", "face", [20.0, 20.0, 10.0]), "b": pick("Block", "face", [210.0, 110.0, 50.0])}),
    );
    assert!(clean(&r), "{r}");
    let (c, n) = (place(&s, "Block", Vec3::new(210.0, 110.0, 50.0)), place(&s, "Block", Vec3::new(210.0, 110.0, 49.0)));
    let normal = n - c;
    assert!(normal.z.abs() < 1e-6, "the block's bottom normal now lies flat: {normal:?}");
}

/// Tangent: the pin's round side touches the plate's top, its axis a radius above it.
#[test]
fn a_tangent_lays_a_cylinder_on_a_face() {
    let mut s = parts();
    let r = run(
        &mut s,
        "joint.constrain",
        json!({"type": "tangent", "a": pick("Base", "face", [20.0, 20.0, 10.0]), "b": pick("Pin", "face", [305.0, 0.0, 15.0])}),
    );
    assert!(clean(&r), "{r}");
    assert_eq!(r["kind"], "line_on_plane");
    let (foot, head) = (place(&s, "Pin", Vec3::new(300.0, 0.0, 0.0)), place(&s, "Pin", Vec3::new(300.0, 0.0, 30.0)));
    assert!((foot.z - 15.0).abs() < 1e-6 && (head.z - 15.0).abs() < 1e-6, "its axis lies 5 mm above the face: {foot:?} {head:?}");
}

/// What a constraint can't take is refused, and the design is left as it was.
#[test]
fn impossible_constraints_are_refused() {
    let mut s = parts();
    let before = s.doc.assembly.joints.len();
    let top = pick("Base", "face", [20.0, 20.0, 10.0]);
    for bad in [
        json!({"type": "flush", "a": top, "b": pick("Pin", "face", [305.0, 0.0, 15.0])}),
        json!({"type": "insert", "a": top, "b": pick("Pin", "circle", [305.0, 0.0, 0.0])}),
        json!({"type": "tangent", "a": top, "b": pick("Pin", "circle", [305.0, 0.0, 0.0])}),
        json!({"type": "mate", "solution": "inside", "a": top, "b": pick("Block", "face", [210.0, 110.0, 50.0])}),
        json!({"type": "mate", "angle": 30, "a": top, "b": pick("Block", "face", [210.0, 110.0, 50.0])}),
        json!({"type": "mate", "a": top, "b": pick("Base", "face", [0.0, 30.0, 5.0])}),
        json!({"type": "glue", "a": top, "b": pick("Block", "face", [210.0, 110.0, 50.0])}),
        json!({"type": "mate", "a": top}),
    ] {
        assert!(s.execute("joint.constrain", &bad).is_err(), "{bad} must be refused");
    }
    assert_eq!(s.doc.assembly.joints.len(), before);
}
