use serde_json::{Value, json};

use crate::Session;
use crate::schema::ParamCheck;

fn run(s: &mut Session, id: &str, p: Value) -> Value {
    match s.execute(id, &p) {
        Ok(v) => v,
        Err(e) => panic!("{id} {p}: {e}"),
    }
}

fn volume(s: &mut Session, body: &str) -> f64 {
    run(s, "inspect.measure", json!({"bodies": [body]}))["total"]["volume_mm3"].as_f64().unwrap_or(f64::NAN)
}

/// Each piece's body holds as much sheet as its flat pattern, the pattern's area times the
/// thickness (the pattern is on the middle surface), within `tol`.
fn pieces_match_their_patterns(s: &mut Session, count: usize, tol: f64) -> Value {
    let pats = run(s, "fitting.patterns", json!({}));
    let pieces = pats["pieces"].as_array().cloned().unwrap_or_default();
    assert_eq!(pieces.len(), count, "{pats}");
    let t = pats["thickness"].as_f64().unwrap_or(0.0);
    for p in &pieces {
        let body = p["body"].as_str().unwrap_or_else(|| panic!("a piece without its body: {p}"));
        let v = volume(s, body);
        let want = p["area"].as_f64().unwrap_or(0.0) * t;
        assert!((v - want).abs() / want < tol, "{}: body {v} mm³, pattern × t {want} mm³", p["piece"]);
    }
    pats
}

#[test]
fn a_tee_is_two_bodies_with_exact_flat_patterns() {
    let mut s = Session::default();
    let r = run(&mut s, "fitting.tee", json!({"diameter": 100, "height": 200, "branch": 60, "branch_length": 120, "thickness": 1}));
    assert_eq!(r["created_bodies"].as_array().map(Vec::len), Some(2), "{r}");
    let pats = pieces_match_their_patterns(&mut s, 2, 0.02);
    // the body is the pipe's middle circumference round, its height high
    let body = &pats["pieces"][0];
    assert_eq!(body["piece"], "body");
    assert!((body["width"].as_f64().unwrap_or(0.0) - 99.0 * std::f64::consts::PI).abs() < 0.2, "{body}");
    assert!((body["height"].as_f64().unwrap_or(0.0) - 200.0).abs() < 1e-6, "{body}");
    assert_eq!(body["holes"].as_array().map(Vec::len), Some(1), "the branch's opening");
    // the branch: its middle circumference round
    let branch = &pats["pieces"][1];
    assert!((branch["width"].as_f64().unwrap_or(0.0) - 59.0 * std::f64::consts::PI).abs() < 0.2, "{branch}");
}

#[test]
fn an_eccentric_cone_takes_a_branch_into_its_mantle() {
    let mut s = Session::default();
    run(
        &mut s,
        "fitting.cone",
        json!({"diameter": 200, "top": 100, "height": 150, "shift": 50, "branch": 50, "branch_at": 70, "branch_angle": "75 deg", "thickness": 1.5}),
    );
    pieces_match_their_patterns(&mut s, 2, 0.03);
}

#[test]
fn a_plain_cone_is_one_body() {
    let mut s = Session::default();
    run(&mut s, "fitting.cone", json!({"diameter": 200, "top": 120, "height": 100, "thickness": 1, "measured": "inside"}));
    pieces_match_their_patterns(&mut s, 1, 0.01);
}

#[test]
fn an_elbow_has_a_body_and_a_pattern_for_each_segment() {
    let mut s = Session::default();
    let r = run(&mut s, "fitting.elbow", json!({"diameter": 100, "thickness": 1, "segments": 4, "angle": "90 deg", "bend": 150}));
    assert_eq!(r["created_bodies"].as_array().map(Vec::len), Some(4), "{r}");
    let pats = pieces_match_their_patterns(&mut s, 4, 0.02);
    // the two end segments are half the middle ones
    let a = |k: usize| pats["pieces"][k]["area"].as_f64().unwrap_or(0.0);
    assert!((a(1) - 2.0 * a(0)).abs() / a(1) < 1e-6, "{pats}");
    assert!((a(0) - a(3)).abs() / a(0) < 1e-6, "{pats}");
}

#[test]
fn a_fittings_patterns_export_as_one_dxf_a_piece() {
    let mut s = Session::default();
    run(&mut s, "fitting.tee", json!({"diameter": 100, "height": 200, "branch": 60, "thickness": 1}));
    let r = run(&mut s, "fitting.export_dxf", json!({}));
    let pieces = r["pieces"].as_array().cloned().unwrap_or_default();
    assert_eq!(pieces.len(), 2, "{r}");
    let body = pieces[0]["dxf"].as_str().unwrap_or_default();
    assert!(body.contains("OUTLINE") && body.contains("\nCUT") && body.contains("MARK"), "the body's outline, its opening and the marks");
    let one = run(&mut s, "fitting.export_dxf", json!({"piece": "branch"}));
    assert_eq!(one["pieces"].as_array().map(Vec::len), Some(1), "{one}");
    assert!(s.execute("fitting.export_dxf", &json!({"piece": "segment 9"})).is_err());
}

