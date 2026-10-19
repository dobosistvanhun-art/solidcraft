//! Gears, chain sprockets and worms: their sections across the axis, from the standard tooth
//! forms. The 2D outlines are exact where the form is (circular arcs for sprocket seats and
//! flanks, root and tip circles) and piecewise cubic where it is not (involute flanks, worm
//! thread flanks), within a few micrometres at usual sizes.
//!
//! - Involute spur and helical gears: module, number of teeth, pressure angle, profile shift
//!   (DIN 867 / ISO 53 basic rack: addendum m, dedendum 1.25 m).
//! - Roller-chain sprockets: the tooth gap built from the chain's pitch and roller diameter by
//!   the ISO 606 tooth-form rules (roller seating radius and angle, tooth flank radius, tip
//!   diameter), each taken halfway between the standard's minimum and maximum.
//! - Cylindrical worms (ZA, straight-sided axial profile) and their wheels.

use std::f64::consts::{PI, TAU};

use solidcraft_geom::{Loop2, Region2, Seg2, Vec2};

use crate::{DocError, Result};

/// A roller chain: name, pitch p, roller (or bush) diameter d1 and inner width b1, in mm.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Chain {
    pub name: &'static str,
    pub pitch: f64,
    pub roller: f64,
    pub inner_width: f64,
}

/// Roller chains by their ISO 606 number, ANSI number and the usual motorcycle and bicycle
/// sizes (nominal dimensions).
pub const CHAINS: &[Chain] = &[
    Chain { name: "04B", pitch: 6.0, roller: 4.0, inner_width: 2.8 },
    Chain { name: "05B", pitch: 8.0, roller: 5.0, inner_width: 3.0 },
    Chain { name: "06B", pitch: 9.525, roller: 6.35, inner_width: 5.72 },
    Chain { name: "081", pitch: 12.7, roller: 7.75, inner_width: 3.3 },
    Chain { name: "083", pitch: 12.7, roller: 7.75, inner_width: 4.88 },
    Chain { name: "08A", pitch: 12.7, roller: 7.92, inner_width: 7.85 },
    Chain { name: "08B", pitch: 12.7, roller: 8.51, inner_width: 7.75 },
    Chain { name: "10A", pitch: 15.875, roller: 10.16, inner_width: 9.4 },
    Chain { name: "10B", pitch: 15.875, roller: 10.16, inner_width: 9.65 },
    Chain { name: "12A", pitch: 19.05, roller: 11.91, inner_width: 12.57 },
    Chain { name: "12B", pitch: 19.05, roller: 12.07, inner_width: 11.68 },
    Chain { name: "16A", pitch: 25.4, roller: 15.88, inner_width: 15.75 },
    Chain { name: "16B", pitch: 25.4, roller: 15.88, inner_width: 17.02 },
    Chain { name: "20A", pitch: 31.75, roller: 19.05, inner_width: 18.9 },
    Chain { name: "20B", pitch: 31.75, roller: 19.05, inner_width: 19.56 },
    Chain { name: "24A", pitch: 38.1, roller: 22.23, inner_width: 25.22 },
    Chain { name: "24B", pitch: 38.1, roller: 25.4, inner_width: 25.4 },
    // ANSI numbers (25 and 35 are rollerless: the bush diameter).
    Chain { name: "25", pitch: 6.35, roller: 3.3, inner_width: 3.18 },
    Chain { name: "35", pitch: 9.525, roller: 5.08, inner_width: 4.77 },
    Chain { name: "40", pitch: 12.7, roller: 7.92, inner_width: 7.85 },
    Chain { name: "41", pitch: 12.7, roller: 7.77, inner_width: 6.38 },
    Chain { name: "50", pitch: 15.875, roller: 10.16, inner_width: 9.4 },
    Chain { name: "60", pitch: 19.05, roller: 11.91, inner_width: 12.57 },
    Chain { name: "80", pitch: 25.4, roller: 15.88, inner_width: 15.75 },
    // Motorcycle chains.
    Chain { name: "415", pitch: 12.7, roller: 7.77, inner_width: 4.76 },
    Chain { name: "420", pitch: 12.7, roller: 7.77, inner_width: 6.35 },
    Chain { name: "428", pitch: 12.7, roller: 8.51, inner_width: 7.94 },
    Chain { name: "520", pitch: 15.875, roller: 10.16, inner_width: 6.35 },
    Chain { name: "525", pitch: 15.875, roller: 10.16, inner_width: 7.94 },
    Chain { name: "530", pitch: 15.875, roller: 10.16, inner_width: 9.53 },
    Chain { name: "630", pitch: 19.05, roller: 11.91, inner_width: 9.53 },
];

