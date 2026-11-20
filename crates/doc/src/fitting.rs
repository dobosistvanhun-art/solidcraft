//! Pipe fittings made of sheet: their shapes and their flat patterns, worked out exactly.
//!
//! A fitting's body is a developable surface between two circles in parallel planes (a pipe,
//! a concentric or an eccentric cone, `Mantle`), with or without a round branch pipe set into
//! its side at a height and an angle (`Branch`); or a mitred elbow of segments (`Elbow`). Each
//! piece's flat pattern is its middle surface unrolled (the sheet's mean diameter): the mantle
//! along its generators, a branch along its own with the saddle where it meets the mantle, and
//! the mantle's opening where the branch comes in. The seam (where a pattern opens) lies on the
//! side away from the cut: a mantle's opposite its branch, a branch's opposite the deepest point
//! of its saddle, an elbow segment's on the inside of the bend.
//!
//! All lengths in millimetres, angles in radians. Pure geometry: no unwrap, inputs checked.

use solidcraft_geom::{Vec2, Vec3};

/// Points round a circle for a pattern's curves.
pub const STEPS: usize = 360;

/// Which diameter a fitting's dimensions give: the sheet's outside, inside or middle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Measured {
    #[default]
    Outside,
    Inside,
    Middle,
}

impl Measured {
    /// From its name (outside, inside, middle; also od / id / mean, and the Hungarian words).
    pub fn parse(s: &str) -> Option<Measured> {
        match s.trim().to_lowercase().as_str() {
            "" | "outside" | "outer" | "od" | "kulso" | "külső" => Some(Measured::Outside),
            "inside" | "inner" | "id" | "belso" | "belső" => Some(Measured::Inside),
            "middle" | "mean" | "neutral" | "kozep" | "közép" => Some(Measured::Middle),
            _ => None,
        }
    }

    /// The middle (mean) diameter of a sheet of thickness `t` whose `d` is measured this way.
    pub fn mean(self, d: f64, t: f64) -> f64 {
        match self {
            Measured::Outside => d - t,
            Measured::Inside => d + t,
            Measured::Middle => d,
        }
    }
}

/// The body of a fitting: the surface between a bottom circle (radius `r0`, centred on the
/// origin in the XY plane) and a top circle (radius `r1`, at height `h`, its centre moved by
/// `shift` along X — an eccentric cone), middle-surface radii.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mantle {
    pub r0: f64,
    pub r1: f64,
    pub h: f64,
    pub shift: f64,
}

/// A round branch set into a mantle's side: its axis in the XZ plane, meeting the mantle's
/// axis line (shifted by `offset` along Y for an off-centre branch) at height `at`, leaning
/// `angle` from the mantle's axis (π/2: square to it), running out to the +X side; `radius`
/// its middle-surface radius, `length` from the mantle's axis to its end along its own axis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Branch {
    pub radius: f64,
    pub length: f64,
    pub at: f64,
    pub angle: f64,
    pub offset: f64,
}

/// A flat pattern: its outline (closed), the holes cut in it (closed), and lines marked on it
/// (the bend lines of an elbow, a few generators to roll by), in the pattern's own plane.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Pattern {
    pub outline: Vec<Vec2>,
    pub holes: Vec<Vec<Vec2>>,
    pub marks: Vec<(Vec2, Vec2)>,
}

impl Pattern {
    /// The sheet the pattern takes, in mm²: inside its outline, less its holes.
    pub fn area(&self) -> f64 {
        area(&self.outline).abs() - self.holes.iter().map(|h| area(h).abs()).sum::<f64>()
    }
}

fn bad(why: &str) -> Result<(), String> {
    Err(why.to_string())
}

impl Mantle {
    pub fn check(&self) -> Result<(), String> {
        let ok = |v: f64| v.is_finite() && v > 0.0;
        if !(ok(self.r0) && ok(self.r1) && ok(self.h)) || !self.shift.is_finite() {
            return bad("the body's diameters and height must be positive");
        }
        if self.r0 > 1e6 || self.r1 > 1e6 || self.h > 1e6 || self.shift.abs() > 1e6 {
            return bad("the body is too large");
        }
        Ok(())
    }

