//! Gears, sprockets, worms and worm wheels made by `solid.gear`.

use std::f64::consts::PI;

use super::*;

fn error(s: &Session) -> Option<String> {
    s.model.results.iter().find_map(|r| r.error.clone())
}

#[test]
fn a_spur_gear_is_about_its_pitch_cylinder() {
    let mut s = Session::default();
    let r = run(&mut s, "solid.gear", json!({"teeth": 20, "module": 2, "width": 10, "bore": 8}));
    assert_eq!(error(&s), None);
    assert!((r["geometry"]["pitch_diameter"].as_f64().unwrap() - 40.0).abs() < 1e-9, "{r}");
    let v = volume(&mut s);
    // The teeth add about as much outside the pitch circle as the gaps take inside it.
    let want = PI * (20.0f64.powi(2) - 4.0f64.powi(2)) * 10.0;
    assert!(rel(v, want) < 0.05, "{v} vs {want}");
}

#[test]
fn a_helical_gear_keeps_the_spur_gears_volume() {
    let mut spur = Session::default();
    run(&mut spur, "solid.gear", json!({"teeth": 24, "module": "1.5 mm", "width": 12}));
    let mut helical = Session::default();
    run(&mut helical, "solid.gear", json!({"teeth": 24, "module": "1.5 mm", "width": 12, "helix_angle": "20 deg", "bore": 6}));
    assert_eq!(error(&helical), None);
    // Twisting a section does not change its volume; the helical one's transverse module is
    // larger (m / cos β), the bore takes 6 mm.
    let (vs, vh) = (volume(&mut spur), volume(&mut helical));
    let scale = (1.0 / 20f64.to_radians().cos()).powi(2);
    let want = vs * scale - PI * 9.0 * 12.0;
    assert!(rel(vh, want) < 0.03, "{vh} vs {want}");
}

#[test]
fn sprockets_for_the_usual_chains() {
    for (chain, teeth) in [("08B", 17), ("520", 14), ("06B", 9), ("428", 45)] {
        let mut s = Session::default();
        let r = run(&mut s, "solid.gear.sprocket", json!({"chain": chain, "teeth": teeth, "bore": 10}));
        assert_eq!(error(&s), None, "{chain} z{teeth}");
        let g = &r["geometry"];
        let (d, da, df) = (g["pitch_diameter"].as_f64().unwrap(), g["tip_diameter"].as_f64().unwrap(), g["root_diameter"].as_f64().unwrap());
        assert!(df < d && d < da, "{chain}: {g}");
        let w = g["tooth_width"].as_f64().unwrap();
        let v = volume(&mut s);
        // Between the root and tip discs (less the bore).
        let disc = |dd: f64| PI * (dd * dd - 100.0) / 4.0 * w;
        assert!(v > disc(df) && v < disc(da), "{chain} z{teeth}: {v}");
    }
    let mut s = Session::default();
    assert!(s.execute("solid.gear.sprocket", &json!({"chain": "99Z", "teeth": 17})).is_err());
}

#[test]
fn a_worm_and_its_wheel_mesh_at_the_centre_distance() {
    let mut s = Session::default();
    run(&mut s, "solid.gear.worm", json!({"starts": 1, "module": 2, "quotient": 10, "width": 30, "axis": "X", "name": "Worm", "body_name": "Worm"}));
    assert_eq!(error(&s), None);
    let r = run(
        &mut s,
        "solid.gear.worm_wheel",
        json!({"teeth": 30, "module": 2, "starts": 1, "quotient": 10, "width": 14, "base": [0, 40, -7], "name": "Wheel", "body_name": "Wheel"}),
    );
    assert_eq!(error(&s), None);
    assert!((r["geometry"]["center_distance"].as_f64().unwrap() - 40.0).abs() < 1e-9, "{r}");
    let st = s.model.state();
    let vol = |n: &str| solidcraft_kernel::measure(&st.body(n).unwrap().body).unwrap().volume;
    // The worm: about its pitch cylinder (20 mm) over 30 mm.
    let worm = PI * 10.0 * 10.0 * 30.0;
    assert!(rel(vol("Worm"), worm) < 0.1, "{} vs {worm}", vol("Worm"));
    let wheel = PI * 30.0 * 30.0 * 14.0;
    assert!(rel(vol("Wheel"), wheel) < 0.05, "{} vs {wheel}", vol("Wheel"));
}

#[test]
fn a_gear_saves_and_reopens() {
    let mut s = Session::default();
    run(&mut s, "solid.gear.sprocket", json!({"chain": "08B", "teeth": 13, "name": "Kerek"}));
    let v = volume(&mut s);
    let path = std::env::temp_dir().join(format!("gear-{}.json", std::process::id()));
    let p = path.to_string_lossy().to_string();
    run(&mut s, "file.save_as", json!({"path": p, "format": "json"}));
    let mut r = Session::default();
    run(&mut r, "doc.open", json!({"path": p}));
    let _ = std::fs::remove_file(&path);
    assert!(rel(volume(&mut r), v) < 1e-9);
}

#[test]
#[ignore = "timing probe"]
fn gear_timings() {
    for (cmd, p) in [
        ("solid.gear.worm", json!({"starts": 1, "module": 2, "quotient": 10, "width": 30})),
        ("solid.gear.worm_wheel", json!({"teeth": 30, "module": 2, "width": 14})),
        ("solid.gear", json!({"teeth": 40, "module": 2, "width": 10, "helix_angle": "20 deg"})),
        ("solid.gear", json!({"teeth": 40, "module": 2, "width": 10})),
        ("solid.gear.sprocket", json!({"chain": "520", "teeth": 45})),
    ] {
        let mut s = Session::default();
        let t = std::time::Instant::now();
        let r = run(&mut s, cmd, p.clone());
        eprintln!("TIMING {cmd} {p}: {:?} ms={}", t.elapsed(), r["ms"]);
    }
}