#[test]
fn fittings_refuse_what_they_cannot_make() {
    let mut s = Session { param_check: ParamCheck::Strict, ..Session::default() };
    // a branch wider than its body, an elbow of one segment, no sheet, an operation
    for (cmd, p) in [
        ("fitting.tee", json!({"diameter": 50, "height": 100, "branch": 80, "thickness": 1})),
        ("fitting.elbow", json!({"diameter": 50, "segments": 1, "thickness": 1})),
        ("fitting.elbow", json!({"diameter": 50, "thickness": 0})),
        ("fitting.cone", json!({"diameter": 50, "top": 30, "height": 40, "thickness": 1, "operation": "join"})),
    ] {
        assert!(s.execute(cmd, &p).is_err(), "{cmd} {p}");
    }
    assert!(s.execute("fitting.patterns", &json!({})).is_err(), "no fitting yet");
}

#[test]
fn a_drawing_shows_a_fitting_pieces_flat_pattern() {
    let mut s = Session::default();
    run(&mut s, "fitting.tee", json!({"diameter": 100, "height": 200, "branch": 60, "thickness": 1}));
    let pats = run(&mut s, "fitting.patterns", json!({"piece": "branch"}));
    let branch = pats["pieces"][0]["body"].as_str().unwrap_or_default().to_string();
    let width = pats["pieces"][0]["width"].as_f64().unwrap_or(0.0);
    let id = run(&mut s, "drawing.create", json!({"name": "D", "size": "A2", "views": "none"}))["drawing"].as_u64().unwrap_or(0);
    let v = run(&mut s, "drawing.view.flat", json!({"drawing": id, "body": branch, "at": [300, 200], "scale": "1:1"}));
    let view = v["view"].as_u64().unwrap_or(0);
    let r = run(&mut s, "drawing.render", json!({"drawing": id}));
    assert_eq!(r["errors"].as_array().map(Vec::len), Some(0), "{}", r["errors"]);
    let frame = r["views"].as_array().into_iter().flatten().find(|x| x["id"] == view).cloned().unwrap_or_default();
    let (x0, x1) = (frame["min"][0].as_f64().unwrap_or(0.0), frame["max"][0].as_f64().unwrap_or(0.0));
    assert!(((x1 - x0) - width).abs() < 0.5, "the view is the pattern's width at 1:1: {frame}");
}

/// A DN 50 PN 16 plate flange (EN 1092-1 type 01): 165 across, 4 holes of 18 on a 125 circle, a
/// 61.5 bore, 20 thick; its flat pattern is the plate, its body the pattern 20 deep.
#[test]
fn a_standard_flange_is_its_own_flat_pattern() {
    use std::f64::consts::PI;
    let mut s = Session::default();
    let r = run(&mut s, "fitting.flange", json!({"pn": 16, "dn": 50}));
    assert_eq!(r["created_bodies"].as_array().map(Vec::len), Some(1), "{r}");
    let pats = pieces_match_their_patterns(&mut s, 1, 0.002);
    let f = &pats["pieces"][0];
    assert_eq!(f["piece"], "flange");
    let want = PI * (82.5f64.powi(2) - 30.75f64.powi(2)) - 4.0 * PI * 81.0;
    assert!((f["area"].as_f64().unwrap_or(0.0) - want).abs() / want < 1e-3, "{f}");
    assert!((f["width"].as_f64().unwrap_or(0.0) - 165.0).abs() < 0.01, "{f}");
    assert_eq!(f["holes"].as_array().map(Vec::len), Some(5), "the bore and four bolt holes");
    assert_eq!(pats["thickness"], 20.0);
    assert_eq!(pats["exact"], true);
    // a blind one of PN 10 DN 200: eight holes, no bore; sizes of its own override the standard's
    run(&mut s, "fitting.flange", json!({"pn": 10, "dn": 200, "blind": true, "thickness": 12}));
    let pats = pieces_match_their_patterns(&mut s, 1, 0.002);
    assert_eq!(pats["pieces"][0]["holes"].as_array().map(Vec::len), Some(8), "{pats}");
    assert_eq!(pats["thickness"], 12.0);
    let dxf = run(&mut s, "fitting.export_dxf", json!({}));
    assert!(dxf["pieces"][0]["dxf"].as_str().is_some_and(|t| t.contains("\nCUT")), "the holes on the CUT layer");
}