/// The chain with this name (`08B`, `ISO 08B`, `ANSI 40`, `520`; case and spaces ignored).
pub fn chain(name: &str) -> Option<Chain> {
    let n: String = name.chars().filter(|c| !c.is_whitespace() && *c != '-').collect::<String>().to_ascii_uppercase();
    let n = n.strip_prefix("ISO").or_else(|| n.strip_prefix("ANSI")).or_else(|| n.strip_prefix("DIN")).unwrap_or(&n).to_string();
    CHAINS.iter().copied().find(|c| c.name == n)
}

/// The chain names, for messages.
pub fn chain_names() -> String {
    CHAINS.iter().map(|c| c.name).collect::<Vec<_>>().join(", ")
}

fn bad(msg: impl Into<String>) -> DocError {
    DocError::Invalid(msg.into())
}

fn rot(v: Vec2, a: f64) -> Vec2 {
    let (s, c) = a.sin_cos();
    Vec2::new(v.x * c - v.y * s, v.x * s + v.y * c)
}

/// The arc about `center` from `a` to `b` (both on it), the shorter way round.
fn arc(center: Vec2, a: Vec2, b: Vec2) -> Seg2 {
    let start = (a - center).angle();
    let mut sweep = (b - center).angle() - start;
    while sweep > PI {
        sweep -= TAU;
    }
    while sweep < -PI {
        sweep += TAU;
    }
    Seg2::Arc { center, radius: (a - center).len(), start, sweep }
}

/// The arc about the origin of radius `r` from angle `a0` to `a1` (counter-clockwise when a1 > a0).
fn circle_arc(r: f64, a0: f64, a1: f64) -> Seg2 {
    Seg2::Arc { center: Vec2::new(0.0, 0.0), radius: r, start: a0, sweep: a1 - a0 }
}

/// Cubic pieces through the curve `f` on [t0, t1] (Hermite: points and derivatives `df` at the
/// piece ends), `n` pieces.
fn hermite(f: impl Fn(f64) -> Vec2, df: impl Fn(f64) -> Vec2, t0: f64, t1: f64, n: usize) -> Vec<Seg2> {
    (0..n)
        .map(|i| {
            let (a, b) = (t0 + (t1 - t0) * i as f64 / n as f64, t0 + (t1 - t0) * (i + 1) as f64 / n as f64);
            let h = b - a;
            let (p0, p3) = (f(a), f(b));
            Seg2::Cubic { p0, p1: p0 + df(a) * (h / 3.0), p2: p3 - df(b) * (h / 3.0), p3 }
        })
        .collect()
}

fn reversed(segs: Vec<Seg2>) -> Vec<Seg2> {
    segs.into_iter().rev().map(|s| s.reversed()).collect()
}

/// A closed loop from segments end to end; checks it closes and runs counter-clockwise.
fn closed(segs: Vec<Seg2>, what: &str) -> Result<Loop2> {
    if segs.iter().any(|s| !s.start().is_finite() || !s.end().is_finite()) {
        return Err(bad(format!("{what}: the outline is not finite")));
    }
    let lp = Loop2 { segs };
    let gap = lp.segs.windows(2).map(|w| w[0].end().dist(w[1].start())).fold(0.0, f64::max);
    let close = match (lp.segs.last(), lp.segs.first()) {
        (Some(a), Some(b)) => a.end().dist(b.start()),
        _ => return Err(bad(format!("{what}: empty outline"))),
    };
    if gap.max(close) > 1e-6 {
        return Err(bad(format!("{what}: the outline does not close ({:.2e} mm)", gap.max(close))));
    }
    if lp.signed_area() <= 0.0 {
        return Err(bad(format!("{what}: the outline turns the wrong way")));
    }
    Ok(lp)
}

