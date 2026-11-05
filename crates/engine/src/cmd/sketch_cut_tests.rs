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
    assert!(dxf.contains("AC1018") && dxf.contains("$INSUNITS"), "an AutoCAD 2004 DXF (exact colours) in millimetres");
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

/// The (code 62, code 420) colour of each entity in the ENTITIES section, by its layer.
fn entity_colours(dxf: &str) -> Vec<(String, String, String)> {
    let lines: Vec<&str> = dxf.lines().map(str::trim).collect();
    let start = lines.iter().position(|l| *l == "ENTITIES").map_or(lines.len(), |i| i + 1);
    let mut out: Vec<(String, String, String)> = Vec::new();
    let mut i = start;
    while i + 1 < lines.len() {
        match lines[i] {
            "0" if lines[i + 1] == "ENDSEC" => break,
            "0" => out.push(Default::default()),
            "8" => {
                if let Some(e) = out.last_mut() {
                    e.0 = lines[i + 1].to_string();
                }
            }
            "62" => {
                if let Some(e) = out.last_mut() {
                    e.1 = lines[i + 1].to_string();
                }
            }
            "420" => {
                if let Some(e) = out.last_mut() {
                    e.2 = lines[i + 1].to_string();
                }
            }
            _ => {}
        }
        i += 2;
    }
    out
}

/// A contour given a colour lies on a layer of that colour and every entity of it carries the
/// colour exactly (LightBurn's orange, 255 128 0) and as its AutoCAD index; the others keep red
/// part edges and blue holes. One curve of a contour colours the whole contour.
#[test]
fn a_contour_s_colour_goes_with_it_into_the_dxf() {
    let mut s = Session::default();
    run(&mut s, "sketch.create", json!({"plane": "XY"}));
    let rect = run(&mut s, "sketch.rectangle.two_point", json!({"p0": [0, 0], "p1": [100, 60]}));
    let edge = rect["curves"][0].as_str().unwrap_or_default().to_string();
    let hole = run(&mut s, "sketch.circle.center", json!({"center": [30, 30], "radius": 10}))["created"]["curves"][0]
        .as_str()
        .unwrap_or_default()
        .to_string();
    run(&mut s, "sketch.circle.center", json!({"center": [30, 30], "radius": 4}));
    let r = run(&mut s, "sketch.cut_colour", json!({"curves": [hole], "colour": "orange"}));
    assert_eq!(r["colour"], "orange");
    let r = run(&mut s, "sketch.cut_colour", json!({"curves": [edge], "colour": 3}));
    assert_eq!(r["curves"].as_array().map(Vec::len), Some(4), "one edge colours the rectangle's four: {r}");
    assert!(s.execute("sketch.cut_colour", &json!({"curves": [edge], "colour": "pink"})).is_err());
    let dxf = run(&mut s, "sketch.export_cut_dxf", json!({}))["dxf"].as_str().unwrap_or_default().to_string();
    let ents = entity_colours(&dxf);
    let orange = solidcraft_sketch::cut_colour::aci(5).to_string();
    assert!(ents.iter().any(|e| (e.0.as_str(), e.1.as_str(), e.2.as_str()) == ("INNER_ORANGE", orange.as_str(), "16744448")), "{ents:?}");
    let green = solidcraft_sketch::cut_colour::aci(3).to_string();
    assert_eq!(ents.iter().filter(|e| e.0 == "OUTER_GREEN" && e.1 == green && e.2 == "57344").count(), 4, "{ents:?}");
    // The Ø8 island keeps red.
    assert!(ents.iter().any(|e| e.0 == "OUTER" && e.2 == "16711680"), "{ents:?}");
    // Colour taken off again: the plain layers.
    run(&mut s, "sketch.cut_colour", json!({"curves": [edge], "colour": null}));
    let dxf = run(&mut s, "sketch.export_cut_dxf", json!({}))["dxf"].as_str().unwrap_or_default().to_string();
    assert!(!entity_layers(&dxf).iter().any(|l| l == "OUTER_GREEN"));
}
