//! Threads of the other standards: Unified, Whitworth and pipe, NPT, trapezoidal (several
//! starts), buttress, round, Pg and custom profiles, cosmetic and modelled.

use std::f64::consts::PI;

use super::*;

fn shaft(s: &mut Session, d: f64, h: f64) {
    run(s, "solid.cylinder", json!({"radius": d / 2.0, "height": h, "name": "Shaft", "body_name": "Shaft"}));
}

fn error(s: &Session) -> Option<String> {
    s.model.results.iter().find_map(|r| r.error.clone())
}

#[test]
fn cosmetic_threads_of_every_standard_fit_their_shafts() {
    for (designation, d) in [
        ("1/4-20 UNC", 6.35),
        ("#10-32 UNF", 4.83),
        ("1/2 BSW", 12.7),
        ("G1/2", 20.955),
        ("R3/8", 16.66),
        ("1/2 NPT", 21.336),
        ("Tr16x4", 16.0),
        ("S20x4", 20.0),
        ("Rd16x1/8", 16.0),
        ("Pg13.5", 20.4),
        ("20x2.5 60°", 20.0),
        ("M10x1", 10.0),
    ] {
        let mut s = Session::default();
        shaft(&mut s, d, 20.0);
        let th = run(&mut s, "solid.thread", json!({"face": [d / 2.0, 0, 10], "designation": designation}));
        assert_eq!(error(&s), None, "{designation}");
        let shown = th["thread"]["designation"].as_str().unwrap_or_default().to_string();
        assert!(!shown.is_empty(), "{designation}: {th}");
    }
    let mut s = Session::default();
    shaft(&mut s, 10.0, 20.0);
    let r = s.execute("solid.thread", &json!({"face": [5, 0, 10], "designation": "Q7"}));
    assert!(r.is_err());
}

#[test]
fn modelled_threads_cut_their_profile() {
    for (designation, d, starts) in [
        ("Tr16x4", 16.0, 1),
        ("Tr16x8(P4)", 16.0, 2),
        ("1/2-13 UNC", 12.7, 1),
        ("G3/4", 26.441, 1),
        ("S20x4", 20.0, 1),
        ("Rd20x4", 20.0, 1),
        ("16x2 40° LH", 16.0, 1),
        ("20x4 30° h1.5 c1.2", 20.0, 1),
        ("20x4 SQ", 20.0, 1),
        ("20x4 P0:0;1:2;2:2;3:0", 20.0, 1),
        ("20x4 P0:0;0.5:2;1.5:2;3.5:0.2 LH", 20.0, 1),
    ] {
        let mut s = Session::default();
        shaft(&mut s, d, 24.0);
        let before = volume(&mut s);
        run(&mut s, "solid.thread", json!({"face": [d / 2.0, 0, 12], "designation": designation, "modeled": true}));
        assert_eq!(error(&s), None, "{designation}");
        let after = volume(&mut s);
        let spec = solidcraft_doc::thread_spec::parse(designation).unwrap();
        // The grooves take about half the ring between the major and minor diameters (more with
        // the run-out past the free end), per start.
        let ring = PI * (spec.major.powi(2) - spec.minor().powi(2)) / 4.0 * 24.0;
        let cut = before - after;
        assert!(cut > 0.25 * ring && cut < 0.9 * ring, "{designation}: cut {cut} of the ring {ring}");
        assert_eq!(spec.starts, starts);
    }
}

#[test]
fn a_tapered_pipe_thread_is_modelled() {
    for (designation, d) in [("1/2 NPT", 21.336), ("R1/2", 20.955)] {
        let mut s = Session::default();
        shaft(&mut s, d, 20.0);
        let before = volume(&mut s);
        run(&mut s, "solid.thread", json!({"face": [d / 2.0, 0, 10], "designation": designation, "modeled": true}));
        assert_eq!(error(&s), None, "{designation}");
        assert!(volume(&mut s) < before, "{designation}");
    }
    // The groove follows the taper: 1:16 on the diameter over the length swept.
    let plane = solidcraft_geom::Plane { origin: Vec3::ZERO, x: Vec3::X, y: Vec3::Z };
    let square = [
        solidcraft_geom::Vec2::new(10.0, -0.2),
        solidcraft_geom::Vec2::new(11.0, -0.2),
        solidcraft_geom::Vec2::new(11.0, 0.2),
        solidcraft_geom::Vec2::new(10.0, 0.2),
    ];
    let m = solidcraft_kernel::sweep_helix_mesh_tapered(&plane, &square, Vec3::ZERO, Vec3::Z, 2.0, 8.0, 48, 1.0 / 32.0).unwrap();
    let radius = |p: &Vec3| (p.x * p.x + p.y * p.y).sqrt();
    let top = m.positions.iter().filter(|p| p.z > 15.5).map(radius).fold(0.0, f64::max);
    let bottom = m.positions.iter().filter(|p| p.z < 0.5).map(radius).fold(0.0, f64::max);
    assert!((bottom - top - 16.0 / 32.0).abs() < 0.05, "{bottom} {top}");
}

#[test]
fn a_hole_tapped_for_a_trapezoidal_thread() {
    let mut s = Session::default();
    run(&mut s, "solid.box", json!({"length": 40, "width": 40, "height": 20, "name": "Nut"}));
    run(&mut s, "solid.hole", json!({"position": [20, 20, 20], "thread": "Tr16x4", "name": "Bore"}));
    assert_eq!(error(&s), None);
    let t = run(&mut s, "model.threads", json!({}));
    assert_eq!(t["threads"][0]["designation"], "Tr16x4", "{t}");
    // The tap drill of a trapezoidal thread: about its minor diameter (16 − 2 · 0.9 · 2).
    let v = volume(&mut s);
    let want = 40.0 * 40.0 * 20.0 - PI * (12.4f64 / 2.0).powi(2) * 20.0;
    assert!(rel(v, want) < 1e-3, "{v} vs {want}");
}
