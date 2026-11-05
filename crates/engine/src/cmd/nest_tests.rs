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
    assert_eq!(r["collisions"], json!([]), "{r}");
    assert_eq!(r["outside"], json!([]), "{r}");
    assert_eq!(r["unplaced"], json!([]), "{r}");
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
    assert!((used - 20.0 * 5000.0 / (1220.0 * 610.0) * 100.0).abs() < 1e-2, "{used}");
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
    // Lying flat (0° or 180°) it is too long for the sheet.
    assert!(angles.iter().all(|a| (a % 180.0).abs() > 1.0), "{angles:?}");
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

/// A DXF of two plates (one with a Ø20 hole), as another program would hand it over.
fn two_plates_dxf() -> String {
    let mut other = Session::default();
    run(&mut other, "sketch.create", json!({"plane": "XY", "name": "Drawing"}));
    run(&mut other, "sketch.rectangle.two_point", json!({"p0": [0, 0], "p1": [80, 60]}));
    run(&mut other, "sketch.circle.center", json!({"center": [40, 30], "radius": 10}));
    run(&mut other, "sketch.rectangle.two_point", json!({"p0": [150, 0], "p1": [200, 30]}));
    run(&mut other, "sketch.finish", json!({}));
    run(&mut other, "sketch.export_dxf", json!({}))["dxf"].as_str().unwrap_or_default().to_string()
}

/// A DXF's closed contours come in as parts kept with the job (one, or one per contour with
/// `split`), holes and all, and nest like any part; the design gets no sketch from it.
#[test]
fn a_dxf_comes_in_as_parts_and_nests() {
    let mut s = Session::default();
    let dxf = two_plates_dxf();
    let r = run(&mut s, "nest.import_dxf", json!({"text": dxf, "name": "Bracket", "count": 2}));
    assert_eq!(r["added"], 1);
    assert_eq!(r["parts"][0]["name"], "Bracket");
    assert!((r["parts"][0]["area_mm2"].as_f64().unwrap_or(0.0) - (4800.0 - std::f64::consts::PI * 100.0 + 1500.0)).abs() < 1.0, "{}", r["parts"][0]);
    let r = run(&mut s, "nest.import_dxf", json!({"text": dxf, "name": "Piece", "split": true}));
    assert_eq!(r["added"], 2);
    let names: Vec<&str> = r["parts"].as_array().into_iter().flatten().filter_map(|p| p["name"].as_str()).collect();
    assert_eq!(names, ["Bracket", "Piece-1", "Piece-2"]);
    assert!(s.doc.features.is_empty(), "the import leaves the design as it was");
    run(&mut s, "nest.setup", json!({"sheet": {"width": 500, "height": 300, "margin": 5}, "quality": "fast"}));
    let r = run(&mut s, "nest.run", json!({}));
    clean(&r);
    assert_eq!(r["stats"]["placed"], 4);
    let out = run(&mut s, "nest.export_dxf", json!({}))["dxf"].as_str().unwrap_or_default().to_string();
    assert!(out.contains("INNER"), "the hole is cut");
    let reopened = solidcraft_doc::Document::from_json(&s.doc.to_json()).unwrap_or_default();
    assert_eq!(reopened.nesting, s.doc.nesting, "kept with the design");
    // An open line is no part.
    let mut other = Session::default();
    run(&mut other, "sketch.create", json!({"plane": "XY"}));
    run(&mut other, "sketch.line", json!({"points": [[0, 0], [50, 0]], "infer": false}));
    let open = run(&mut other, "sketch.export_dxf", json!({}))["dxf"].as_str().unwrap_or_default().to_string();
    assert!(s.execute("nest.import_dxf", &json!({"text": open})).is_err());
}

/// The part list: parts added one by one keep the layout of the others; a count, grain or
/// fixed angle changes one part; a part taken out takes its copies out of the layout.
#[test]
fn parts_are_added_changed_and_removed_one_at_a_time() {
    let mut s = Session::default();
    rect(&mut s, "A", 60.0, 40.0);
    rect(&mut s, "B", 30.0, 30.0);
    run(&mut s, "nest.setup", json!({"add": [{"sketch": "A", "count": 2}], "sheet": {"width": 400, "height": 200}, "quality": "fast"}));
    run(&mut s, "nest.run", json!({}));
    let r = run(&mut s, "nest.setup", json!({"add": [{"sketch": "B", "count": 3}]}));
    assert_eq!(r["placements"].as_array().map(Vec::len), Some(2), "A's copies stay laid out");
    assert!(s.execute("nest.setup", &json!({"add": [{"sketch": "B"}]})).is_err(), "B is in already");
    let r = run(&mut s, "nest.setup", json!({"edit": {"index": 1, "count": 5, "grain": true}}));
    assert_eq!((r["parts"][1]["count"].as_u64(), r["parts"][1]["grain"].as_bool()), (Some(5), Some(true)));
    let r = run(&mut s, "nest.run", json!({}));
    assert_eq!(r["stats"]["placed"], 7);
    let r = run(&mut s, "nest.setup", json!({"edit": {"index": 0, "remove": true}}));
    assert_eq!(r["parts"].as_array().map(Vec::len), Some(1));
    assert_eq!(r["placements"].as_array().map(Vec::len), Some(5), "A's two copies left with it");
    assert!(r["placements"].as_array().into_iter().flatten().all(|p| p["part"] == 0), "B is part 0 now");
    assert!(s.execute("nest.setup", &json!({"edit": {"index": 4}})).is_err());
}

