//! Sheet metal loft (a transition such as square to round): the flat pattern of the ruled wall
//! between two sections, unfolded triangle by triangle along the neutral surface.
//!
//! Between two convex sections the wall is made developable: each ruling joins points whose
//! tangents are parallel, so a straight side becomes a flat triangle and a round becomes a cone
//! fan. Sections that are not convex are joined by their share of the length instead. The wall is
//! ripped open at the middle of the longest straight side (or of the longest edge), and the
//! fans are bent in steps: one bend line per `max_bend` degrees of each fan.

use crate::sheet::BendInfo;
use crate::{DocError, Result};
use solidcraft_geom::{Plane, Vec2, Vec3};
use std::f64::consts::{PI, TAU};

/// Points a section's outline may have once sampled.
const MAX_POINTS: usize = 20_000;

/// The unfolded wall.
#[derive(Clone, Debug, PartialEq)]
pub struct Unfolded {
    /// The flat outline (counter-clockwise, flat coordinates).
    pub outline: Vec<Vec2>,
    /// The bend lines, one per step of each fan.
    pub bends: Vec<BendInfo>,
    /// The folded wall as a closed triangle mesh wound outward: the strip of the neutral
    /// surface offset along its normals, the K-factor's share of the thickness to the inside.
    pub wall: Vec<[Vec3; 3]>,
}

fn wrap(a: f64) -> f64 {
    let a = (a + PI).rem_euclid(TAU) - PI;
    if a <= -PI { a + TAU } else { a }
}

/// Closed polyline without its repeated last point; consecutive duplicates dropped.
fn clean(pts: &[Vec3]) -> Vec<Vec3> {
    let mut out: Vec<Vec3> = Vec::with_capacity(pts.len());
    for &p in pts {
        if out.last().is_none_or(|q| q.dist(p) > 1e-9) {
            out.push(p);
        }
    }
    while out.len() > 1 && out.first().zip(out.last()).is_some_and(|(a, b)| a.dist(*b) <= 1e-9) {
        out.pop();
    }
    out
}

fn get<T: Copy>(v: &[T], i: usize) -> Result<T> {
    v.get(i).copied().ok_or_else(|| DocError::Invalid("sheet metal loft: an index ran past its section".into()))
}

/// Edge `k`'s direction angle (projected).
fn edge_angles(p2: &[Vec2]) -> Vec<f64> {
    let n = p2.len();
    (0..n).map(|k| p2.get((k + 1) % n).copied().unwrap_or_default() - p2.get(k).copied().unwrap_or_default()).map(|d| d.angle()).collect()
}

/// Whether a counter-clockwise polygon turns left (or goes straight) at every corner.
fn convex(p2: &[Vec2]) -> bool {
    let a = edge_angles(p2);
    let n = a.len();
    let mut total = 0.0;
    for k in 0..n {
        let turn = wrap(a.get((k + 1) % n).copied().unwrap_or_default() - a.get(k).copied().unwrap_or_default());
        if turn < -1e-6 {
            return false;
        }
        total += turn;
    }
    (total - TAU).abs() < 1e-3
}

fn signed_area(p2: &[Vec2]) -> f64 {
    let n = p2.len();
    (0..n)
        .map(|k| {
            let (a, b) = (p2.get(k).copied().unwrap_or_default(), p2.get((k + 1) % n).copied().unwrap_or_default());
            a.cross(b)
        })
        .sum::<f64>()
        / 2.0
}

/// `pts` turned to start at `i`, closed (its first point repeated at the end).
fn from(pts: &[Vec3], i: usize) -> Vec<Vec3> {
    let n = pts.len();
    (0..=n).filter_map(|k| pts.get((i + k) % n).copied()).collect()
}

/// Insert the point `f` of the way along edge `k` and return its index.
fn split(pts: &mut Vec<Vec3>, k: usize, f: f64) -> usize {
    let n = pts.len();
    let (a, b) = (pts.get(k).copied().unwrap_or_default(), pts.get((k + 1) % n).copied().unwrap_or_default());
    pts.insert(k + 1, a.lerp(b, f.clamp(1e-3, 1.0 - 1e-3)));
    k + 1
}

