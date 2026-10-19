//! Probe: shell a loft with its end caps opened.
use super::*;

fn sk(s: &mut Session, name: &str, z: f64, shape: &str, a: f64, b: f64) {
    run(s, "sketch.create", json!({"plane": {"origin": [0, 0, z], "x_dir": [1, 0, 0], "y_dir": [0, 1, 0]}, "name": name}));
    match shape {
        "rect" => {
            run(s, "sketch.rectangle.center", json!({"center": [0, 0], "corner": [a, b]}));
        }
        _ => {
            run(s, "sketch.circle.center", json!({"center": [0, 0], "radius": a}));
        }
    }
    run(s, "sketch.finish", json!({}));
}

fn probe(label: &str, secs: &[(f64, &str, f64, f64)], dir: &str) {
    let mut s = Session::default();
    let mut list = Vec::new();
    for (i, (z, shape, a, b)) in secs.iter().enumerate() {
        let n = format!("S{i}");
        sk(&mut s, &n, *z, shape, *a, *b);
        list.push(json!({"sketch": n}));
    }
    run(&mut s, "solid.loft", json!({"sections": list, "operation": "new", "name": "L", "body_name": "B"}));
    let v0 = volume(&mut s);
    let z0 = secs[0].0;
    let z1 = secs[secs.len() - 1].0;
    let t0 = std::time::Instant::now();
    let r = s.execute("solid.shell", &json!({"faces": [[0.01, 0.02, z0], [0.01, 0.02, z1]], "thickness": 2, "direction": dir, "name": "Sh"}));
    let err = s.model.results.iter().filter_map(|r| r.error.clone()).collect::<Vec<_>>().join(";");
    let v1 = if r.is_ok() { volume(&mut s) } else { -1.0 };
    eprintln!("PROBE {label} [{dir}]: solid={v0:.0} shell={v1:.0} ok={} t={:?} err={err} {:?}", r.is_ok(), t0.elapsed(), r.err());
}

#[test]
fn probe_thin_lofts() {
    for d in ["inside", "outside", "both"] {
        probe("rect->smaller rect", &[(0.0, "rect", 30.0, 20.0), (50.0, "rect", 15.0, 10.0)], d);
        probe("rect->circle", &[(0.0, "rect", 30.0, 20.0), (50.0, "circle", 15.0, 0.0)], d);
        probe("circle->circle", &[(0.0, "circle", 30.0, 0.0), (50.0, "circle", 15.0, 0.0)], d);
        probe("3 sections", &[(0.0, "circle", 30.0, 0.0), (30.0, "rect", 20.0, 20.0), (60.0, "circle", 12.0, 0.0)], d);
    }
}