/// A part's inner contours, a sketch's or a DXF's, lie on every placed copy: `nest.info` sends
/// them turned and moved with the copy, and the cutting DXF cuts them on INNER.
#[test]
fn the_holes_lie_on_every_placed_copy() {
    let mut s = Session::default();
    run(&mut s, "sketch.create", json!({"plane": "XY", "name": "Flange"}));
    run(&mut s, "sketch.rectangle.two_point", json!({"p0": [0, 0], "p1": [80, 60]}));
    run(&mut s, "sketch.circle.center", json!({"center": [40, 30], "radius": 10}));
    run(&mut s, "sketch.finish", json!({}));
    let dxf = run(&mut s, "sketch.export_dxf", json!({}))["dxf"].as_str().unwrap_or_default().to_string();
    run(&mut s, "nest.setup", json!({"add": [{"sketch": "Flange", "count": 2}], "sheet": {"width": 400, "height": 200}, "quality": "fast"}));
    run(&mut s, "nest.import_dxf", json!({"text": dxf, "name": "Imported", "count": 2}));
    clean(&run(&mut s, "nest.run", json!({})));
    let r = run(&mut s, "nest.info", json!({}));
    let placements = r["placements"].as_array().cloned().unwrap_or_default();
    assert_eq!(placements.len(), 4);
    for pl in &placements {
        let holes = pl["holes"].as_array().cloned().unwrap_or_default();
        assert_eq!(holes.len(), 1, "one hole on each copy: {pl}");
        // The hole is the Ø20 circle about the copy's centre, wherever and however it lies.
        let (cx, cy) = (pl["x"].as_f64().unwrap_or(0.0), pl["y"].as_f64().unwrap_or(0.0));
        for q in holes[0].as_array().into_iter().flatten() {
            let d = ((q[0].as_f64().unwrap_or(0.0) - cx).powi(2) + (q[1].as_f64().unwrap_or(0.0) - cy).powi(2)).sqrt();
            assert!((d - 10.0).abs() < 0.1, "{d}");
        }
    }
    let out = run(&mut s, "nest.export_dxf", json!({}))["dxf"].as_str().unwrap_or_default().to_string();
    let entities = out.split("ENTITIES").nth(1).unwrap_or_default();
    assert!(entities.matches("INNER").count() >= 4, "every copy's hole is cut");
}

/// The nest DXF gives each contour its cutting colour: a sketch contour's own, a part's colour
/// over it, else red edges and blue holes; LightBurn reads the exact colour on every entity.
#[test]
fn the_nest_dxf_carries_the_cutting_colours() {
    let mut s = Session::default();
    run(&mut s, "sketch.create", json!({"plane": "XY", "name": "Flange"}));
    run(&mut s, "sketch.rectangle.two_point", json!({"p0": [0, 0], "p1": [80, 60]}));
    let hole = run(&mut s, "sketch.circle.center", json!({"center": [40, 30], "radius": 10}))["created"]["curves"][0]
        .as_str()
        .unwrap_or_default()
        .to_string();
    run(&mut s, "sketch.cut_colour", json!({"curves": [hole], "colour": "orange"}));
    run(&mut s, "sketch.finish", json!({}));
    rect(&mut s, "Tab", 30.0, 20.0);
    run(
        &mut s,
        "nest.setup",
        json!({"add": [{"sketch": "Flange"}, {"sketch": "Tab", "outer_colour": "green"}], "sheet": {"width": 400, "height": 200}, "quality": "fast"}),
    );
    let info = run(&mut s, "nest.info", json!({"brief": true}));
    assert_eq!(info["parts"][1]["outer_colour"], "green");
    clean(&run(&mut s, "nest.run", json!({})));
    let dxf = run(&mut s, "nest.export_dxf", json!({}))["dxf"].as_str().unwrap_or_default().to_string();
    let entities = dxf.split("ENTITIES").nth(1).unwrap_or_default();
    // The hole on its orange layer with its exact colour, the Tab green, the Flange's edge red.
    assert!(entities.contains("INNER_ORANGE") && entities.contains("16744448"), "{entities}");
    assert!(entities.contains("OUTER_GREEN") && entities.contains("57344"));
    assert!(entities.contains("\nOUTER\n") && entities.contains("16711680"));
    assert!(dxf.contains("AC1018"));
    // A part's colour taken off: the Tab is red again.
    run(&mut s, "nest.setup", json!({"edit": {"index": 1, "outer_colour": null}}));
    let dxf = run(&mut s, "nest.export_dxf", json!({}))["dxf"].as_str().unwrap_or_default().to_string();
    assert!(!dxf.contains("OUTER_GREEN"));
    assert!(s.execute("nest.setup", &json!({"edit": {"index": 1, "outer_colour": "pink"}})).is_err());
}