    /// The circle's centre and radius at fraction `s` (0 bottom … 1 top) of the height.
    fn ring(&self, s: f64) -> (Vec3, f64) {
        (Vec3::new(self.shift * s, 0.0, self.h * s), self.r0 + (self.r1 - self.r0) * s)
    }

    /// The point at angle `phi` round the axis (from +X) and fraction `s` up the generator.
    pub fn point(&self, phi: f64, s: f64) -> Vec3 {
        let (c, r) = self.ring(s);
        c + Vec3::new(r * phi.cos(), r * phi.sin(), 0.0)
    }

    /// Where `p` is on the mantle's coordinates (angle round, fraction up), and how far it is
    /// outside the surface (negative inside), measured in the plane of its height.
    fn locate(&self, p: Vec3) -> (f64, f64, f64) {
        let s = p.z / self.h;
        let (c, r) = self.ring(s);
        let (dx, dy) = (p.x - c.x, p.y - c.y);
        (dy.atan2(dx), s, dx.hypot(dy) - r)
    }

    /// The mantle unrolled along its generators, from the seam at angle `seam`: the flat
    /// position of each generator's bottom and top end (`STEPS + 1` of them, round once).
    /// Exact for a pipe and a cone (a developable surface): each strip between two generators
    /// is laid flat as the two triangles it is.
    pub fn unroll(&self, seam: f64) -> Vec<(Vec2, Vec2)> {
        let at = |i: usize| seam + std::f64::consts::TAU * i as f64 / STEPS as f64;
        let mut out = Vec::with_capacity(STEPS + 1);
        let (b0, t0) = (self.point(at(0), 0.0), self.point(at(0), 1.0));
        let (mut fb, mut ft) = (Vec2::new(0.0, 0.0), Vec2::new(0.0, b0.dist(t0)));
        out.push((fb, ft));
        // along the circles the arc's length, not the chord's: the pattern rolls up to the
        // circles' true circumference
        let dphi = std::f64::consts::TAU / STEPS as f64;
        let (arc0, arc1) = (self.r0 * dphi, self.r1 * dphi);
        let chord = |r: f64| 2.0 * r * (dphi / 2.0).sin();
        // the strip's diagonal: its run round the axis an arc too (at the mean radius)
        let mid = 0.5 * (self.r0 + self.r1);
        let stretch = (mid * dphi).powi(2) - chord(mid).powi(2);
        for i in 1..=STEPS {
            let pt = self.point(at(i - 1), 1.0);
            let (nb, nt) = (self.point(at(i), 0.0), self.point(at(i), 1.0));
            // the next bottom point: its distances to this strip's bottom and top points (the
            // strip's diagonal stretched as its circle's arcs are)
            let nb_flat = third(fb, ft, arc0, (pt.dist(nb).powi(2) + stretch).max(0.0).sqrt());
            let nt_flat = third(nb_flat, ft, nb.dist(nt), arc1);
            fb = nb_flat;
            ft = nt_flat;
            out.push((fb, ft));
        }
        out
    }

    /// The mantle's flat pattern with the opening a branch cuts in it (the seam opposite the
    /// branch). `branch`: the curve where it meets the mantle, as (angle round, fraction up).
    pub fn pattern(&self, branch: Option<&[(f64, f64)]>) -> Pattern {
        let seam = std::f64::consts::PI;
        let gens = self.unroll(seam);
        let mut outline: Vec<Vec2> = gens.iter().map(|g| g.0).collect();
        outline.extend(gens.iter().rev().map(|g| g.1));
        let flat = |phi: f64, s: f64| {
            // the generator's place round the pattern, between the two unrolled ones about it
            let u = (phi - seam).rem_euclid(std::f64::consts::TAU) / std::f64::consts::TAU * STEPS as f64;
            let i = (u.floor() as usize).min(STEPS - 1);
            let f = u - i as f64;
            let (Some(a), Some(b)) = (gens.get(i), gens.get(i + 1)) else { return Vec2::new(0.0, 0.0) };
            let bottom = a.0 + (b.0 - a.0) * f;
            let top = a.1 + (b.1 - a.1) * f;
            bottom + (top - bottom) * s
        };
        let holes = branch.map(|c| vec![c.iter().map(|&(phi, s)| flat(phi, s)).collect()]).unwrap_or_default();
        // a few generators marked, to roll it by
        let marks = (1..12).filter_map(|k| gens.get(k * STEPS / 12)).map(|g| (g.0, g.1)).collect();
        Pattern { outline, holes, marks }
    }
}

