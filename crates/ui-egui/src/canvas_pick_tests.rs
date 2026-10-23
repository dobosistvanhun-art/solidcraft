//! A reference image (canvas) is picked by clicking it in the viewport and goes with Delete,
//! the ribbon's Delete Canvas or its menu, while the sketch traced over it stays.

use serde_json::json;

use crate::scenario::Harness;

fn png(w: u32, h: u32) -> String {
    let mut b = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
    b.extend(w.to_be_bytes());
    b.extend(h.to_be_bytes());
    b.extend([8, 6, 0, 0, 0, 0, 0, 0, 0]);
    solidcraft_engine::doc::canvas::base64_encode(&b)
}

/// An 80 × 40 canvas on XY and an open outline traced round it.
fn traced() -> Harness {
    let mut h = Harness::new();
    h.app.home.open = false;
    h.app.run("canvas.insert", json!({"data": png(400, 200), "plane": "XY", "width": 80, "opacity": 0.6})).unwrap();
    h.app.run("sketch.create", json!({"plane": "XY", "name": "Trace"})).unwrap();
    // An outline traced round the image (no region inside it to pick instead of the image).
    h.app.run("sketch.line", json!({"points": [[-45, -25], [45, -25], [45, 25], [-45, 25]]})).unwrap();
    h.app.finish_sketch();
    h.app.animate_view("top");
    h.settle();
    h.app.animate_view("fit");
    h.settle();
    h
}

#[test]
fn a_canvas_clicked_in_the_viewport_is_picked_and_deleted() {
    let mut h = traced();
    let id = h.app.session.doc.canvases[0].id;
    h.step(&json!({"click": {"world": [-20, 5, 0]}})).unwrap();
    h.frames(2);
    assert_eq!(h.app.tree.picked_canvases, vec![id]);
    // A click on nothing lets it go; another click picks it again.
    h.step(&json!({"click": {"world": [-42.5, 0, 0]}})).unwrap();
    h.frames(1);
    assert!(h.app.tree.picked_canvases.is_empty());
    h.step(&json!({"click": {"world": [10, -10, 0]}})).unwrap();
    h.frames(1);
    assert_eq!(h.app.tree.picked_canvases, vec![id]);
    crate::delete::delete_selection(&mut h.app);
    assert!(h.app.session.doc.canvases.is_empty());
    assert!(h.app.session.world_state().sketches.iter().any(|s| s.name == "Trace"));
}

#[test]
fn the_ribbon_deletes_the_only_canvas_and_the_panel_has_a_delete_button() {
    let mut h = traced();
    h.app.start("canvas.delete");
    h.frames(2);
    assert!(h.app.session.doc.canvases.is_empty());
    assert!(h.app.session.world_state().sketches.iter().any(|s| s.name == "Trace"));
    // The Edit Canvas panel's own Delete.
    h.app.run("canvas.insert", json!({"data": png(400, 200), "plane": "XY", "width": 80})).unwrap();
    h.app.start("canvas.edit");
    h.frames(3);
    let at = h.widget("Delete Canvas").expect("the panel's Delete Canvas button");
    h.step(&json!({"click": at})).unwrap();
    h.frames(2);
    assert!(h.app.session.doc.canvases.is_empty());
}