fn inv(a: f64) -> f64 {
    a.tan() - a
}

/// What an involute gear's section is made from.
#[derive(Clone, Copy, Debug)]
pub struct Involute {
    /// Transverse module (mm).
    pub module: f64,
    pub teeth: usize,
    /// Transverse pressure angle (radians).
    pub pressure: f64,
    /// Profile shift coefficient x (of the module).
    pub shift: f64,
}

impl Involute {
    pub fn pitch_radius(&self) -> f64 {
        self.module * self.teeth as f64 / 2.0
    }
    pub fn tip_radius(&self) -> f64 {
        self.pitch_radius() + self.module * (1.0 + self.shift)
    }
    pub fn root_radius(&self) -> f64 {
        self.pitch_radius() - self.module * (1.25 - self.shift)
    }
}

/// An involute gear's section across its axis, centred on the origin, a tooth on +x.
pub fn involute_section(g: &Involute) -> Result<Loop2> {
    let (m, z, alpha, x) = (g.module, g.teeth, g.pressure, g.shift);
    if !(m.is_finite() && m > 0.01 && m < 1000.0) {
        return Err(bad("the module must be between 0.01 and 1000 mm"));
    }
    if !(6..=1000).contains(&z) {
        return Err(bad("a gear needs 6…1000 teeth"));
    }
    if !(alpha.is_finite() && alpha > 5f64.to_radians() && alpha < 45f64.to_radians()) {
        return Err(bad("the pressure angle must be between 5° and 45°"));
    }
    if !(x.is_finite() && x.abs() <= 2.0) {
        return Err(bad("the profile shift must be within ±2 modules"));
    }
    let r = g.pitch_radius();
    let rb = r * alpha.cos();
    let (ra, rf) = (g.tip_radius(), g.root_radius());
    if rf <= 0.0 {
        return Err(bad("too few teeth for this module: the root circle reaches the axis"));
    }
    // Half the tooth's angle at the pitch circle, and the involute's offset so that the right
    // flank (below the tooth's centre line) is the standard involute turned by `-off`.
    let psi = (PI / 2.0 + 2.0 * x * alpha.tan()) / z as f64;
    let off = psi + inv(alpha);
    // Roll angle of the involute where it reaches radius ρ.
    let roll = |rho: f64| ((rho / rb).powi(2) - 1.0).max(0.0).sqrt();
    let half = |rho: f64| off - inv((rb / rho.max(rb)).min(1.0).acos());
    let ha = half(ra);
    if ha <= 1e-4 {
        return Err(bad("the teeth come to a point: use more teeth or less profile shift"));
    }
    let r0 = rf.max(rb);
    let (t0, t1) = (roll(r0), roll(ra));
    let step = TAU / z as f64;
    // The standard involute from the base circle and its derivative in the roll angle.
    let inv_pt = move |t: f64| Vec2::new(rb * (t.cos() + t * t.sin()), rb * (t.sin() - t * t.cos()));
    let inv_d = move |t: f64| Vec2::new(rb * t * t.cos(), rb * t * t.sin());
    let mut segs = Vec::new();
    for k in 0..z {
        let c = step * k as f64;
        // Right flank: outward, turned by c - off.
        let right = hermite(|t| rot(inv_pt(t), c - off), |t| rot(inv_d(t), c - off), t0, t1, 4);
        // Left flank: the mirror image, inward.
        let mir = |v: Vec2| Vec2::new(v.x, -v.y);
        let left = reversed(hermite(|t| rot(mir(inv_pt(t)), c + off), |t| rot(mir(inv_d(t)), c + off), t0, t1, 4));
        let a_root = half(r0);
        if rf < rb - 1e-9 {
            // Below the base circle the flank runs straight down to the root circle.
            segs.push(Seg2::Line { a: Vec2::from_angle(c - a_root) * rf, b: Vec2::from_angle(c - a_root) * rb });
        }
        segs.extend(right);
        segs.push(circle_arc(ra, c - ha, c + ha));
        segs.extend(left);
        if rf < rb - 1e-9 {
            segs.push(Seg2::Line { a: Vec2::from_angle(c + a_root) * rb, b: Vec2::from_angle(c + a_root) * rf });
        }
        let next = c + step - a_root;
        if next - (c + a_root) <= 1e-6 {
            return Err(bad("the teeth overlap at the root: use a smaller profile shift"));
        }
        segs.push(circle_arc(rf, c + a_root, next));
    }
    closed(segs, "gear")
}

