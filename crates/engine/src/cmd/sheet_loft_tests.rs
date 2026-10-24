//! Sheet metal loft (`sheet.loft`): a square-to-round transition ripped open, with a flat
//! pattern whose area is the wall's and whose rounds are bent in steps.

use serde_json::{Value, json};

use crate::Session;

fn run(s: &mut Session, id: &str, p: Value) -> Value {
    match s.execute(id, &p) {
        Ok(v) => v,
        Err(e) => panic!("{id} {p}: {e}"),
    }
}

/// A 100 × 100 square on XY and a Ø60 circle 80 above it.
fn sections() -> Session {
    let mut s = Session::default();
    run(&mut s, "sketch.create", json!({"plane": "XY", "name": "Square"}));
    run(&mut s, "sketch.rectangle.two_point", json!({"p0": [-50, -50], "p1": [50, 50]}));
    run(&mut s, "sketch.finish", json!({}));
    run(&mut s, "sketch.create", json!({"plane": {"origin": [0, 0, 80], "x_dir": [1, 0, 0], "y_dir": [0, 1, 0]}, "name": "Round"}));
    run(&mut s, "sketch.circle.center", json!({"center": [0, 0], "radius": 30}));
    run(&mut s, "sketch.finish", json!({}));
    s
}

fn area(outline: &Value) -> f64 {
    let pts: Vec<(f64, f64)> = outline.as_array().into_iter().flatten().filter_map(|p| Some((p.get(0)?.as_f64()?, p.get(1)?.as_f64()?))).collect();
    let n = pts.len();
    (0..n).map(|i| pts[i].0 * pts[(i + 1) % n].1 - pts[(i + 1) % n].0 * pts[i].1).sum::<f64>().abs() / 2.0
}

fn volume(s: &mut Session) -> f64 {
    run(s, "inspect.measure", json!({}))["total"]["volume_mm3"].as_f64().unwrap_or(f64::NAN)
}

#[test]
fn a_square_to_round_transition_unfolds_with_its_rounds_bent_in_steps() {
    let mut s = sections();
    run(&mut s, "sheet.loft", json!({"sections": ["Square", "Round"]}));
    // A solid of planar facets (sewn, not a mesh).
    let st = s.model.state();
    let body = st.bodies.last().unwrap();
    assert!(!body.body.is_mesh(), "the wall should be a B-rep");
    let flat = run(&mut s, "sheet.flat_pattern", json!({}));
    let t = flat["thickness"].as_f64().unwrap();
    // Four rounds of 90°, in bends of at most 15°.
    let bends = flat["bends"].as_array().unwrap();
    assert_eq!(bends.len(), 24, "{bends:?}");
    assert!(bends.iter().all(|b| (b["angle_deg"].as_f64().unwrap() - 15.0).abs() < 0.5), "{bends:?}");
    // The flat keeps the wall's material.
    let folded = volume(&mut s);
    let a = area(&flat["outline"][0]);
    assert!((a * t - folded).abs() / folded < 0.03, "flat {a} × {t} vs folded {folded}");
    // Unfolded it is that plate; refolded the wall again.
    run(&mut s, "sheet.unfold", json!({}));
    let plate = volume(&mut s);
    assert!((plate - a * t).abs() / plate < 1e-3, "{plate} vs {}", a * t);
    run(&mut s, "sheet.refold", json!({}));
    assert!((volume(&mut s) - folded).abs() < 1e-6 * folded);
    // A finer step bends more often.
    let mut s = sections();
    run(&mut s, "sheet.loft", json!({"sections": ["Square", "Round"], "bend_step": 5}));
    assert_eq!(run(&mut s, "sheet.flat_pattern", json!({}))["bends"].as_array().map(Vec::len), Some(72));
    // A rip through a point near a corner: the same material.
    let a5 = area(&run(&mut s, "sheet.flat_pattern", json!({}))["outline"][0]);
    let mut r = sections();
    run(&mut r, "sheet.loft", json!({"sections": ["Square", "Round"], "bend_step": 5, "rip": [40, 50, 0]}));
    let ar = area(&run(&mut r, "sheet.flat_pattern", json!({}))["outline"][0]);
    assert!((ar - a5).abs() < 0.01 * a5, "{ar} vs {a5}");
    // At an angle: 90° from the square's x axis is the middle of its far side, as by default
    // (the longest side's middle); a wider rip takes more material from the wall.
    let mut g = sections();
    run(&mut g, "sheet.loft", json!({"sections": ["Square", "Round"], "bend_step": 5, "rip_angle": 90}));
    let ag = area(&run(&mut g, "sheet.flat_pattern", json!({}))["outline"][0]);
    assert!((ag - a5).abs() < 0.01 * a5, "{ag} vs {a5}");
    let narrow = volume(&mut g);
    let mut w = sections();
    run(&mut w, "sheet.loft", json!({"sections": ["Square", "Round"], "bend_step": 5, "rip_angle": 90, "rip_gap": 3}));
    assert!(volume(&mut w) < narrow - 1.0, "a 3 mm rip takes more than a 0.1 mm one");
    // The facets' joins on the wall are drawn smooth: the rims and the rip keep their lines.
    let mesh = w.model.state().bodies.last().map(|b| b.mesh()).unwrap();
    let drawn = mesh.smooth_edges.iter().filter(|f| !**f).count();
    assert!(mesh.smooth_edges.iter().filter(|f| **f).count() > drawn, "{drawn} drawn of {}", mesh.edges.len());
    // The DXF of the flat pattern.
    let dxf = run(&mut s, "sheet.export_dxf", json!({}));
    assert!(dxf.to_string().contains("BEND"), "{}", &dxf.to_string()[..dxf.to_string().len().min(400)]);
}

#[test]
fn hostile_sheet_lofts_are_refused() {
    let mut s = sections();
    for p in [
        json!({"sections": ["Square"]}),
        json!({"sections": ["Square", "Round", "Round"]}),
        json!({"sections": ["Square", "Round"], "rule": "nope"}),
        json!({"sections": ["Square", "Round"], "side": "up"}),
        json!({"sections": ["Square", "Round"], "bend_step": 0}),
        json!({"sections": ["Square", "Round"], "bend_step": 1e300}),
        json!({"sections": ["Square", "Square"]}),
        json!({"sections": ["Square", "Round"], "rip": "here"}),
        json!({"sections": ["Square", "Round"], "rip_gap": 0}),
        json!({"sections": ["Square", "Round"], "rip_gap": "5 deg"}),
        json!({"sections": ["Square", "Round"], "rip_angle": "10 mm"}),
        json!({"sections": ["Square", "Round"], "rip_angle": 1e300}),
    ] {
        assert!(s.execute("sheet.loft", &p).is_err(), "{p}");
    }
    // Flanges on it are not supported yet.
    run(&mut s, "sheet.loft", json!({"sections": ["Square", "Round"]}));
    assert!(s.execute("sheet.hem", &json!({"edges": [[0, -50, 0]], "length": 10})).is_err());
}