/// The point at distance `da` from `a` and `db` from `b`, on the left of a→b (the unrolled
/// strip turns one way).
fn third(a: Vec2, b: Vec2, da: f64, db: f64) -> Vec2 {
    let ab = b - a;
    let d = ab.len().max(1e-12);
    let x = (da * da - db * db + d * d) / (2.0 * d);
    let y = (da * da - x * x).max(0.0).sqrt();
    let u = ab * (1.0 / d);
    let n = Vec2::new(u.y, -u.x);
    a + u * x + n * y
}

impl Branch {
    pub fn check(&self, m: &Mantle) -> Result<(), String> {
        let ok = |v: f64| v.is_finite() && v > 0.0;
        if !(ok(self.radius) && ok(self.length)) || !self.at.is_finite() || !self.offset.is_finite() {
            return bad("the branch's diameter and length must be positive");
        }
        if !(self.angle.is_finite() && self.angle > 10f64.to_radians() && self.angle < 170f64.to_radians()) {
            return bad("the branch's angle must be between 10° and 170°");
        }
        let inside = |s: f64| {
            let (_, r) = m.ring(s);
            r
        };
        if self.at - self.radius / self.angle.sin() < 0.0 || self.at + self.radius / self.angle.sin() > m.h {
            return bad("the branch must meet the body within its height");
        }
        let narrowest = inside(0.0).min(inside(1.0));
        if self.radius + self.offset.abs() >= narrowest {
            return bad("the branch must be narrower than the body where it meets it");
        }
        Ok(())
    }

    /// The axis direction and the two directions across it.
    fn frame(&self) -> (Vec3, Vec3, Vec3) {
        let (s, c) = self.angle.sin_cos();
        (Vec3::new(s, 0.0, c), Vec3::new(c, 0.0, -s), Vec3::new(0.0, 1.0, 0.0))
    }

    /// The point of the branch at angle `theta` round it (0: on its upper side, toward the
    /// mantle's top for a square branch) and `t` along its axis from the mantle's axis.
    pub fn point(&self, theta: f64, t: f64) -> Vec3 {
        let (b, n1, n2) = self.frame();
        Vec3::new(0.0, self.offset, self.at) + b * t + (n1 * (-theta.cos()) + n2 * theta.sin()) * self.radius
    }

    /// Where the generator at `theta` meets the mantle: how far along the axis.
    pub fn meets(&self, m: &Mantle, theta: f64) -> f64 {
        let (mut lo, mut hi) = (-self.length, self.length * 4.0 + m.r0.max(m.r1) * 4.0);
        // outside the mantle at `hi`, inside at the axis
        for _ in 0..80 {
            let mid = 0.5 * (lo + hi);
            if m.locate(self.point(theta, mid)).2 < 0.0 {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        0.5 * (lo + hi)
    }

    /// The curve where the branch meets the mantle, on the mantle's coordinates.
    pub fn on_mantle(&self, m: &Mantle) -> Vec<(f64, f64)> {
        (0..=STEPS)
            .map(|i| {
                let theta = std::f64::consts::TAU * i as f64 / STEPS as f64;
                let p = self.point(theta, self.meets(m, theta));
                let (phi, s, _) = m.locate(p);
                (phi, s)
            })
            .collect()
    }

    /// The branch's flat pattern: its generators' lengths from the mantle out to its end,
    /// round its middle circumference, opened opposite the deepest point of its saddle.
    pub fn pattern(&self, m: &Mantle) -> Pattern {
        let len = |theta: f64| self.length - self.meets(m, theta);
        // the saddle's deepest point: the longest generator; the seam opposite it
        let deepest = (0..STEPS).map(|i| std::f64::consts::TAU * i as f64 / STEPS as f64).max_by(|a, b| len(*a).total_cmp(&len(*b))).unwrap_or(0.0);
        let seam = deepest + std::f64::consts::PI;
        let circ = std::f64::consts::TAU * self.radius;
        let saddle: Vec<Vec2> = (0..=STEPS)
            .map(|i| {
                let u = i as f64 / STEPS as f64;
                Vec2::new(u * circ, self.length - len(seam + std::f64::consts::TAU * u))
            })
            .collect();
        let mut outline = saddle.clone();
        outline.push(Vec2::new(circ, self.length));
        outline.push(Vec2::new(0.0, self.length));
        let marks = (1..12).filter_map(|k| saddle.get(k * STEPS / 12).map(|p| (*p, Vec2::new(p.x, self.length)))).collect();
        Pattern { outline, holes: Vec::new(), marks }
    }
}

/// A mitred elbow: `segments` pieces (two half ones at the ends) turning `angle` about a centre
/// line of radius `bend`, of middle-surface radius `radius`, with `straight` added to each end.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Elbow {
    pub radius: f64,
    pub bend: f64,
    pub angle: f64,
    pub segments: usize,
    pub straight: f64,
}

impl Elbow {
    pub fn check(&self) -> Result<(), String> {
        let ok = |v: f64| v.is_finite() && v > 0.0;
        if !(ok(self.radius) && ok(self.bend)) || !(self.straight.is_finite() && self.straight >= 0.0) {
            return bad("the elbow's diameter and bend radius must be positive");
        }
        if !(2..=48).contains(&self.segments) {
            return bad("an elbow has 2 to 48 segments");
        }
        if !(self.angle.is_finite() && self.angle > 1f64.to_radians() && self.angle <= std::f64::consts::PI) {
            return bad("the elbow's angle must be between 1° and 180°");
        }
        if self.bend <= self.radius {
            return bad("the bend radius must be larger than the pipe's radius");
        }
        Ok(())
    }