/// A sprocket's tooth form (mm, radians), from ISO 606.
#[derive(Clone, Copy, Debug)]
pub struct SprocketForm {
    pub pitch_diameter: f64,
    pub tip_diameter: f64,
    pub root_diameter: f64,
    pub seating_radius: f64,
    pub seating_angle: f64,
    pub flank_radius: f64,
    /// Tooth width for a simplex chain.
    pub tooth_width: f64,
}

/// The tooth form of a `teeth`-tooth sprocket for `chain`.
pub fn sprocket_form(chain: &Chain, teeth: usize) -> Result<SprocketForm> {
    if !(6..=200).contains(&teeth) {
        return Err(bad("a sprocket needs 6…200 teeth"));
    }
    let (p, d1, z) = (chain.pitch, chain.roller, teeth as f64);
    let d = p / (PI / z).sin();
    // ISO 606 limits; the form takes each halfway between them.
    let ri = 0.505 * d1 + 0.5 * 0.069 * d1.cbrt();
    let alpha = (130.0 - 90.0 / z).to_radians();
    let re = 0.5 * (0.008 * d1 * (z * z + 180.0) + 0.12 * d1 * (z + 2.0));
    let da = 0.5 * ((d + 1.25 * p - d1) + (d + p * (1.0 - 1.6 / z) - d1));
    let bf = if p <= 12.7 { 0.93 } else { 0.95 } * chain.inner_width;
    Ok(SprocketForm {
        pitch_diameter: d,
        tip_diameter: da,
        root_diameter: d - 2.0 * ri,
        seating_radius: ri,
        seating_angle: alpha,
        flank_radius: re,
        tooth_width: bf,
    })
}

/// Where the circles about `a` (radius `ra`) and `b` (radius `rb`) meet, the point nearer `near`.
fn circles_meet(a: Vec2, ra: f64, b: Vec2, rb: f64, near: Vec2) -> Option<Vec2> {
    let d = a.dist(b);
    if !(d > 1e-12) || d > ra + rb || d < (ra - rb).abs() {
        return None;
    }
    let along = (d * d + ra * ra - rb * rb) / (2.0 * d);
    let h = (ra * ra - along * along).max(0.0).sqrt();
    let u = (b - a) * (1.0 / d);
    let m = a + u * along;
    let (p, q) = (m + u.perp() * h, m - u.perp() * h);
    Some(if p.dist(near) <= q.dist(near) { p } else { q })
}

/// A sprocket's section across its axis, centred on the origin, a roller seat on +x.
pub fn sprocket_section(chain: &Chain, teeth: usize) -> Result<(Loop2, SprocketForm)> {
    let f = sprocket_form(chain, teeth)?;
    let (r, ra, ri, re, alpha) = (f.pitch_diameter / 2.0, f.tip_diameter / 2.0, f.seating_radius, f.flank_radius, f.seating_angle);
    let step = TAU / teeth as f64;
    // One tooth gap at angle 0: the roller's centre on the pitch circle, the seat's ends half the
    // seating angle either side of the direction to the axis, the flanks' centres on the lines
    // from the roller's centre through those ends, `ri + re` from it.
    let c = Vec2::new(r, 0.0);
    let inward = Vec2::new(-1.0, 0.0);
    let (low_dir, high_dir) = (rot(inward, alpha / 2.0), rot(inward, -alpha / 2.0));
    let (p_low, p_high) = (c + low_dir * ri, c + high_dir * ri);
    let (f_low, f_high) = (c + low_dir * (ri + re), c + high_dir * (ri + re));
    let o = Vec2::new(0.0, 0.0);
    let tip_low = circles_meet(f_low, re, o, ra, p_low).ok_or_else(|| bad("sprocket: the flank does not reach the tip circle"))?;
    let tip_high = circles_meet(f_high, re, o, ra, p_high).ok_or_else(|| bad("sprocket: the flank does not reach the tip circle"))?;
    // The tooth between this gap and the next: its tip from this gap's high side to the next
    // gap's low side (which is this gap's low tip turned by one pitch).
    let (a_high, a_low_next) = (tip_high.angle(), tip_low.angle() + step);
    if a_low_next - a_high <= 1e-4 {
        return Err(bad("sprocket: the teeth come to a point (tip circle too large for this chain)"));
    }
    let mut segs = Vec::new();
    for k in 0..teeth {
        let t = step * k as f64;
        let (tl, pl, ph, th) = (rot(tip_low, t), rot(p_low, t), rot(p_high, t), rot(tip_high, t));
        segs.push(arc(rot(f_low, t), tl, pl));
        segs.push(arc(rot(c, t), pl, ph));
        segs.push(arc(rot(f_high, t), ph, th));
        segs.push(circle_arc(ra, a_high + t, a_low_next + t));
    }
    // The first segment starts at the low tip; the loop closes on it.
    Ok((closed(segs, "sprocket")?, f))
}