/// The edge of a closed polyline nearest `p`, how far along it, and the distance.
fn nearest(pts: &[Vec3], p: Vec3) -> (usize, f64, f64) {
    let n = pts.len();
    (0..n)
        .map(|k| {
            let (a, b) = (pts.get(k).copied().unwrap_or_default(), pts.get((k + 1) % n).copied().unwrap_or_default());
            let d = b - a;
            let f = if d.len2() > 0.0 { ((p - a).dot(d) / d.len2()).clamp(0.0, 1.0) } else { 0.0 };
            (k, f, a.lerp(b, f).dist(p))
        })
        .min_by(|x, y| x.2.total_cmp(&y.2))
        .unwrap_or((0, 0.5, f64::MAX))
}

/// The sort key of each edge of a closed polyline (first point repeated at the end): its
/// direction turned from the rip's, unwrapped (convex), or the middle of its share of the length.
fn keys(pts: &[Vec3], frame: &Plane, rip: f64, by_angle: bool) -> Vec<f64> {
    let p2: Vec<Vec2> = pts.iter().map(|p| frame.to_local(*p)).collect();
    let m = p2.len().saturating_sub(1);
    if by_angle {
        let mut out = Vec::with_capacity(m);
        let mut prev: Option<f64> = None;
        let mut acc = 0.0;
        for k in 0..m {
            let d = p2.get(k + 1).copied().unwrap_or_default() - p2.get(k).copied().unwrap_or_default();
            let a = d.angle();
            acc = match prev {
                // A start just short of the rip's direction counts as its beginning.
                None => {
                    let w = wrap(a - rip);
                    if w < -1e-6 { w + TAU } else { w.max(0.0) }
                }
                Some(p) => acc + wrap(a - p),
            };
            prev = Some(a);
            out.push(acc);
        }
        out
    } else {
        let lens: Vec<f64> = pts.windows(2).map(|w| w[0].dist(w[1])).collect();
        let total: f64 = lens.iter().sum::<f64>().max(1e-12);
        let mut s = 0.0;
        lens.iter()
            .map(|l| {
                let mid = (s + l / 2.0) / total;
                s += l;
                mid
            })
            .collect()
    }
}

