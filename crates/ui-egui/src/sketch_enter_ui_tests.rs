//! Inline dimensions without Tab: Enter after typing a value moves on to the next box, Enter in
//! a box left as it is places the shape.

use serde_json::json;

use crate::scenario::Harness;

fn type_value(h: &mut Harness, text: &str) {
    h.step(&json!({"key": "A", "cmd": true})).unwrap();
    h.step(&json!({"text": text})).unwrap();
}

fn areas(h: &Harness) -> Vec<f64> {
    let state = h.app.session.world_state();
    let solved = state.sketch(h.app.session.active_sketch.unwrap()).unwrap();
    solved.profiles.iter().map(|p| p.area).collect()
}

fn start_rectangle(h: &mut Harness) {
    h.app.run("sketch.create", json!({"plane": "XY"})).unwrap();
    h.step(&json!({"start": "sketch.rectangle.two_point"})).unwrap();
    h.step(&json!({"click": {"world": [0, 0, 0]}})).unwrap();
    h.step(&json!({"move": {"world": [10, 10, 0]}})).unwrap();
}

#[test]
fn enter_moves_from_the_typed_width_to_the_height() {
    let mut h = Harness::new();
    start_rectangle(&mut h);
    type_value(&mut h, "40");
    h.step(&json!({"key": "Enter"})).unwrap();
    // Not placed yet: the height box has the keyboard.
    assert!(areas(&h).is_empty(), "{:?}", areas(&h));
    type_value(&mut h, "25");
    h.step(&json!({"key": "Enter"})).unwrap();
    h.step(&json!({"key": "Escape"})).unwrap();
    h.settle();
    let a = areas(&h);
    assert_eq!(a.len(), 1, "{a:?}");
    assert!((a[0] - 1000.0).abs() < 1e-6, "{a:?}");
}

#[test]
fn enter_in_the_untouched_height_places_the_rectangle() {
    let mut h = Harness::new();
    start_rectangle(&mut h);
    type_value(&mut h, "40");
    h.step(&json!({"key": "Enter"})).unwrap();
    h.step(&json!({"key": "Enter"})).unwrap();
    h.step(&json!({"key": "Escape"})).unwrap();
    h.settle();
    let a = areas(&h);
    // 40 wide, as high as the pointer was (10).
    assert_eq!(a.len(), 1, "{a:?}");
    assert!((a[0] - 400.0).abs() < 1e-3, "{a:?}");
}