/// A cylindrical worm (ZA: straight flanks in the axial section).
#[derive(Clone, Copy, Debug)]
pub struct Worm {
    /// Axial module (mm).
    pub module: f64,
    /// Number of starts (threads).
    pub starts: usize,
    /// Diameter quotient q: the pitch diameter is q·m.
    pub quotient: f64,
    /// Axial pressure angle (radians).
    pub pressure: f64,
}

impl Worm {
    pub fn pitch_radius(&self) -> f64 {
        self.module * self.quotient / 2.0
    }
    /// The thread's lead: the axial advance of one turn.
    pub fn lead(&self) -> f64 {
        PI * self.module * self.starts as f64
    }
    /// The lead angle at the pitch diameter (radians).
    pub fn lead_angle(&self) -> f64 {
        (self.starts as f64 / self.quotient).atan()
    }
}

/// A worm's section across its axis (the thread turns about the axis as it advances: sweeping
/// this section along a helix of the worm's lead makes the worm), centred on the origin.
pub fn worm_section(w: &Worm) -> Result<Loop2> {
    let (m, z1, q, alpha) = (w.module, w.starts, w.quotient, w.pressure);
    if !(m.is_finite() && m > 0.05 && m < 100.0) {
        return Err(bad("the worm's module must be between 0.05 and 100 mm"));
    }
    if !(1..=8).contains(&z1) {
        return Err(bad("a worm has 1…8 starts"));
    }
    if !(q.is_finite() && (5.0..=30.0).contains(&q)) {
        return Err(bad("the diameter quotient q must be between 5 and 30"));
    }
    if !(alpha.is_finite() && alpha > 5f64.to_radians() && alpha < 35f64.to_radians()) {
        return Err(bad("the pressure angle must be between 5° and 35°"));
    }
    let r1 = w.pitch_radius();
    let (ra, rf) = (r1 + m, r1 - 1.2 * m);
    if rf <= 0.5 * m {
        return Err(bad("the worm is too thin for this module: use a larger diameter quotient"));
    }
    let lead = w.lead();
    // Half the thread's axial thickness at radius r, as a turn about the axis.
    let half = |r: f64| TAU * (PI * m / 4.0 - (r - r1) * alpha.tan()) / lead;
    let dhalf = -TAU * alpha.tan() / lead;
    let (hf, ha) = (half(rf), half(ra));
    let step = TAU / z1 as f64;
    if ha <= 1e-4 || 2.0 * hf >= step {
        return Err(bad("the worm's thread does not fit this pressure angle"));
    }
    let mut segs = Vec::new();
    for k in 0..z1 {
        let c = step * k as f64;
        // Lower flank outward (θ = c − half(r)), tip, upper flank inward, root to the next start.
        let lower = hermite(
            |r| Vec2::from_angle(c - half(r)) * r,
            |r| Vec2::from_angle(c - half(r)) + Vec2::from_angle(c - half(r)).perp() * (-dhalf * r),
            rf,
            ra,
            6,
        );
        let upper = reversed(hermite(
            |r| Vec2::from_angle(c + half(r)) * r,
            |r| Vec2::from_angle(c + half(r)) + Vec2::from_angle(c + half(r)).perp() * (dhalf * r),
            rf,
            ra,
            6,
        ));
        segs.extend(lower);
        segs.push(circle_arc(ra, c - ha, c + ha));
        segs.extend(upper);
        segs.push(circle_arc(rf, c + hf, c + step - hf));
    }
    closed(segs, "worm")
}