/// Unfold the wall between two closed section polylines (`a` the first section, `b` the last;
/// neutral surface, 3D). `frame` is the first section's plane; `gap` the rip's width; `max_bend`
/// the largest angle (radians) one bend line takes of a fan; `radius` the bends' inside radius.
/// `t` and `k` are the sheet's thickness and K-factor (the folded wall's faces lie `k·t` inside
/// and `(1 − k)·t` outside the neutral surface).
#[allow(clippy::too_many_arguments)]
pub fn unfold(
    a: &[Vec3],
    b: &[Vec3],
    frame: &Plane,
    rip_at: Option<Vec3>,
    gap: f64,
    max_bend: f64,
    radius: f64,
    t: f64,
    kf: f64,
) -> Result<Unfolded> {
    if !(t.is_finite() && t > 0.0 && kf.is_finite() && (0.0..=1.0).contains(&kf)) {
        return Err(DocError::Invalid("sheet metal loft: bad thickness or K-factor".into()));
    }
    let (mut a, mut b) = (clean(a), clean(b));
    if a.len() < 3 || b.len() < 3 {
        return Err(DocError::Invalid("sheet metal loft: each section needs a closed outline".into()));
    }
    if a.len() > MAX_POINTS || b.len() > MAX_POINTS || a.iter().chain(&b).any(|p| !p.is_finite()) {
        return Err(DocError::Invalid("sheet metal loft: the sections are too detailed".into()));
    }
    if !(gap.is_finite() && gap >= 0.0 && max_bend.is_finite() && max_bend > 1e-3 && radius.is_finite()) {
        return Err(DocError::Invalid("sheet metal loft: bad gap or bend angle".into()));
    }
    // Both turning the same way, counter-clockwise seen along the frame's normal.
    let a2: Vec<Vec2> = a.iter().map(|p| frame.to_local(*p)).collect();
    if signed_area(&a2) < 0.0 {
        a.reverse();
    }
    let b2: Vec<Vec2> = b.iter().map(|p| frame.to_local(*p)).collect();
    if signed_area(&b2) < 0.0 {
        b.reverse();
    }
    let a2: Vec<Vec2> = a.iter().map(|p| frame.to_local(*p)).collect();
    let b2: Vec<Vec2> = b.iter().map(|p| frame.to_local(*p)).collect();
    let by_angle = convex(&a2) && convex(&b2);
    // The rip: the middle of the longest edge of either section.
    let longest = |p: &[Vec3]| {
        let n = p.len();
        (0..n)
            .map(|k| (k, p.get(k).copied().unwrap_or_default().dist(p.get((k + 1) % n).copied().unwrap_or_default())))
            .max_by(|x, y| x.1.total_cmp(&y.1))
            .unwrap_or((0, 0.0))
    };
    // Or where it is asked for: the point of either section's edge nearest `rip_at`.
    let (first_on_a, k, frac) = match rip_at {
        Some(p) if p.is_finite() => {
            let (na, nb) = (nearest(&a, p), nearest(&b, p));
            if na.2 <= nb.2 { (true, na.0, na.1) } else { (false, nb.0, nb.1) }
        }
        _ => {
            let (la, lb) = (longest(&a), longest(&b));
            if la.1 >= lb.1 { (true, la.0, 0.5) } else { (false, lb.0, 0.5) }
        }
    };
    let (rip_sec, other) = if first_on_a { (&mut a, &mut b) } else { (&mut b, &mut a) };
    let n = rip_sec.len();
    let dir2 = |p: &[Vec3], k: usize| {
        let m = p.len();
        frame.to_local(p.get((k + 1) % m).copied().unwrap_or_default()) - frame.to_local(p.get(k).copied().unwrap_or_default())
    };
    let rip_angle = dir2(rip_sec, k % n).angle();
    let s = split(rip_sec, k, frac);
    let at = get(rip_sec, s)?;
    // The other section's start: the middle of a parallel edge, else (convex) the corner whose
    // tangents straddle the rip's direction, else the point nearest the rip.
    let m = other.len();
    let o = if by_angle {
        let ang: Vec<f64> = (0..m).map(|j| dir2(other, j).angle()).collect();
        let parallel = (0..m)
            .filter(|&j| wrap(ang.get(j).copied().unwrap_or_default() - rip_angle).abs() < 1e-4)
            .max_by(|x, y| dir2(other, *x).len().total_cmp(&dir2(other, *y).len()));
        match parallel {
            Some(j) => split(other, j, 0.5),
            None => (0..m)
                .find(|&j| {
                    let before = wrap(ang.get((j + m - 1) % m).copied().unwrap_or_default() - rip_angle);
                    let after = wrap(ang.get(j).copied().unwrap_or_default() - rip_angle);
                    before <= 0.0 && after > 0.0
                })
                .unwrap_or(0),
        }
    } else {
        let p = frame.to_local(at);
        (0..m)
            .min_by(|x, y| {
                frame
                    .to_local(other.get(*x).copied().unwrap_or_default())
                    .dist(p)
                    .total_cmp(&frame.to_local(other.get(*y).copied().unwrap_or_default()).dist(p))
            })
            .unwrap_or(0)
    };
    let (sa, sb) = if first_on_a { (s, o) } else { (o, s) };
    let (mut ta, mut tb) = (from(&a, sa), from(&b, sb));
    let (ka, kb) = (keys(&ta, frame, rip_angle, by_angle), keys(&tb, frame, rip_angle, by_angle));
    let (na, nb) = (ta.len() - 1, tb.len() - 1);
    // The strip: rulings (i on a, j on b), each step adding the next point of one section.
    let mut steps: Vec<(usize, usize, bool)> = Vec::with_capacity(na + nb);
    let (mut i, mut j) = (0usize, 0usize);
    while i < na || j < nb {
        let on_a = j >= nb || (i < na && get(&ka, i)? <= get(&kb, j)?);
        steps.push((i, j, on_a));
        if on_a {
            i += 1;
        } else {
            j += 1;
        }
    }
    // The rip's width, half off each end, along the sections.
    let trim = |f: &mut Vec<Vec3>, first: bool| {
        let n = f.len();
        let (k, toward) = if first { (0, 1) } else { (n.saturating_sub(1), n.saturating_sub(2)) };
        if let (Some(&p), Some(&q)) = (f.get(k), f.get(toward)) {
            let d = q - p;
            if d.len() > gap
                && let (Some(slot), Some(u)) = (f.get_mut(k), d.normalized())
            {
                *slot = p + u * (gap / 2.0);
            }
        }
    };
    if gap > 0.0 {
        trim(&mut ta, true);
        trim(&mut ta, false);
        trim(&mut tb, true);
        trim(&mut tb, false);
    }
    // Lay the triangles flat: the first ruling up the y axis, the strip running to +x.
    let mut fa = vec![Vec2::new(0.0, 0.0); na + 1];
    let mut fb = vec![Vec2::new(0.0, 0.0); nb + 1];
    let l0 = get(&ta, 0)?.dist(get(&tb, 0)?);
    if !(l0 > 1e-9) {
        return Err(DocError::Invalid("sheet metal loft: the sections touch at the rip".into()));
    }
    if let Some(p) = fa.get_mut(0) {
        *p = Vec2::new(0.0, l0);
    }
    let mut normals: Vec<Option<Vec3>> = Vec::with_capacity(steps.len());
    for &(i, j, on_a) in &steps {
        let (pa, pb) = (get(&ta, i)?, get(&tb, j)?);
        let (qa, qb) = (get(&fa, i)?, get(&fb, j)?);
        let v = if on_a { get(&ta, i + 1)? } else { get(&tb, j + 1)? };
        let (d1, d2) = (v.dist(pa), v.dist(pb));
        let e = qa - qb;
        let l = e.len();
        let Some(u) = e.normalized() else { return Err(DocError::Invalid("sheet metal loft: a ruling has no length".into())) };
        let along = (d2 * d2 - d1 * d1 + l * l) / (2.0 * l);
        let h = (d2 * d2 - along * along).max(0.0).sqrt();
        let flat = qb + u * along - u.perp() * h;
        if on_a {
            if let Some(p) = fa.get_mut(i + 1) {
                *p = flat;
            }
        } else if let Some(p) = fb.get_mut(j + 1) {
            *p = flat;
        }
        // Outward-consistent normal of the triangle (b, a, v).
        normals.push((pa - pb).cross(v - pb).normalized());
    }
    // Dihedral at each interior ruling: between step s-1 and step s, the ruling (i, j) of step s.
    let mut fold: Vec<(usize, f64)> = Vec::new();
    let mut last: Option<Vec3> = None;
    for (s, n) in normals.iter().enumerate() {
        let Some(n) = *n else { continue };
        if let (Some(p), Some(&(i, j, _))) = (last, steps.get(s)) {
            let r = (get(&ta, i)? - get(&tb, j)?).normalized().unwrap_or_default();
            let c = p.cross(n);
            let ang = c.len().atan2(p.dot(n)) * if c.dot(r) >= 0.0 { 1.0 } else { -1.0 };
            fold.push((s, ang));
        }
        last = Some(n);
    }
    // Fans: runs of rulings that fold the same way, broken where the strip crosses a straight
    // side (an edge much longer than the sampled curves' steps) and where the fold turns round.
    let mut lens: Vec<f64> = ta.windows(2).chain(tb.windows(2)).map(|w| w[0].dist(w[1])).collect();
    lens.sort_by(f64::total_cmp);
    let long = 10.0 * lens.get(lens.len() / 2).copied().unwrap_or(0.0);
    let small = 0.01f64.to_radians();
    let folds: std::collections::BTreeMap<usize, f64> = fold.into_iter().collect();
    let mut groups: Vec<Vec<(usize, f64)>> = vec![Vec::new()];
    for (s, &(i, j, on_a)) in steps.iter().enumerate() {
        if let Some(&ang) = folds.get(&s).filter(|a| a.abs() >= small) {
            let turned = groups.last().and_then(|g| g.last()).is_some_and(|x| x.1.signum() != ang.signum());
            if turned {
                groups.push(Vec::new());
            }
            if let Some(g) = groups.last_mut() {
                g.push((s, ang));
            }
        }
        let edge = if on_a { get(&ta, i)?.dist(get(&ta, i + 1)?) } else { get(&tb, j)?.dist(get(&tb, j + 1)?) };
        if edge > long && groups.last().is_some_and(|g| !g.is_empty()) {
            groups.push(Vec::new());
        }
    }
    let mut bends = Vec::new();
    for g in groups.iter().filter(|g| !g.is_empty()) {
        let total: f64 = g.iter().map(|x| x.1).sum();
        let count = ((total.abs() / max_bend).ceil() as usize).clamp(1, 1000);
        for c in 0..count {
            let want = total.abs() * (c as f64 + 0.5) / count as f64;
            let mut acc = 0.0;
            let mut pick = g.first().map_or(0, |x| x.0);
            for &(s, ang) in g {
                acc += ang.abs();
                pick = s;
                if acc >= want {
                    break;
                }
            }
            let Some(&(i, j, _)) = steps.get(pick) else { continue };
            let (start, end) = (get(&fb, j)?, get(&fa, i)?);
            bends.push(BendInfo {
                index: bends.len() + 1,
                angle_deg: (total / count as f64).abs().to_degrees(),
                radius,
                up: total >= 0.0,
                allowance: 0.0,
                length: start.dist(end),
                start,
                end,
            });
        }
    }
    let mut outline: Vec<Vec2> = fb.clone();
    outline.extend(fa.iter().rev());
    if signed_area(&outline) < 0.0 {
        outline.reverse();
    }
    let wall = folded(&ta, &tb, &steps, t, kf)?;
    Ok(Unfolded { outline, bends, wall })
}

