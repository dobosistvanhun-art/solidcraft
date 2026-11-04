//! `sketch.export_cut_dxf`: closed contours once each, part edges on OUTER and holes on INNER,
//! open curves refused unless asked for.

use serde_json::{Value, json};

use super::{INNER_LAYER, OPEN_LAYER, OUTER_LAYER};
use crate::Session;

fn run(s: &mut Session, id: &str, p: Value) -> Value {
    match s.execute(id, &p) {
        Ok(v) => v,
        Err(e) => panic!("{id} {p}: {e}"),
    }
}

/// The layer of each entity in the ENTITIES section (group code 8 after a code 0).
fn entity_layers(dxf: &str) -> Vec<String> {
    let lines: Vec<&str> = dxf.lines().map(str::trim).collect();
    // "ENTITIES" is the value of a code-2 pair: the section's pairs start on the next line.
    let start = lines.iter().position(|l| *l == "ENTITIES").map_or(lines.len(), |i| i + 1);
    let mut out = Vec::new();
    let mut i = start;
    while i + 1 < lines.len() {
        if lines[i] == "8" {
            out.push(lines[i + 1].to_string());
        }
        if lines[i] == "0" && lines[i + 1] == "ENDSEC" {
            break;
        }
        i += 2;
    }
    out
}

/// A 100 × 60 plate with a Ø20 hole, and a Ø8 island standing inside the hole.
fn plate(s: &mut Session) {
    run(s, "sketch.create", json!({"plane": "XY"}));
    run(s, "sketch.rectangle.two_point", json!({"p0": [0, 0], "p1": [100, 60]}));
    run(s, "sketch.circle.center", json!({"center": [30, 30], "radius": 10}));
    run(s, "sketch.circle.center", json!({"center": [30, 30], "radius": 4}));
}

#[test]
fn a_plate_with_a_hole_and_an_island_sorts_its_contours_by_nesting() {
    let mut s = Session::default();
    plate(&mut s);
    let r = run(&mut s, "sketch.export_cut_dxf", json!({}));
    assert_eq!((r["outer"].as_u64(), r["inner"].as_u64()), (Some(2), Some(1)), "{r}");
    assert_eq!(r["open_ends"], 0);
    assert_eq!(r["open_curves"], 0);
    let dxf = r["dxf"].as_str().unwrap_or_default();
    assert!(dxf.contains("AC1015") && dxf.contains("$INSUNITS"), "a DXF in millimetres");
    let layers = entity_layers(dxf);
    assert!(layers.iter().any(|l| l == OUTER_LAYER) && layers.iter().any(|l| l == INNER_LAYER), "{layers:?}");
    assert!(!layers.iter().any(|l| l == OPEN_LAYER), "{layers:?}");
}

#[test]
fn open_curves_are_refused_unless_allowed_and_then_go_on_their_own_layer() {
    let mut s = Session::default();
    plate(&mut s);
    run(&mut s, "sketch.line", json!({"points": [[120, 0], [140, 20]], "infer": false}));
    let err = s.execute("sketch.export_cut_dxf", &json!({})).err().map(|e| e.to_string()).unwrap_or_default();
    assert!(err.contains("2 open ends") && err.contains("1 curves"), "{err}");
    let r = run(&mut s, "sketch.export_cut_dxf", json!({"allow_open": true}));
    assert_eq!((r["open_ends"].as_u64(), r["open_curves"].as_u64()), (Some(2), Some(1)), "{r}");
    assert!(entity_layers(r["dxf"].as_str().unwrap_or_default()).iter().any(|l| l == OPEN_LAYER));
}

#[test]
fn construction_curves_are_left_out_and_an_empty_sketch_is_an_error() {
    let mut s = Session::default();
    run(&mut s, "sketch.create", json!({"plane": "XY"}));
    assert!(s.execute("sketch.export_cut_dxf", &json!({})).is_err());
    run(&mut s, "sketch.line", json!({"points": [[0, 0], [50, 0]], "infer": false, "construction": true}));
    assert!(s.execute("sketch.export_cut_dxf", &json!({})).is_err(), "construction alone is nothing to cut");
    assert!(s.execute("sketch.export_cut_dxf", &json!({"unknown": 1})).is_err());
}

#[test]
fn a_cut_dxf_reads_back_as_the_same_contours() {
    let mut s = Session::default();
    plate(&mut s);
    let dxf = run(&mut s, "sketch.export_cut_dxf", json!({}))["dxf"].as_str().unwrap_or_default().to_string();
    let geom = solidcraft_io::read_dxf(&dxf).unwrap_or_default();
    assert!(geom.len() >= 3, "{geom:?}");
}

/// After Finish Sketch nothing is being edited: the save takes the design's newest sketch, as the Cutting
/// tab's button does; a design without a sketch is told so.
#[test]
fn a_finished_sketch_is_still_saved_as_the_newest_one() {
    let mut s = Session::default();
    assert!(s.execute("sketch.export_cut_dxf", &json!({})).is_err(), "no sketch at all");
    plate(&mut s);
    run(&mut s, "sketch.finish", json!({}));
    let r = run(&mut s, "sketch.export_cut_dxf", json!({}));
    assert_eq!((r["outer"].as_u64(), r["inner"].as_u64()), (Some(2), Some(1)), "{r}");
    let plain = run(&mut s, "sketch.export_dxf", json!({}));
    assert!(plain["dxf"].as_str().is_some_and(|d| d.contains("CIRCLE")), "Save As DXF takes it too: {plain}");
}
