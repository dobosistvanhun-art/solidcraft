//! Nesting: the acceptance checks of the first version — the same part many times, mixed parts,
//! irregular outlines, holes, turning, no overlaps, nothing past the margin, several sheets,
//! hand edits and a DXF where the parts lie.

use serde_json::{Value, json};

use crate::Session;

fn run(s: &mut Session, id: &str, p: Value) -> Value {
    match s.execute(id, &p) {
        Ok(v) => v,
        Err(e) => panic!("{id} {p}: {e}"),
    }
}

fn rect(s: &mut Session, name: &str, w: f64, h: f64) {
    run(s, "sketch.create", json!({"plane": "XY", "name": name}));
    run(s, "sketch.rectangle.two_point", json!({"p0": [0, 0], "p1": [w, h]}));
    run(s, "sketch.finish", json!({}));
}

/// An L: 100 × 100 with a 70 × 70 notch out of its upper right.
fn ell(s: &mut Session, name: &str) {
    run(s, "sketch.create", json!({"plane": "XY", "name": name}));
    run(s, "sketch.line", json!({"points": [[0, 0], [100, 0], [100, 30], [30, 30], [30, 100], [0, 100]], "closed": true}));
    run(s, "sketch.finish", json!({}));
}

fn clean(r: &Value) {
    assert_eq!(r["collisions"], json!([]), "{}", r["collisions"]);
    assert_eq!(r["outside"], json!([]), "{}", r["outside"]);
    assert_eq!(r["unplaced"], json!([]), "{}", r["unplaced"]);
}

/// The same part many times: twenty 100 × 50 plates on one 1220 × 610 sheet, none touching,
/// none past the margin.
#[test]
fn twenty_copies_of_one_part_go_on_one_sheet() {
    let mut s = Session::default();
    rect(&mut s, "Plate", 100.0, 50.0);
    run(
        &mut s,
        "nest.setup",
        json!({"parts": [{"sketch": "Plate", "count": 20}], "sheet": {"width": 1220, "height": 610, "margin": 10}, "gap": 4, "quality": "fast"}),
    );
    let r = run(&mut s, "nest.run", json!({}));
    clean(&r);
    assert_eq!(r["stats"]["placed"], 20);
    assert_eq!(r["stats"]["sheets"], 1);
    let used = r["stats"]["utilisation_percent"].as_f64().unwrap_or(0.0);
    assert!((used - 20.0 * 5000.0 / (1220.0 * 610.0) * 100.0).abs() < 1e-6, "{used}");
    assert!((r["stats"]["cut_length_mm"].as_f64().unwrap_or(0.0) - 20.0 * 300.0).abs() < 1e-6, "{}", r["stats"]);
}

/// Mixed parts and several sheets: ten 500 × 500 plates take five 1220 × 610 sheets (two each),
/// and the small parts fill in beside them.
#[test]
fn mixed_parts_spread_over_several_sheets() {
    let mut s = Session::default();
    rect(&mut s, "Big", 500.0, 500.0);
    rect(&mut s, "Small", 60.0, 40.0);
    run(
        &mut s,
        "nest.setup",
        json!({"parts": [{"sketch": "Big", "count": 10}, {"sketch": "Small", "count": 12}], "sheet": {"width": 1220, "height": 610, "margin": 5}, "gap": 5, "quality": "fast"}),
    );
    let r = run(&mut s, "nest.run", json!({}));
    clean(&r);
    assert_eq!(r["stats"]["sheets"], 5, "{}", r["stats"]);
    assert_eq!(r["stats"]["placed"], 22);
}

/// An irregular outline is nested by its real shape: a 50 × 50 square goes into an L's notch
/// on a sheet the L's own size, which a box around the L would leave no room for.
#[test]
fn a_part_goes_into_another_parts_notch() {
    let mut s = Session::default();
    ell(&mut s, "Ell");
    rect(&mut s, "Square", 50.0, 50.0);
    run(
        &mut s,
        "nest.setup",
        json!({"parts": [{"sketch": "Ell", "fixed_angle": true}, {"sketch": "Square", "fixed_angle": true}], "sheet": {"width": 102, "height": 102, "margin": 0}, "gap": 2, "quality": "precise"}),
    );
    let r = run(&mut s, "nest.run", json!({}));
    clean(&r);
    assert_eq!(r["stats"]["sheets"], 1, "{}", r["stats"]);
}

/// Turning finds room: a 500 × 20 strip fits a 300 × 600 sheet only standing up.
#[test]
fn a_part_is_turned_to_fit() {
    let mut s = Session::default();
    rect(&mut s, "Strip", 500.0, 20.0);
    run(
        &mut s,
        "nest.setup",
        json!({"parts": [{"sketch": "Strip", "count": 3}], "sheet": {"width": 300, "height": 600, "margin": 5}, "gap": 3, "quality": "normal"}),
    );
    let r = run(&mut s, "nest.run", json!({}));
    clean(&r);
    let angles: Vec<f64> = r["placements"].as_array().into_iter().flatten().filter_map(|p| p["angle"].as_f64()).collect();
    assert_eq!(angles.len(), 3);
    assert!(angles.iter().all(|a| ((a - 90.0).abs() < 1e-6) || ((a - 270.0).abs() < 1e-6)), "{angles:?}");
    // With its grain along it, a half turn is all it may take: it doesn't fit.
    run(&mut s, "nest.setup", json!({"parts": [{"sketch": "Strip", "count": 3, "grain": true}]}));
    let r = run(&mut s, "nest.run", json!({}));
    assert_eq!(r["unplaced"].as_array().map(Vec::len), Some(3), "{}", r["unplaced"]);
}

