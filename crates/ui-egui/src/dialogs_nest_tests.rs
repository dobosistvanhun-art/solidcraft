//! The nesting editor: opened from the NESTING tab, its parts ticked in the panel, Auto Nest laying
//! them out, a part dragged by hand checked against the others.

use serde_json::json;

use super::*;

fn plates(h: &mut crate::scenario::Harness) {
    for (id, p) in [
        ("sketch.create", json!({"plane": "XY", "name": "Plate"})),
        ("sketch.rectangle.two_point", json!({"p0": [0, 0], "p1": [100, 50]})),
        ("sketch.finish", json!({})),
    ] {
        h.app.run(id, p).unwrap();
    }
}

/// The ribbon's Nesting opens the editor; the job's parts come from the panel's ticks (here
/// as the panel sends them), Auto Nest lays them out, and the editor reads the layout back.
#[test]
fn the_editor_lays_out_the_ticked_parts() {
    let mut h = crate::scenario::Harness::new();
    plates(&mut h);
    h.app.start("nest.setup");
    h.frames(2);
    assert!(matches!(h.app.dialog.as_ref().map(|d| &d.kind), Some(Kind::Nest(_))));
    let sketch = h.app.session.doc.find_feature("Plate").map(|f| f.id).unwrap();
    h.app.run("nest.setup", json!({"parts": [{"sketch": sketch, "count": 6}], "sheet": {"width": 600, "height": 300, "margin": 5}, "quality": "fast"})).unwrap();
    let mut k = NestUi::default();
    auto_nest(&mut h.app, &mut k);
    // Off the thread or not, the layout arrives.
    for _ in 0..200 {
        poll(&mut h.app, &mut k);
        if k.running.is_none() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let v = info(&mut h.app, &mut k).unwrap();
    assert_eq!(v["placements"].as_array().map(Vec::len), Some(6), "{v}");
    assert_eq!(v["collisions"], json!([]));
    h.frames(2);
}

/// The candidates are the sketches with a closed contour and the sheet metal bodies.
#[test]
fn candidates_are_closed_sketches() {
    let mut h = crate::scenario::Harness::new();
    plates(&mut h);
    h.app.run("sketch.create", json!({"plane": "XY", "name": "Open"})).unwrap();
    h.app.run("sketch.line", json!({"points": [[0, 0], [10, 10]], "infer": false})).unwrap();
    h.app.run("sketch.finish", json!({})).unwrap();
    let names: Vec<String> = candidates(&h.app).into_iter().map(|c| c.name).collect();
    assert_eq!(names, ["Plate"]);
}