/// A socket of sheet over a 60.3 pipe (its inside the pipe's), and a turned one with a G 2
/// thread inside: a cosmetic thread on its bore, no sheet pattern.
#[test]
fn a_socket_of_sheet_or_threaded() {
    use std::f64::consts::PI;
    let mut s = Session::default();
    run(&mut s, "fitting.socket", json!({"diameter": 60.3, "height": 80, "thickness": 2}));
    let pats = pieces_match_their_patterns(&mut s, 1, 0.01);
    let w = pats["pieces"][0]["width"].as_f64().unwrap_or(0.0);
    assert!((w - 62.3 * PI).abs() < 0.05, "the middle circumference of a sleeve 60.3 inside: {w}");
    let r = run(&mut s, "fitting.socket", json!({"diameter": 75, "height": 50, "thickness": 2, "thread": "G2"}));
    let id = r["feature"].as_u64().unwrap_or(0);
    let st = s.model.state();
    let th: Vec<_> = st.threads.iter().filter(|t| t.feature == id).collect();
    assert_eq!(th.len(), 1, "the socket's own thread, on its bore");
    assert!(th[0].internal && th[0].designation.starts_with("G2"), "{:?}", th[0]);
    // an edit keeps one thread, the fitting's own
    run(
        &mut s,
        "timeline.redefine",
        json!({"feature": id, "command": "fitting.socket", "params": {"diameter": 80, "height": 60, "thickness": 2, "thread": "G2"}}),
    );
    assert_eq!(s.model.state().threads.iter().filter(|t| t.feature == id).count(), 1);
    let pats = run(&mut s, "fitting.patterns", json!({}));
    assert_eq!(pats["pieces"].as_array().map(Vec::len), Some(0), "a turned socket has no sheet pattern");
}

/// A threaded end (nipple) G 1/2, 40 long, the thread 15 from its top.
#[test]
fn a_threaded_end_carries_its_pipe_thread() {
    let mut s = Session::default();
    let r = run(&mut s, "fitting.nipple", json!({"thread": "G1/2", "height": 40, "thickness": 2.6, "thread_length": 15}));
    assert_eq!(r["created_bodies"].as_array().map(Vec::len), Some(1), "{r}");
    let st = s.model.state();
    assert_eq!(st.threads.len(), 1);
    let t = &st.threads[0];
    assert!(!t.internal && (t.end - t.start - 15.0).abs() < 1e-6, "15 of thread outside: {t:?}");
    let v = volume(&mut s, r["created_bodies"][0].as_str().unwrap_or_default());
    let (ro, ri) = (20.955 / 2.0, 20.955 / 2.0 - 2.6);
    let want = std::f64::consts::PI * (ro * ro - ri * ri) * 40.0;
    assert!((v - want).abs() / want < 0.01, "{v} vs {want}");
}

/// A collar (bördel): a pipe turned out at its top; its pattern is the blank, not exact.
#[test]
fn a_collar_turns_out_its_pipe() {
    let mut s = Session::default();
    let r = run(&mut s, "fitting.collar", json!({"diameter": 100, "height": 80, "thickness": 1.5, "width": 25, "bend": 3}));
    assert_eq!(r["created_bodies"].as_array().map(Vec::len), Some(1), "{r}");
    let pats = run(&mut s, "fitting.patterns", json!({}));
    assert_eq!(pats["exact"], false, "{pats}");
    let p = &pats["pieces"][0];
    let body = p["body"].as_str().unwrap_or_default().to_string();
    let v = volume(&mut s, &body);
    // the collar's sheet spreads round as it turns out: the body holds more than its blank
    let blank = p["area"].as_f64().unwrap_or(0.0) * 1.5;
    assert!(v > blank && v < blank * 1.4, "{v} vs the blank's {blank}");
    let bb = run(&mut s, "inspect.measure", json!({"bodies": [body]}));
    assert!(bb.to_string().contains("volume"), "{bb}");
}

#[test]
fn flanges_and_threaded_ends_refuse_what_they_cannot_make() {
    let mut s = Session { param_check: ParamCheck::Strict, ..Session::default() };
    for (cmd, p) in [
        ("fitting.flange", json!({"pn": 25, "dn": 50})),
        ("fitting.flange", json!({"pn": 16, "dn": 55})),
        ("fitting.flange", json!({"diameter": 100, "bolt_circle": 80, "holes": 40, "hole": 14, "thickness": 10})),
        ("fitting.flange", json!({"diameter": 100, "thickness": 10})),
        ("fitting.nipple", json!({"thread": "G9/7", "height": 20, "thickness": 2})),
        ("fitting.nipple", json!({"height": 20, "thickness": 2})),
        ("fitting.socket", json!({"diameter": 20, "height": 30, "thickness": 2, "thread": "G1"})),
        ("fitting.collar", json!({"diameter": 100, "height": 50, "thickness": 2, "width": 3, "bend": 2})),
    ] {
        assert!(s.execute(cmd, &p).is_err(), "{cmd} {p}");
    }
    assert!(s.doc.features.is_empty(), "nothing was made");
}