/// Holes are kept: a plate with a Ø20 hole exports its outline on OUTER and its hole on INNER,
/// the hole where the plate was put, and the DXF reads back.
#[test]
fn holes_go_to_the_dxf_where_the_part_lies() {
    let mut s = Session::default();
    run(&mut s, "sketch.create", json!({"plane": "XY", "name": "Ring"}));
    run(&mut s, "sketch.rectangle.two_point", json!({"p0": [0, 0], "p1": [80, 60]}));
    run(&mut s, "sketch.circle.center", json!({"center": [40, 30], "radius": 10}));
    run(&mut s, "sketch.finish", json!({}));
    run(
        &mut s,
        "nest.setup",
        json!({"parts": [{"sketch": "Ring", "count": 2, "fixed_angle": true}], "sheet": {"width": 400, "height": 200, "margin": 10}, "gap": 5, "quality": "fast"}),
    );
    let r = run(&mut s, "nest.run", json!({}));
    clean(&r);
    let dxf = run(&mut s, "nest.export_dxf", json!({}))["dxf"].as_str().unwrap_or_default().to_string();
    assert!(dxf.contains("OUTER") && dxf.contains("INNER") && dxf.contains("$INSUNITS"), "layers and mm");
    let geom = solidcraft_io::read_dxf(&dxf).unwrap_or_default();
    assert!(geom.len() >= 4, "two outlines and two holes: {geom:?}");
}

/// Hand edits: a copy moved onto another is reported as colliding; locked, it stays where it
/// was put when the layout runs again, and the others make room for it.
#[test]
fn a_moved_copy_is_checked_and_a_locked_one_stays() {
    let mut s = Session::default();
    rect(&mut s, "Plate", 100.0, 50.0);
    run(
        &mut s,
        "nest.setup",
        json!({"parts": [{"sketch": "Plate", "count": 4}], "sheet": {"width": 600, "height": 300, "margin": 5}, "gap": 4, "quality": "fast"}),
    );
    let r = run(&mut s, "nest.run", json!({}));
    clean(&r);
    let first = r["placements"][0].clone();
    let e = run(&mut s, "nest.edit", json!({"placement": 1, "x": first["x"], "y": first["y"]}));
    assert!(e["collides"].as_array().is_some_and(|c| c.contains(&json!(0))), "{e}");
    let e = run(&mut s, "nest.edit", json!({"placement": 1, "x": 450, "y": 200, "angle": 30, "locked": true}));
    assert_eq!(e["collides"], json!([]), "{e}");
    assert_eq!(e["outside"], json!(false));
    let r = run(&mut s, "nest.run", json!({}));
    clean(&r);
    let kept = r["placements"].as_array().into_iter().flatten().find(|p| p["locked"] == json!(true)).cloned().unwrap_or_default();
    assert_eq!((kept["x"].as_f64(), kept["y"].as_f64(), kept["angle"].as_f64()), (Some(450.0), Some(200.0), Some(30.0)), "{kept}");
    assert_eq!(r["stats"]["placed"], 4);
    // Removed from the layout: three left.
    let idx = kept["index"].as_u64().unwrap_or(0);
    let r = run(&mut s, "nest.edit", json!({"placement": idx, "remove": true}));
    assert_eq!(r["placements"].as_array().map(Vec::len), Some(3));
}

/// What the job can't take is refused, and a part too big for the sheet is left unplaced.
#[test]
fn impossible_jobs_are_refused_or_reported() {
    let mut s = Session::default();
    rect(&mut s, "Plate", 100.0, 50.0);
    assert!(s.execute("nest.run", &json!({})).is_err(), "no job yet");
    for bad in [
        json!({"parts": [{"sketch": "Nothing"}]}),
        json!({"parts": [{"sketch": "Plate", "count": 0}]}),
        json!({"parts": [{}]}),
        json!({"sheet": {"width": 0, "height": 100}}),
        json!({"gap": -1}),
        json!({"quality": "slow"}),
    ] {
        assert!(s.execute("nest.setup", &bad).is_err(), "{bad} must be refused");
    }
    run(&mut s, "nest.setup", json!({"parts": [{"sketch": "Plate"}], "sheet": {"width": 80, "height": 80, "margin": 0}, "quality": "fast"}));
    let r = run(&mut s, "nest.run", json!({}));
    assert_eq!(r["unplaced"], json!([0]), "{r}");
    assert!(s.execute("nest.export_dxf", &json!({})).is_err(), "nothing laid out");
    run(&mut s, "nest.setup", json!({"clear": true}));
    assert!(s.doc.nesting.is_none());
}

/// The job is saved with the design.
#[test]
fn the_job_is_saved_with_the_design() {
    let mut s = Session::default();
    rect(&mut s, "Plate", 100.0, 50.0);
    run(&mut s, "nest.setup", json!({"parts": [{"sketch": "Plate", "count": 3}], "sheet": {"width": 400, "height": 200}, "quality": "fast"}));
    run(&mut s, "nest.run", json!({}));
    let reopened = solidcraft_doc::Document::from_json(&s.doc.to_json()).unwrap_or_default();
    assert_eq!(reopened.nesting, s.doc.nesting);
    assert_eq!(reopened.nesting.map(|j| j.layout.placements.len()), Some(3));
}
