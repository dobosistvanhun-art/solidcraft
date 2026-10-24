use serde_json::{Value, json};

use crate::Session;

fn run(s: &mut Session, id: &str, p: Value) -> Value {
    s.execute(id, &p).unwrap_or_else(|e| panic!("{id} {p}: {e}"))
}

/// A 100 × 50 × 20 plate (x, y, z from the origin) with a 20 mm hole through its middle.
fn plate() -> Session {
    let mut s = Session::default();
    run(&mut s, "solid.box", json!({"length": 100, "width": 50, "height": 20}));
    run(&mut s, "solid.cylinder", json!({"diameter": 20, "height": 30, "base": [50, 25, -5], "operation": "cut"}));
    s
}

fn view_frame(r: &Value, id: u64) -> (f64, f64, f64, f64) {
    let v = r["views"].as_array().and_then(|vs| vs.iter().find(|v| v["id"] == id).cloned()).unwrap_or_else(|| panic!("view {id}: {r}"));
    (v["min"][0].as_f64().unwrap(), v["min"][1].as_f64().unwrap(), v["max"][0].as_f64().unwrap(), v["max"][1].as_f64().unwrap())
}

fn texts(r: &Value) -> Vec<String> {
    r["prims"]
        .as_array()
        .map(|a| a.iter().filter(|p| p["type"] == "text").filter_map(|p| p["text"].as_str().map(str::to_string)).collect())
        .unwrap_or_default()
}

fn count(r: &Value, style: &str) -> usize {
    r["prims"].as_array().map(|a| a.iter().filter(|p| p["type"] == "line" && p["style"] == style).count()).unwrap_or(0)
}

#[test]
fn a_standard_drawing_lays_out_four_views_at_a_scale_that_fits() {
    let mut s = plate();
    let d = run(&mut s, "drawing.create", json!({"name": "Plate", "size": "A4"}));
    let id = d["drawing"].as_u64().unwrap();
    let dr = s.doc.drawings.first().cloned().unwrap();
    assert_eq!(dr.views.len(), 4);
    // With the iso view beside the top view the 100 mm plate fits A4 at 1:2, A3 at 1:1.
    assert_eq!(dr.scale, "1:2");
    assert_eq!(dr.title.title, s.doc.name);
    let r = run(&mut s, "drawing.render", json!({"drawing": id}));
    assert_eq!((r["width"].as_f64(), r["height"].as_f64()), (Some(297.0), Some(210.0)));
    assert_eq!(r["errors"].as_array().map(Vec::len), Some(0), "{}", r["errors"]);
    // Front view: 100 wide, 20 tall at 1:1; the top view below it 100 × 50; the side view 50 × 20.
    let ids: Vec<u64> = dr.views.iter().map(|v| v.id).collect();
    let (x0, y0, x1, y1) = view_frame(&r, ids[0]);
    assert!(((x1 - x0) - 50.0).abs() < 0.01 && ((y1 - y0) - 10.0).abs() < 0.01, "front {x0} {y0} {x1} {y1}");
    let (tx0, ty0, tx1, ty1) = view_frame(&r, ids[1]);
    assert!(((tx1 - tx0) - 50.0).abs() < 0.01 && ((ty1 - ty0) - 25.0).abs() < 0.01, "top");
    assert!(ty1 < y0, "first angle: the top view is below the front view");
    let (sx0, _, sx1, _) = view_frame(&r, ids[2]);
    assert!(((sx1 - sx0) - 25.0).abs() < 0.01 && sx0 > x1, "the side view is to the right");
    assert!(count(&r, "visible") > 20 && count(&r, "wide") >= 4);
    // The hole's hidden edges are left out unless asked for.
    assert_eq!(count(&r, "hidden"), 0);
    run(&mut s, "drawing.view.edit", json!({"drawing": id, "view": ids[0], "hidden": true}));
    let r = run(&mut s, "drawing.render", json!({"drawing": id}));
    assert!(count(&r, "hidden") >= 2, "the hole's two hidden lines in the front view");
    // The title block names the drawing and the scale.
    let t = texts(&r);
    assert!(t.iter().any(|x| x == "1:2") && t.iter().any(|x| x.starts_with("A4")), "{t:?}");
    let a3 = run(&mut s, "drawing.create", json!({"name": "A3", "size": "A3"}))["drawing"].as_u64().unwrap();
    assert_eq!(s.doc.drawings.iter().find(|d| d.id == a3).map(|d| d.scale.as_str()), Some("1:1"));
    // No two views overlap.
    let r = run(&mut s, "drawing.render", json!({"drawing": a3}));
    let frames: Vec<(f64, f64, f64, f64)> = s.doc.drawings.iter().find(|d| d.id == a3).unwrap().views.iter().map(|v| view_frame(&r, v.id)).collect();
    for (i, a) in frames.iter().enumerate() {
        for b in frames.iter().skip(i + 1) {
            assert!(a.2 < b.0 || b.2 < a.0 || a.3 < b.1 || b.3 < a.1, "{a:?} overlaps {b:?}");
        }
    }
}