/// The folded wall (see [`Unfolded::wall`]) of the strip `steps` between `ta` and `tb`.
fn folded(ta: &[Vec3], tb: &[Vec3], steps: &[(usize, usize, bool)], t: f64, k: f64) -> Result<Vec<[Vec3; 3]>> {
    let tri = |&(i, j, on_a): &(usize, usize, bool)| -> Result<[Vec3; 3]> {
        Ok([get(tb, j)?, get(ta, i)?, if on_a { get(ta, i + 1)? } else { get(tb, j + 1)? }])
    };
    let strip: Vec<[Vec3; 3]> = steps.iter().map(tri).collect::<Result<_>>()?;
    let count = (ta.len() + tb.len()).max(1) as f64;
    let centre = ta.iter().chain(tb).fold(Vec3::ZERO, |s, p| s + *p) * (1.0 / count);
    let normal = |x: &[Vec3; 3]| (x[1] - x[0]).cross(x[2] - x[0]);
    // Outward: away from the middle of the wall.
    let out: f64 = strip.iter().map(|x| normal(x).dot((x[0] + x[1] + x[2]) * (1.0 / 3.0) - centre)).sum();
    let sign = if out >= 0.0 { 1.0 } else { -1.0 };
    // Vertex normals, area-weighted.
    let (mut na, mut nb) = (vec![Vec3::ZERO; ta.len()], vec![Vec3::ZERO; tb.len()]);
    for (&(i, j, on_a), x) in steps.iter().zip(&strip) {
        let n = normal(x) * sign;
        let add = |v: &mut Vec<Vec3>, q: usize| {
            if let Some(slot) = v.get_mut(q) {
                *slot += n;
            }
        };
        add(&mut na, i);
        add(&mut nb, j);
        if on_a {
            add(&mut na, i + 1);
        } else {
            add(&mut nb, j + 1);
        }
    }
    let unit = |v: &[Vec3]| -> Result<Vec<Vec3>> {
        v.iter().map(|n| n.normalized().ok_or_else(|| DocError::Invalid("sheet metal loft: the wall folds onto itself".into()))).collect()
    };
    let (na, nb) = (unit(&na)?, unit(&nb)?);
    let (dout, din) = ((1.0 - k) * t, k * t);
    let off = |p: Vec3, n: Vec3| (p + n * dout, p - n * din);
    let oa: Vec<(Vec3, Vec3)> = ta.iter().zip(&na).map(|(p, n)| off(*p, *n)).collect();
    let ob: Vec<(Vec3, Vec3)> = tb.iter().zip(&nb).map(|(p, n)| off(*p, *n)).collect();
    let mut wall = Vec::with_capacity(4 * strip.len() + 8);
    // Each triangle wound to face `want`.
    let mut push = |x: [Vec3; 3], want: Vec3| {
        if normal(&x).len() <= 1e-12 {
            return;
        }
        wall.push(if normal(&x).dot(want) >= 0.0 { x } else { [x[0], x[2], x[1]] });
    };
    for (&(i, j, on_a), x) in steps.iter().zip(&strip) {
        let n = normal(x) * sign;
        let pick = |s: &[(Vec3, Vec3)], q: usize, outer: bool| s.get(q).map(|p| if outer { p.0 } else { p.1 });
        let v = |outer: bool| if on_a { pick(&oa, i + 1, outer) } else { pick(&ob, j + 1, outer) };
        for outer in [true, false] {
            if let (Some(p0), Some(p1), Some(p2)) = (pick(&ob, j, outer), pick(&oa, i, outer), v(outer)) {
                push([p0, p1, p2], if outer { n } else { n * -1.0 });
            }
        }
    }
    // The open ends: each section's edge, facing away from the other section.
    let (ca, cb) = (
        ta.iter().fold(Vec3::ZERO, |s, p| s + *p) * (1.0 / ta.len().max(1) as f64),
        tb.iter().fold(Vec3::ZERO, |s, p| s + *p) * (1.0 / tb.len().max(1) as f64),
    );
    for (o, away) in [(&oa, ca - cb), (&ob, cb - ca)] {
        for w in o.windows(2) {
            let ((p, pi), (q, qi)) = (w[0], w[1]);
            push([p, q, qi], away);
            push([p, qi, pi], away);
        }
    }
    // The rip's two faces.
    let (Some(a0), Some(b0), Some(a1), Some(an), Some(bn), Some(an1)) =
        (oa.first(), ob.first(), ta.get(1), oa.last(), ob.last(), ta.get(ta.len().saturating_sub(2)))
    else {
        return Err(DocError::Invalid("sheet metal loft: the rip".into()));
    };
    let (s0, sn) = (ta.first().copied().unwrap_or_default(), ta.last().copied().unwrap_or_default());
    for ((a, b), away) in [((a0, b0), s0 - *a1), ((an, bn), sn - *an1)] {
        push([a.0, b.0, b.1], away);
        push([a.0, b.1, a.1], away);
    }
    Ok(wall)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(z: f64, h: f64) -> Vec<Vec3> {
        vec![Vec3::new(-h, -h, z), Vec3::new(h, -h, z), Vec3::new(h, h, z), Vec3::new(-h, h, z)]
    }
    fn circle(z: f64, r: f64, n: usize) -> Vec<Vec3> {
        (0..n).map(|k| k as f64 * TAU / n as f64).map(|a| Vec3::new(r * a.cos(), r * a.sin(), z)).collect()
    }
    fn area(p: &[Vec2]) -> f64 {
        signed_area(p)
    }

    #[test]
    fn a_cylinder_unrolls_to_a_rectangle() {
        let (a, b) = (circle(0.0, 50.0, 360), circle(100.0, 50.0, 360));
        let u = unfold(&a, &b, &Plane::XY, None, 0.0, 10f64.to_radians(), 1.0, 1.0, 0.5).unwrap();
        let circ = 360.0 * 2.0 * 50.0 * (PI / 360.0).sin();
        assert!((area(&u.outline) - circ * 100.0).abs() < 1.0, "area {}", area(&u.outline));
        // 360° of fan in bends of at most 10°.
        assert_eq!(u.bends.len(), 36);
        // The folded wall: a closed tube 1 thick round the neutral radius.
        let vol: f64 = u.wall.iter().map(|x| x[0].dot(x[1].cross(x[2]))).sum::<f64>() / 6.0;
        assert!((vol - circ * 100.0).abs() < 0.01 * vol, "volume {vol}");
    }

    #[test]
    fn square_to_round_makes_flat_triangles_and_four_fans() {
        let (a, b) = (square(0.0, 50.0), circle(80.0, 30.0, 720));
        let u = unfold(&a, &b, &Plane::XY, None, 0.0, 15f64.to_radians(), 1.0, 1.0, 0.5).unwrap();
        // Each corner fans through 90°: 6 bends of 15°.
        assert_eq!(u.bends.len(), 24, "{:?}", u.bends.iter().map(|b| b.angle_deg).collect::<Vec<_>>());
        assert!(u.bends.iter().all(|b| (b.angle_deg - 15.0).abs() < 0.5));
        // The flat keeps the wall's area: four triangles plus four cones.
        let slant = |d: f64| (d * d + 80.0f64 * 80.0).sqrt();
        let tri = 4.0 * 0.5 * 100.0 * slant(20.0);
        assert!(area(&u.outline) > tri, "{}", area(&u.outline));
        // Its two rip edges are as long as the wall is high at the middle of a side.
        let (first, last) = (u.outline.first().copied(), u.outline.last().copied());
        assert!(first.zip(last).is_some_and(|(p, q)| (p.dist(q) - slant(20.0)).abs() < 1e-6));
    }

    #[test]
    fn a_rip_elsewhere_keeps_the_material() {
        let (a, b) = (square(0.0, 50.0), circle(80.0, 30.0, 720));
        let mid = unfold(&a, &b, &Plane::XY, None, 0.0, 15f64.to_radians(), 1.0, 1.0, 0.5).unwrap();
        let near_corner = unfold(&a, &b, &Plane::XY, Some(Vec3::new(40.0, 50.0, 0.0)), 0.0, 15f64.to_radians(), 1.0, 1.0, 0.5).unwrap();
        assert!((area(&mid.outline) - area(&near_corner.outline)).abs() < 1e-3 * area(&mid.outline));
        // The rip edge now starts 10 from the corner.
        let first = near_corner.outline.first().copied().zip(near_corner.outline.last().copied());
        assert!(first.is_some_and(|(p, q)| (p.dist(q) - mid.outline[0].dist(*mid.outline.last().unwrap())).abs() > 1.0));
    }

    #[test]
    fn hostile_sections_are_refused() {
        let nan = vec![Vec3::new(f64::NAN, 0.0, 0.0), Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, 1.0, 0.0)];
        assert!(unfold(&nan, &square(5.0, 1.0), &Plane::XY, None, 0.0, 0.2, 1.0, 1.0, 0.5).is_err());
        assert!(unfold(&square(0.0, 1.0)[..2], &square(5.0, 1.0), &Plane::XY, None, 0.0, 0.2, 1.0, 1.0, 0.5).is_err());
        assert!(unfold(&square(0.0, 1.0), &square(0.0, 1.0), &Plane::XY, None, 0.0, 0.2, 1.0, 1.0, 0.5).is_err());
        assert!(unfold(&square(0.0, 1.0), &square(5.0, 1.0), &Plane::XY, None, f64::NAN, 0.2, 1.0, 1.0, 0.5).is_err());
    }
}
