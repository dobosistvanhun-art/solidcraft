//! The nesting editor: opened from the NESTING tab, its parts added in the panel (sketches, sheet
//! metal bodies, DXF files), Auto Nest laying them out, a part dragged by hand checked against the others.

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
    h.app
        .run("nest.setup", json!({"parts": [{"sketch": sketch, "count": 6}], "sheet": {"width": 600, "height": 300, "margin": 5}, "quality": "fast"}))
        .unwrap();
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

/// Every sketch is offered (one without a closed contour shown but not choosable), and the
/// sheet metal bodies.
#[test]
fn candidates_are_every_sketch_closed_or_not() {
    let mut h = crate::scenario::Harness::new();
    plates(&mut h);
    h.app.run("sketch.create", json!({"plane": "XY", "name": "Open"})).unwrap();
    h.app.run("sketch.line", json!({"points": [[0, 0], [10, 10]], "infer": false})).unwrap();
    h.app.run("sketch.finish", json!({})).unwrap();
    let got: Vec<(String, bool)> = candidates(&h.app).into_iter().map(|c| (c.name, c.closed)).collect();
    assert_eq!(got, [("Plate".to_string(), true), ("Open".to_string(), false)]);
}

/// Add part in the panel puts the chosen sketch in the job; its row then changes its count and
/// its cross takes it out again.
#[test]
fn add_part_puts_a_sketch_in_the_job_and_the_cross_takes_it_out() {
    let mut h = crate::scenario::Harness::new();
    plates(&mut h);
    h.app.start("nest.setup");
    h.frames(4);
    let at = h.widget("Add part");
    assert!(at.is_some(), "the panel has Add part");
    if let Some([x, y]) = at {
        h.call("ui.click", json!({"x": x, "y": y}));
    }
    h.frames(3);
    let at = h.widget("Plate");
    assert!(at.is_some(), "the menu lists the sketch");
    if let Some([x, y]) = at {
        h.call("ui.click", json!({"x": x, "y": y}));
    }
    h.frames(3);
    let parts = h.app.session.doc.nesting.as_ref().map(|j| j.parts.len());
    assert_eq!(parts, Some(1), "the click adds the part");
    let at = h.widget("✕");
    assert!(at.is_some(), "the part's row has its cross");
    if let Some([x, y]) = at {
        h.call("ui.click", json!({"x": x, "y": y}));
    }
    h.frames(3);
    let parts = h.app.session.doc.nesting.as_ref().map(|j| j.parts.len());
    assert_eq!(parts, Some(0), "the cross takes it out");
}

/// A DXF brought in lists its part in the panel like any other.
#[test]
fn an_imported_dxf_is_listed_in_the_panel() {
    let mut h = crate::scenario::Harness::new();
    plates(&mut h);
    let dxf = h.app.run("sketch.export_dxf", json!({})).unwrap()["dxf"].as_str().unwrap_or_default().to_string();
    h.app.run("nest.import_dxf", json!({"text": dxf, "name": "Bracket"})).unwrap();
    h.app.start("nest.setup");
    h.frames(4);
    assert!(h.widget("Bracket").is_some(), "the imported part has its row");
}