#[test]
fn dimensions_measure_the_model_and_follow_it() {
    let mut s = plate();
    run(&mut s, "drawing.create", json!({"name": "D", "size": "A3", "scale": "1:1"}));
    let dr = s.doc.drawings.first().cloned().unwrap();
    let front = dr.views.first().map(|v| v.id).unwrap();
    let top = dr.views.get(1).map(|v| v.id).unwrap();
    let r = run(&mut s, "drawing.render", json!({"drawing": "D"}));
    let (x0, y0, x1, y1) = view_frame(&r, front);
    // The plate's length between the front view's lower corners.
    let dim = run(
        &mut s,
        "drawing.dimension",
        json!({"drawing": "D", "view": front, "kind": "horizontal", "points": [[x0 + 0.5, y0 + 0.3], [x1 - 0.2, y0]], "offset": -10}),
    );
    assert!(dim["dimension"].as_u64().is_some());
    let height = run(
        &mut s,
        "drawing.dimension",
        json!({"drawing": "D", "view": front, "kind": "vertical", "points": [[x1, y0], [x1, y1]], "tolerance": "±0.1"}),
    );
    assert!(height["dimension"].as_u64().is_some());
    // The hole's diameter in the top view: a point on its circle.
    let (tx0, ty0, tx1, ty1) = view_frame(&r, top);
    let c = [(tx0 + tx1) / 2.0, (ty0 + ty1) / 2.0];
    run(&mut s, "drawing.dimension", json!({"drawing": "D", "view": top, "kind": "diameter", "points": [[c[0] + 10.0, c[1]]]}));
    let r = run(&mut s, "drawing.render", json!({"drawing": "D"}));
    let t = texts(&r);
    assert!(t.iter().any(|x| x == "100"), "{t:?}");
    assert!(t.iter().any(|x| x == "20 ±0,1"), "{t:?}");
    assert!(t.iter().any(|x| x == "Ø20"), "{t:?}");
    assert!(r["prims"].as_array().is_some_and(|a| a.iter().filter(|p| p["type"] == "fill").count() >= 5), "arrowheads");
    // Nothing there: refused.
    assert!(s.execute("drawing.dimension", &json!({"drawing": "D", "view": front, "kind": "horizontal", "points": [[5, 5], [6, 6]]})).is_err());
    // The model gets longer: the dimension follows.
    let boxf = s.doc.features.first().cloned().unwrap();
    let length = boxf.param_names.first().cloned().unwrap();
    run(&mut s, "parameters.change", json!({"name": length, "expression": "120 mm"}));
    let t = texts(&run(&mut s, "drawing.render", json!({"drawing": "D"})));
    assert!(t.iter().any(|x| x == "120"), "{t:?}");
    // Deleting the front view takes its projected views and every dimension on them.
    run(&mut s, "drawing.view.delete", json!({"drawing": "D", "view": front}));
    let dr = s.doc.drawings.first().cloned().unwrap();
    assert_eq!(dr.views.len(), 1, "the iso view is a base view of its own: {:?}", dr.views);
    assert!(dr.dims.is_empty());
}