/// The region of a section with an optional bore of `bore` diameter (0: none).
pub fn with_bore(outer: Loop2, bore: f64, root: f64) -> Result<Region2> {
    if !(bore.is_finite() && bore >= 0.0) {
        return Err(bad("the bore must be a positive diameter"));
    }
    if bore >= root * 0.98 {
        return Err(bad(format!("the bore ({bore:.2} mm) is as large as the root circle ({root:.2} mm)")));
    }
    let holes = if bore > 0.0 { vec![Loop2::circle(Vec2::new(0.0, 0.0), bore / 2.0).reversed()] } else { Vec::new() };
    Ok(Region2 { outer, holes })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_spur_gear_section_has_the_area_of_its_teeth() {
        let g = Involute { module: 2.0, teeth: 20, pressure: 20f64.to_radians(), shift: 0.0 };
        let lp = involute_section(&g).unwrap();
        let a = lp.signed_area();
        // Between the root and tip circles' areas, about the pitch circle's.
        let (ra, rf, r) = (g.tip_radius(), g.root_radius(), g.pitch_radius());
        assert!(a > PI * rf * rf && a < PI * ra * ra, "{a}");
        assert!((a - PI * r * r).abs() < 0.05 * PI * r * r, "{a} vs {}", PI * r * r);
        // Every point lies between the root and tip circles.
        for p in lp.polyline(1e-3) {
            assert!(p.len() > rf - 1e-6 && p.len() < ra + 1e-6, "{p:?}");
        }
    }

    #[test]
    fn involute_flanks_stay_on_the_involute() {
        // The tooth's thickness at the pitch circle is half the circular pitch.
        let g = Involute { module: 3.0, teeth: 30, pressure: 20f64.to_radians(), shift: 0.0 };
        let lp = involute_section(&g).unwrap();
        let r = g.pitch_radius();
        let crossings: Vec<f64> = lp.polyline(1e-5).windows(2).filter(|w| (w[0].len() - r) * (w[1].len() - r) <= 0.0).map(|w| w[0].angle()).collect();
        // Around the first tooth (on +x): two crossings ±s/(2r).
        let mut near: Vec<f64> = crossings.iter().copied().filter(|a| a.abs() < 0.1).collect();
        near.sort_by(f64::total_cmp);
        let s = (near[near.len() - 1] - near[0]) * r;
        assert!((s - PI * 3.0 / 2.0).abs() < 0.01, "{s}");
    }

    #[test]
    fn sprocket_sections_seat_the_rollers() {
        for name in ["08B", "06B", "520", "40"] {
            let c = chain(name).unwrap();
            for z in [9, 13, 20, 45] {
                let (lp, f) = sprocket_section(&c, z).unwrap();
                // A roller on the pitch circle fits in each seat: nothing of the sprocket inside it.
                let roller = Vec2::new(f.pitch_diameter / 2.0, 0.0);
                for p in lp.polyline(1e-4) {
                    assert!(p.dist(roller) >= c.roller / 2.0 - 1e-6, "{name} z{z}: {p:?}");
                }
                assert!(f.root_diameter < f.pitch_diameter && f.tip_diameter > f.pitch_diameter, "{name} z{z}: {f:?}");
            }
        }
        assert_eq!(chain("iso 08b").map(|c| c.pitch), Some(12.7));
        assert_eq!(chain("ANSI-40").map(|c| c.roller), Some(7.92));
        assert!(chain("99Z").is_none());
    }

    #[test]
    fn worm_sections_close_for_several_starts() {
        for z1 in 1..=4 {
            let w = Worm { module: 2.0, starts: z1, quotient: 10.0, pressure: 20f64.to_radians() };
            let lp = worm_section(&w).unwrap();
            let (ra, rf) = (w.pitch_radius() + 2.0, w.pitch_radius() - 2.4);
            let a = lp.signed_area();
            assert!(a > PI * rf * rf && a < PI * ra * ra, "{z1}: {a}");
        }
    }
}
