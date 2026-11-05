//! Cut Colour: the picked contours take the colour clicked, No Colour takes it off, and with
//! nothing picked the dialog says so.

use serde_json::json;
use solidcraft_engine::Sel;

use super::*;

/// A sketch being edited with a 100 × 60 rectangle; its first edge's id.
fn plate(h: &mut crate::scenario::Harness) -> String {
    h.app.run("sketch.create", json!({"plane": "XY"})).unwrap();
    let r = h.app.run("sketch.rectangle.two_point", json!({"p0": [0, 0], "p1": [100, 60]})).unwrap();
    r["curves"][0].as_str().unwrap_or_default().to_string()
}

fn colours(h: &crate::scenario::Harness) -> Vec<Option<u8>> {
    let id = h.app.session.active_sketch.unwrap_or_default();
    let st = h.app.session.model.state();
    st.sketch(id).map(|ss| ss.sketch.curves.iter().map(|c| c.cut_colour).collect()).unwrap_or_default()
}

/// The ribbon's Cut Colour opens the dialog with the picked curves; a colour gives the whole
/// rectangle that colour, No Colour takes it off.
#[test]
fn the_picked_contour_takes_the_colour_and_no_colour_takes_it_off() {
    let mut h = crate::scenario::Harness::new();
    let edge = plate(&mut h);
    h.app.session.selection = vec![Sel::SketchCurve { id: edge, sketch: None }];
    h.app.start("sketch.cut_colour");
    h.frames(3);
    let Some(d) = h.app.dialog.take() else { panic!("the dialog opened") };
    assert!(matches!(d.kind, Kind::CutColour(_)));
    assert_eq!(d.inputs[0].items.len(), 1, "the picked curve is in the dialog");
    let mut k = CutColourUi::default();
    apply(&mut h.app, &mut k, &d.inputs, Some(5));
    assert_eq!(colours(&h), vec![Some(5); 4], "the rectangle's four edges are orange: {:?}", k.status);
    h.app.dialog = Some(d);
    h.frames(2);
    let at = h.widget("No colour");
    assert!(at.is_some(), "the dialog has No colour");
    if let Some([x, y]) = at {
        h.call("ui.click", json!({"x": x, "y": y}));
    }
    h.frames(3);
    assert_eq!(colours(&h), vec![None; 4]);
}

/// With nothing picked a colour does nothing but say what to do.
#[test]
fn nothing_picked_says_so() {
    let mut h = crate::scenario::Harness::new();
    plate(&mut h);
    let mut k = CutColourUi::default();
    apply(&mut h.app, &mut k, &[SelInput::new("Contours", CURVES, true)], Some(2));
    assert!(k.status.is_some());
    assert_eq!(colours(&h), vec![None; 4]);
}

/// Every colour has a label: the named ones their name and LightBurn number, the rest the number.
#[test]
fn every_colour_has_a_label() {
    assert_eq!(label(5), "Orange (05)");
    assert_eq!(label(21), "21");
    assert_eq!(NAMES.len(), 17);
}