#[test]
fn a_section_cuts_the_part_and_hatches_the_cut() {
    let mut s = plate();
    run(&mut s, "drawing.create", json!({"name": "D", "size": "A3", "scale": "1:1", "views": "none"}));
    let front = run(&mut s, "drawing.view.base", json!({"drawing": "D", "orientation": "top", "at": [120, 200]}))["view"].as_u64().unwrap();
    // A horizontal cutting line across the top view, through the hole.
    let sec = run(&mut s, "drawing.view.section", json!({"drawing": "D", "parent": front, "line": [[60, 200], [180, 200]], "at": [120, 120]}));
    let sec = sec["view"].as_u64().unwrap();
    let dr = s.doc.drawings.first().cloned().unwrap();
    assert_eq!(dr.view(sec).map(|v| v.label.as_str()), Some("A"));
    let r = run(&mut s, "drawing.render", json!({"drawing": "D"}));
    assert_eq!(r["errors"].as_array().map(Vec::len), Some(0), "{}", r["errors"]);
    // The section through the hole: two hatched rectangles beside it, 100 wide overall, 20 tall.
    let (x0, y0, x1, y1) = view_frame(&r, sec);
    assert!(((x1 - x0) - 100.0).abs() < 0.05 && ((y1 - y0) - 20.0).abs() < 0.05, "{x0} {y0} {x1} {y1}");
    let hatch: Vec<&Value> = r["prims"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter(|p| p["type"] == "line" && p["style"] == "thin")
                .filter(|p| {
                    [&p["a"], &p["b"]].iter().all(|q| {
                        q[0].as_f64().is_some_and(|x| x > x0 - 0.01 && x < x1 + 0.01) && q[1].as_f64().is_some_and(|y| y > y0 - 0.01 && y < y1 + 0.01)
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    assert!(hatch.len() > 20, "hatching: {}", hatch.len());
    // None of the hatching falls in the hole (20 wide in the middle).
    let mid = (x0 + x1) / 2.0;
    assert!(hatch.iter().all(|p| [&p["a"], &p["b"]].iter().all(|q| (q[0].as_f64().unwrap_or(0.0) - mid).abs() > 9.9)), "no hatching in the hole");
    let t = texts(&r);
    assert!(t.iter().any(|x| x == "A–A") && t.iter().filter(|x| *x == "A").count() == 2, "{t:?}");
    // A detail view at twice the scale.
    let det =
        run(&mut s, "drawing.view.detail", json!({"drawing": "D", "parent": front, "center": [130, 200], "radius": 15, "at": [300, 200]}))["view"]
            .as_u64()
            .unwrap();
    let r = run(&mut s, "drawing.render", json!({"drawing": "D"}));
    let (dx0, _, dx1, _) = view_frame(&r, det);
    assert!(((dx1 - dx0) - 60.0).abs() < 0.5, "a 15 mm circle at 2:1: {}", dx1 - dx0);
    assert!(texts(&r).iter().any(|x| x == "B (2:1)"));
}

#[test]
fn a_drawing_exports_as_pdf_and_dxf_and_is_saved_with_the_design() {
    let mut s = plate();
    run(&mut s, "drawing.create", json!({"name": "D"}));
    run(&mut s, "drawing.edit", json!({"drawing": "D", "title": {"title": "Lemez", "drawn_by": "István", "number": "ŐŰ-1"}}));
    run(&mut s, "drawing.note", json!({"drawing": "D", "at": [30, 60], "text": "Élek tompítva 0,5×45°\nSorjamentes"}));
    let pdf = run(&mut s, "drawing.export_pdf", json!({"drawing": "D"}));
    let bytes = solidcraft_doc::canvas::base64_decode(pdf["pdf_base64"].as_str().unwrap()).unwrap();
    assert!(bytes.starts_with(b"%PDF-1.4") && bytes.len() > 2000, "{}", bytes.len());
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains("/MediaBox [0 0 1190.551 841.89]"), "A3 landscape");
    assert!(text.contains("(Lemez)") && text.contains("(Istv\\341n)"), "the title block");
    let dxf = run(&mut s, "drawing.export_dxf", json!({"drawing": "D"}))["dxf"].as_str().unwrap().to_string();
    for layer in ["VISIBLE", "FRAME", "TEXT", "THIN"] {
        assert!(dxf.contains(layer), "{layer}");
    }
    // Saved and opened again, unchanged; undo takes the note away without touching the model.
    let doc = solidcraft_doc::Document::from_json(&s.doc.to_json()).unwrap();
    assert_eq!(doc.drawings, s.doc.drawings);
    let rev = s.revision;
    run(&mut s, "edit.undo", json!({}));
    assert!(s.doc.drawings.first().is_some_and(|d| d.notes.is_empty()));
    assert!(s.revision > rev);
    let listed = run(&mut s, "drawing.list", json!({}));
    assert_eq!(listed["drawings"][0]["views"], 4);
}

#[test]
fn hostile_drawing_parameters_are_refused() {
    let mut s = plate();
    run(&mut s, "drawing.create", json!({"name": "D", "views": "none"}));
    for (id, p) in [
        ("drawing.create", json!({"size": "A9"})),
        ("drawing.create", json!({"scale": "1:0"})),
        ("drawing.create", json!({"name": "D"})),
        ("drawing.edit", json!({"drawing": 99})),
        ("drawing.edit", json!({"drawing": "D", "projection": "sideways"})),
        ("drawing.view.base", json!({"drawing": "D", "orientation": "under"})),
        ("drawing.view.base", json!({"drawing": "D", "at": [1e300, 0]})),
        ("drawing.view.projected", json!({"drawing": "D", "parent": 42, "side": "right"})),
        ("drawing.view.section", json!({"drawing": "D", "parent": 1, "line": [[0, 0], [0, 0]]})),
        ("drawing.view.detail", json!({"drawing": "D", "parent": 1, "center": [0, 0], "radius": -1})),
        ("drawing.dimension", json!({"drawing": "D", "view": 1, "kind": "fancy", "points": []})),
        ("drawing.note", json!({"drawing": "D", "at": [0, 0], "text": ""})),
        ("drawing.note", json!({"drawing": "D", "at": [0, 0], "text": "x", "height": 1e9})),
        ("drawing.export_pdf", json!({"drawing": "D", "path": 5})),
    ] {
        assert!(s.execute(id, &p).is_err(), "{id} {p}");
    }
    // A damaged drawing is drawn as a sheet with an error, never a crash.
    if let Some(d) = s.doc_mut().drawings.first_mut() {
        d.views.push(solidcraft_doc::drawing::DrawView {
            id: 7,
            kind: solidcraft_doc::drawing::ViewKind::Detail { parent: 7, center: [f64::NAN, 0.0], radius: 1.0 },
            at: [0.0, 0.0],
            scale: None,
            label: "Z".into(),
            hidden: false,
        });
    }
    let r = run(&mut s, "drawing.render", json!({"drawing": "D"}));
    assert!(r["errors"].as_array().is_some_and(|e| !e.is_empty()));
}

/// Writes a sample sheet for looking at: `SOLIDCRAFT_DRAWING_SAMPLE=/path/sample.pdf cargo test … -- --ignored`.
#[test]
#[ignore]
fn write_a_sample_sheet() {
    let Ok(path) = std::env::var("SOLIDCRAFT_DRAWING_SAMPLE") else { return };
    let mut s = plate();
    run(&mut s, "drawing.create", json!({"name": "Minta", "size": "A3"}));
    run(
        &mut s,
        "drawing.edit",
        json!({"drawing": "Minta", "title": {"title": "Lemez furattal", "drawn_by": "István", "number": "SC-0001", "company": "Lézer műhely", "tolerances": "ISO 2768-m"}}),
    );
    let dr = s.doc.drawings.first().cloned().unwrap();
    let (front, top) = (dr.views[0].id, dr.views[1].id);
    run(&mut s, "drawing.view.edit", json!({"drawing": "Minta", "view": front, "hidden": true}));
    let r = run(&mut s, "drawing.render", json!({"drawing": "Minta"}));
    let (x0, y0, x1, y1) = view_frame(&r, front);
    run(&mut s, "drawing.dimension", json!({"drawing": "Minta", "view": front, "kind": "horizontal", "points": [[x0, y1], [x1, y1]]}));
    run(
        &mut s,
        "drawing.dimension",
        json!({"drawing": "Minta", "view": front, "kind": "vertical", "points": [[x0, y0], [x0, y1]], "offset": -10, "tolerance": "±0.1"}),
    );
    let (tx0, ty0, tx1, ty1) = view_frame(&r, top);
    let c = [(tx0 + tx1) / 2.0, (ty0 + ty1) / 2.0];
    let k = s.doc.drawings.first().map(|d| d.scale_factor()).unwrap_or(1.0);
    run(
        &mut s,
        "drawing.dimension",
        json!({"drawing": "Minta", "view": top, "kind": "diameter", "points": [[c[0] + 10.0 * k, c[1]]], "tolerance": "H7"}),
    );
    run(&mut s, "drawing.view.section", json!({"drawing": "Minta", "parent": top, "line": [[tx0 - 5.0, c[1]], [tx1 + 5.0, c[1]]], "at": [330, 260]}));
    run(&mut s, "drawing.note", json!({"drawing": "Minta", "at": [30, 40], "text": "Élek tompítva 0,5×45°\nSorjamentes, őrölt felület"}));
    run(&mut s, "drawing.export_pdf", json!({"drawing": "Minta", "path": path}));
    let dxf = path.replace(".pdf", ".dxf");
    run(&mut s, "drawing.export_dxf", json!({"drawing": "Minta", "path": dxf}));
}

#[test]
fn a_dimension_between_two_arcs_measures_to_their_centres_near_or_far_sides() {
    let mut s = Session::default();
    run(&mut s, "solid.box", json!({"length": 100, "width": 50, "height": 10}));
    for x in [30, 70] {
        run(&mut s, "solid.cylinder", json!({"diameter": 20, "height": 30, "base": [x, 25, -5], "operation": "cut"}));
    }
    run(&mut s, "drawing.create", json!({"name": "D", "size": "A3", "scale": "1:1", "views": "none"}));
    let top = run(&mut s, "drawing.view.base", json!({"drawing": "D", "orientation": "top", "at": [200, 150]}))["view"].as_u64().unwrap();
    // The holes' centres on the sheet: (180, 150) and (220, 150); a point on each circle's edge.
    let (a, b) = ([180.0, 160.0], [220.0, 140.0]);
    for (arc, want) in [("max", "60"), ("min", "20"), ("center", "40")] {
        let id =
            run(&mut s, "drawing.dimension.arcs", json!({"drawing": "D", "view": top, "points": [a, b], "arc": arc}))["dimension"].as_u64().unwrap();
        let t = texts(&run(&mut s, "drawing.render", json!({"drawing": "D"})));
        assert!(t.iter().any(|x| x == want), "{arc}: {t:?}");
        run(&mut s, "drawing.dimension.delete", json!({"drawing": "D", "dimension": id}));
    }
    // Arc to Arc measures over the far sides unless told; switching the side is an edit.
    let id = run(&mut s, "drawing.dimension.arcs", json!({"drawing": "D", "view": top, "points": [a, b]}))["dimension"].as_u64().unwrap();
    assert_eq!(s.doc.drawings[0].dims[0].arc, "max");
    run(&mut s, "drawing.dimension.edit", json!({"drawing": "D", "dimension": id, "arc": "min"}));
    assert!(texts(&run(&mut s, "drawing.render", json!({"drawing": "D"}))).iter().any(|x| x == "20"));
    assert!(s.execute("drawing.dimension", &json!({"drawing": "D", "view": top, "kind": "diameter", "points": [a], "arc": "max"})).is_err());
    assert!(s.execute("drawing.dimension.arcs", &json!({"drawing": "D", "view": top, "points": [a, b], "arc": "outside"})).is_err());
    for (cmd, kind) in [("drawing.dimension.vertical", "vertical"), ("drawing.dimension.aligned", "aligned")] {
        run(&mut s, cmd, json!({"drawing": "D", "view": top, "points": [[150, 125], [150, 175]]}));
        let last = s.doc.drawings[0].dims.last().map(|d| serde_json::to_value(d.kind).unwrap()).unwrap();
        assert_eq!(last, kind);
    }
}

#[test]
fn a_dimension_to_virtual_sharps_measures_the_corners_as_if_they_werent_rounded() {
    let mut s = Session::default();
    // A 100 × 50 × 10 plate with its four vertical edges rounded R10.
    run(&mut s, "solid.box", json!({"length": 100, "width": 50, "height": 10}));
    let corners: Vec<[f64; 3]> = vec![[0.0, 0.0, 5.0], [100.0, 0.0, 5.0], [100.0, 50.0, 5.0], [0.0, 50.0, 5.0]];
    run(&mut s, "solid.fillet", json!({"edges": corners, "radius": 10}));
    assert!(s.model.results.iter().all(|r| r.error.is_none()), "{:?}", s.model.results);
    run(&mut s, "drawing.create", json!({"name": "D", "size": "A3", "scale": "1:1", "views": "none"}));
    let top = run(&mut s, "drawing.view.base", json!({"drawing": "D", "orientation": "top", "at": [200, 150]}))["view"].as_u64().unwrap();
    // The top view spans x 150…250, y 125…175 on the sheet; the sharp corners are its corners.
    let len =
        run(&mut s, "drawing.dimension.horizontal", json!({"drawing": "D", "view": top, "points": [[150.5, 175.5], [249.5, 175.5]], "arc": "sharp"}));
    assert!(len["dimension"].as_u64().is_some());
    let r = run(&mut s, "drawing.render", json!({"drawing": "D"}));
    assert!(texts(&r).iter().any(|x| x == "100"), "{:?}", texts(&r));
    // An aligned one between diagonal corners: √(100² + 50²).
    run(&mut s, "drawing.dimension.aligned", json!({"drawing": "D", "view": top, "points": [[150, 125], [250, 175]], "arc": "sharp"}));
    let t = texts(&run(&mut s, "drawing.render", json!({"drawing": "D"})));
    assert!(t.iter().any(|x| x == "111,8"), "{t:?}");
    // Points not at a rounded corner are refused.
    assert!(
        s.execute("drawing.dimension.horizontal", &json!({"drawing": "D", "view": top, "points": [[200, 150], [210, 150]], "arc": "sharp"})).is_err()
    );
}

/// A 100 × 100 square to a Ø60 circle 80 above it, as a sheet metal loft.
fn lofted_sheet() -> Session {
    let mut s = Session::default();
    run(&mut s, "sketch.create", json!({"plane": "XY", "name": "Square"}));
    run(&mut s, "sketch.rectangle.two_point", json!({"p0": [-50, -50], "p1": [50, 50]}));
    run(&mut s, "sketch.finish", json!({}));
    run(&mut s, "sketch.create", json!({"plane": {"origin": [0, 0, 80], "x_dir": [1, 0, 0], "y_dir": [0, 1, 0]}, "name": "Round"}));
    run(&mut s, "sketch.circle.center", json!({"center": [0, 0], "radius": 30}));
    run(&mut s, "sketch.finish", json!({}));
    run(&mut s, "sheet.loft", json!({"sections": ["Square", "Round"]}));
    s
}

/// A flat pattern view: the outline as cut, a chain line and a note for every bend, and a
/// dimension across it measuring the flat pattern's width.
#[test]
fn a_flat_pattern_view_shows_the_outline_and_bend_lines_and_takes_dimensions() {
    let mut s = lofted_sheet();
    let flat = run(&mut s, "sheet.flat_pattern", json!({}));
    let bends = flat["bends"].as_array().map(Vec::len).unwrap_or(0);
    assert!(bends > 0);
    let outline: Vec<(f64, f64)> =
        flat["outline"][0].as_array().into_iter().flatten().filter_map(|p| Some((p.get(0)?.as_f64()?, p.get(1)?.as_f64()?))).collect();
    let (x0, x1) = outline.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), p| (a.min(p.0), b.max(p.0)));
    let (y0, y1) = outline.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), p| (a.min(p.1), b.max(p.1)));
    let d = run(&mut s, "drawing.create", json!({"name": "D", "size": "A2", "views": "none"}));
    let id = d["drawing"].as_u64().unwrap();
    let before = s.doc.drawings[0].views.len();
    let v = run(&mut s, "drawing.view.flat", json!({"drawing": id, "at": [300, 200], "scale": "1:2"}));
    let view = v["view"].as_u64().unwrap();
    assert_eq!(s.doc.drawings[0].views.len(), before + 1);
    let r = run(&mut s, "drawing.render", json!({"drawing": id}));
    assert_eq!(r["errors"].as_array().map(Vec::len), Some(0), "{}", r["errors"]);
    // Its frame is the flat pattern at 1:2 about its place.
    let (fx0, fy0, fx1, fy1) = view_frame(&r, view);
    assert!(((fx1 - fx0) - (x1 - x0) / 2.0).abs() < 0.05 && ((fy1 - fy0) - (y1 - y0) / 2.0).abs() < 0.05, "{fx0} {fy0} {fx1} {fy1}");
    assert!(((fx0 + fx1) / 2.0 - 300.0).abs() < 0.05 && ((fy0 + fy1) / 2.0 - 200.0).abs() < 0.05);
    assert!(count(&r, "chain") >= bends, "a centre line for each bend");
    assert!(texts(&r).iter().filter(|t| t.starts_with("UP ") || t.starts_with("DOWN ")).count() >= bends, "{:?}", texts(&r));
    // Across it: from the leftmost to the rightmost outline point, its width.
    let on_sheet = |p: (f64, f64)| json!([300.0 + (p.0 - (x0 + x1) / 2.0) / 2.0, 200.0 + (p.1 - (y0 + y1) / 2.0) / 2.0]);
    let left = outline.iter().copied().min_by(|a, b| a.0.total_cmp(&b.0)).unwrap();
    let right = outline.iter().copied().max_by(|a, b| a.0.total_cmp(&b.0)).unwrap();
    run(&mut s, "drawing.dimension", json!({"drawing": id, "view": view, "kind": "horizontal", "points": [on_sheet(left), on_sheet(right)]}));
    let r = run(&mut s, "drawing.render", json!({"drawing": id}));
    let want = crate::cmd::view_dxf::drawing_views::number(x1 - x0);
    assert!(texts(&r).contains(&want), "{want} in {:?}", texts(&r));
    // Nothing is projected, cut or enlarged from it.
    assert!(s.execute("drawing.view.projected", &json!({"drawing": id, "parent": view, "side": "right"})).is_err());
    assert!(s.execute("drawing.view.detail", &json!({"drawing": id, "parent": view, "center": [300, 200], "radius": 10})).is_err());
    // Without a sheet metal body there is none to show.
    let mut p = plate();
    let d = run(&mut p, "drawing.create", json!({"name": "P"}));
    assert!(p.execute("drawing.view.flat", &json!({"drawing": d["drawing"]})).is_err());
    assert!(s.execute("drawing.view.flat", &json!({"drawing": id, "body": "nope"})).is_err());
}