    /// The angle one joint turns (two half segments at the ends: `segments − 1` joints).
    pub fn step(&self) -> f64 {
        self.angle / (self.segments - 1) as f64
    }

    /// Segment `k`'s generator lengths about its axis: from its start plane to its end plane at
    /// angle `theta` round the pipe (0: the outside of the bend). The end segments are half.
    pub fn length(&self, k: usize, theta: f64) -> f64 {
        let half = (self.step() / 2.0).tan() * (self.bend + self.radius * theta.cos());
        let end = k == 0 || k + 1 == self.segments;
        if end { half + self.straight } else { 2.0 * half }
    }

    /// Segment `k`'s flat pattern, opened on the inside of the bend (its shortest generator),
    /// its middle line marked (where it crosses the pipe's axis plane).
    pub fn pattern(&self, k: usize) -> Pattern {
        let circ = std::f64::consts::TAU * self.radius;
        let end = k == 0 || k + 1 == self.segments;
        let half = |theta: f64| (self.step() / 2.0).tan() * (self.bend + self.radius * theta.cos());
        // from the seam on the inside (θ = π) round once
        let at = |u: f64| std::f64::consts::PI + std::f64::consts::TAU * u;
        let n = STEPS;
        let lower: Vec<Vec2> = (0..=n)
            .map(|i| {
                let u = i as f64 / n as f64;
                Vec2::new(u * circ, if end { 0.0 } else { -half(at(u)) })
            })
            .collect();
        let upper: Vec<Vec2> = (0..=n)
            .map(|i| {
                let u = i as f64 / n as f64;
                Vec2::new(u * circ, half(at(u)) + if end { self.straight } else { 0.0 })
            })
            .collect();
        let mut outline = lower;
        outline.extend(upper.iter().rev());
        let marks = vec![(Vec2::new(0.0, 0.0), Vec2::new(circ, 0.0))];
        Pattern { outline, holes: Vec::new(), marks }
    }
}

/// The area a closed ring encloses (positive counter-clockwise).
pub fn area(ring: &[Vec2]) -> f64 {
    let n = ring.len();
    (0..n)
        .map(|i| {
            let (a, b) = (ring[i], ring[(i + 1) % n]);
            a.x * b.y - b.x * a.y
        })
        .sum::<f64>()
        / 2.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::{PI, TAU};

    fn pipe(r: f64, h: f64) -> Mantle {
        Mantle { r0: r, r1: r, h, shift: 0.0 }
    }

    /// A pipe unrolls into a rectangle of its circumference by its height.
    #[test]
    fn a_pipe_unrolls_into_its_circumference_by_its_height() {
        let m = pipe(50.0, 200.0);
        let p = m.pattern(None);
        assert!((area(&p.outline).abs() - TAU * 50.0 * 200.0).abs() < 1.0, "{}", area(&p.outline));
        let xs: Vec<f64> = p.outline.iter().map(|q| q.x).collect();
        let w = xs.iter().cloned().fold(f64::MIN, f64::max) - xs.iter().cloned().fold(f64::MAX, f64::min);
        assert!((w - TAU * 50.0).abs() < 1e-6 * w, "{w}");
    }

    /// A cone unrolls into its sector: its area is its mantle's, its edges the slant length.
    #[test]
    fn a_cone_unrolls_into_its_sector() {
        let m = Mantle { r0: 100.0, r1: 40.0, h: 120.0, shift: 0.0 };
        let slant = (60f64 * 60.0 + 120.0 * 120.0).sqrt();
        let mantle = PI * (100.0 + 40.0) * slant;
        let p = m.pattern(None);
        assert!((area(&p.outline).abs() - mantle).abs() < 1e-3 * mantle, "{} vs {mantle}", area(&p.outline));
        let gens = m.unroll(PI);
        for g in [gens[0], gens[STEPS / 3], gens[STEPS]] {
            assert!((g.0.dist(g.1) - slant).abs() < 1e-6, "{}", g.0.dist(g.1));
        }
        // the bottom edge's length is the bottom circumference
        let bottom: f64 = gens.windows(2).map(|w| w[0].0.dist(w[1].0)).sum();
        assert!((bottom - TAU * 100.0).abs() < 0.01, "{bottom}");
    }

    /// An eccentric cone (its top moved over so one side stands upright) unrolls with each
    /// generator as long as it is in space, and its edges as long as its circles round.
    #[test]
    fn an_eccentric_cone_unrolls_true_to_length() {
        let m = Mantle { r0: 100.0, r1: 50.0, h: 150.0, shift: 50.0 };
        let gens = m.unroll(PI);
        for (i, g) in gens.iter().enumerate().step_by(37) {
            let phi = PI + TAU * i as f64 / STEPS as f64;
            let space = m.point(phi, 0.0).dist(m.point(phi, 1.0));
            assert!((g.0.dist(g.1) - space).abs() < 1e-6, "{i}: {} vs {space}", g.0.dist(g.1));
        }
        let top: f64 = gens.windows(2).map(|w| w[0].1.dist(w[1].1)).sum();
        assert!((top - TAU * 50.0).abs() < 0.05, "{top}");
        // the upright side (φ = 0) is the height
        assert!((m.point(0.0, 0.0).dist(m.point(0.0, 1.0)) - 150.0).abs() < 1e-9);
    }

    /// An equal square T: the branch's saddle drops by the pipe's radius at its sides, its
    /// generators are the classic √(R² − r² sin² θ) short of the branch's length; the opening in
    /// the pipe is the branch's circumference across and its diameter high.
    #[test]
    fn a_square_t_cuts_the_classic_saddle_and_opening() {
        let m = pipe(50.0, 300.0);
        let b = Branch { radius: 30.0, length: 150.0, at: 150.0, angle: PI / 2.0, offset: 0.0 };
        assert!(b.check(&m).is_ok());
        for theta in [0.0, 0.4, PI / 2.0, 2.0, PI] {
            let t = b.meets(&m, theta);
            let want = (50f64.powi(2) - (30.0 * theta.sin()).powi(2)).sqrt();
            assert!((t - want).abs() < 1e-6, "{theta}: {t} vs {want}");
        }
        let p = b.pattern(&m);
        assert!((p.outline.iter().map(|q| q.x).fold(0.0, f64::max) - TAU * 30.0).abs() < 1e-6);
        let hole = m.pattern(Some(&b.on_mantle(&m))).holes.remove(0);
        let ys: Vec<f64> = hole.iter().map(|q| q.y).collect();
        let high = ys.iter().cloned().fold(f64::MIN, f64::max) - ys.iter().cloned().fold(f64::MAX, f64::min);
        assert!((high - 60.0).abs() < 1e-6, "{high}");
        let xs: Vec<f64> = hole.iter().map(|q| q.x).collect();
        let wide = xs.iter().cloned().fold(f64::MIN, f64::max) - xs.iter().cloned().fold(f64::MAX, f64::min);
        // across: the arc 2·R·asin(r/R) round the pipe
        assert!((wide - 2.0 * 50.0 * (30f64 / 50.0).asin()).abs() < 0.01, "{wide}");
        // the opening sits in the middle of the pattern: the seam is opposite the branch
        let mid = xs.iter().sum::<f64>() / xs.len() as f64;
        assert!((mid - PI * 50.0).abs() < 0.5, "{mid}");
    }

    /// A leaning branch (45°) is longer on its lower side; its seam is opposite that, and every
    /// point of its saddle lies on the pipe.
    #[test]
    fn a_leaning_branch_meets_the_pipe_all_round() {
        let m = pipe(60.0, 400.0);
        let b = Branch { radius: 25.0, length: 200.0, at: 200.0, angle: PI / 4.0, offset: 0.0 };
        assert!(b.check(&m).is_ok());
        for i in 0..24 {
            let theta = TAU * i as f64 / 24.0;
            let p = b.point(theta, b.meets(&m, theta));
            assert!((p.x.hypot(p.y) - 60.0).abs() < 1e-6, "{i}");
        }
        let lower = b.length - b.meets(&m, PI);
        let upper = b.length - b.meets(&m, 0.0);
        assert!(lower > upper, "{lower} {upper}");
    }

    /// A branch into a cone's side meets the cone all round, and its opening lies inside the
    /// cone's pattern.
    #[test]
    fn a_branch_into_a_cone_meets_it_all_round() {
        let m = Mantle { r0: 120.0, r1: 60.0, h: 300.0, shift: 0.0 };
        let b = Branch { radius: 30.0, length: 250.0, at: 150.0, angle: PI / 2.0, offset: 0.0 };
        assert!(b.check(&m).is_ok());
        for (phi, s) in b.on_mantle(&m).iter().step_by(30) {
            let q = m.point(*phi, *s);
            let (_, _, off) = m.locate(q);
            assert!(off.abs() < 1e-6 && (0.0..=1.0).contains(s));
        }
        let p = m.pattern(Some(&b.on_mantle(&m)));
        assert!(area(&p.holes[0]).abs() > 0.8 * PI * 30.0 * 30.0, "{}", area(&p.holes[0]));
        assert!(area(&p.holes[0]).abs() < area(&p.outline).abs());
    }

    /// A 90° elbow of 4 segments: three joints of 30°; a middle segment is twice an end one
    /// (without its straight); the outside of the bend is the longest.
    #[test]
    fn a_mitred_elbows_segments() {
        let e = Elbow { radius: 50.0, bend: 150.0, angle: PI / 2.0, segments: 4, straight: 0.0 };
        assert!(e.check().is_ok());
        assert!((e.step() - PI / 6.0).abs() < 1e-12);
        let out = e.length(1, 0.0);
        assert!((out - 2.0 * (PI / 12.0).tan() * 200.0).abs() < 1e-9, "{out}");
        assert!((e.length(0, 0.0) * 2.0 - out).abs() < 1e-9);
        assert!(e.length(1, PI) < out);
        // the middle line along the bend's centre line: the segment's centre-line length
        let p = e.pattern(1);
        let circ = TAU * 50.0;
        assert!((p.outline.iter().map(|q| q.x).fold(0.0, f64::max) - circ).abs() < 1e-6);
        assert!(Elbow { segments: 1, ..e }.check().is_err() && Elbow { bend: 40.0, ..e }.check().is_err());
    }

    #[test]
    fn hostile_sizes_are_refused() {
        assert!(Mantle { r0: f64::NAN, r1: 1.0, h: 1.0, shift: 0.0 }.check().is_err());
        assert!(Mantle { r0: -1.0, r1: 1.0, h: 1.0, shift: 0.0 }.check().is_err());
        let m = pipe(50.0, 100.0);
        let b = Branch { radius: 30.0, length: 100.0, at: 50.0, angle: PI / 2.0, offset: 0.0 };
        assert!(Branch { radius: 60.0, ..b }.check(&m).is_err(), "wider than the body");
        assert!(Branch { at: 10.0, ..b }.check(&m).is_err(), "off the body's end");
        assert!(Branch { angle: 0.01, ..b }.check(&m).is_err());
        assert!(Branch { length: f64::INFINITY, ..b }.check(&m).is_err());
    }
}
