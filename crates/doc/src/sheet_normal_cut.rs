//! A cut across the sheet's folds (a normal cut): a through cut that meets a folded bend, or a
//! panel not parallel to the cut's plane, takes out the sheet wherever its middle surface lies
//! inside the tool, square to the sheet, as a sheet metal cut-out does. Its cut-outs are worked
//! out in the flat pattern, so that the folded part, its flat pattern and its DXF carry them.
//!
//! In a bend the flat-to-folded map is not a motion, so the cut-out there is traced: the
//! piece's flat area is sampled on a fine grid (aligned with the bend), each grid edge that
//! crosses the tool's boundary is bisected to where it crosses, the crossings are joined into
//! loops (marching squares), and the corners a grid square cut off are put back where the two
//! straight boundaries beside them meet. Straight boundaries come out exact, curved ones within
//! the grid's chord error (well under a hundredth of a millimetre). The panels parallel to the
//! cut's plane keep their exact cut-outs (`sheet_prism_cut`).

use super::*;

/// How far (mm) a traced cut-out runs past its piece's edge: a hair over the piece next to it,
/// so that the two cut-outs overlap rather than meet edge to edge.
const SEAM_OVERLAP: f64 = 1e-4;
/// The finest grid step (mm), and the most samples one cut may take.
const FINE_STEP: f64 = 0.05;
const MAX_SAMPLES: usize = 4_000_000;
/// Bisection steps for a boundary crossing: the grid step over 2^40.
const BISECT: usize = 40;

/// A piece of the sheet as the trace sees it: where its flat area is and how it folds.
struct Traced {
    /// The grid's frame in flat coordinates: origin and its two directions (unit, square).
    origin: Vec2,
    e1: Vec2,
    e2: Vec2,
    /// The grid's extent in that frame (overlap included).
    lo: Vec2,
    hi: Vec2,
    /// The flat region to keep the result inside (panels), None for a bend (its frame's
    /// rectangle is its extent).
    clip: Option<Region2>,
    fold: Fold,
}

enum Fold {
    Panel(Mat),
    Bend { parent: Mat, origin: Vec3, axis: Vec3, turn: f64, ba: f64, p: Vec2, d: Vec2, n: Vec2 },
}

impl SheetBody {
    /// The flat cut-outs a through cut makes in the pieces `which` of
    /// [`SheetBody::unmodeled_solids`] (the same order): where the folded middle surface of
    /// each lies inside the tool (`inside`, a world point).
    pub(crate) fn normal_cut(&self, plane: &Plane, which: &[usize], inside: &dyn Fn(Vec3) -> bool, work: &mut FlatWork) -> Result<Vec<Region2>> {
        if self.flat || self.flanges.is_empty() || which.is_empty() {
            return Ok(Vec::new());
        }
        let cells: Vec<Cell> = self
            .cells_with_work(work)?
            .into_iter()
            .filter(|c| match c.piece {
                Piece::Zone(_) => true,
                Piece::Panel(pi) => apply_vector(&self.world(pi), Vec3::Z).cross(plane.normal()).len() > 1e-6,
            })
            .collect();
        let frame = self.frame_mat();
        let t = self.t;
        let mut out = Vec::new();
        for (ci, cell) in cells.iter().enumerate() {
            if !which.contains(&ci) {
                continue;
            }
            let Some(piece) = self.traced(cell)? else { continue };
            let to_world = |q: Vec2, z: f64| -> Vec3 {
                match &piece.fold {
                    Fold::Panel(m) => apply_point(m, Vec3::new(q.x, q.y, z)),
                    Fold::Bend { parent, origin, axis, turn, ba, p, d, n } => {
                        let s = (q - *p).dot(*n);
                        let u = (q - *p).dot(*d);
                        let phi = turn.signum() * turn.abs() * s / ba;
                        let local = apply_point(&mat_mul(parent, &rot_about(*origin, *axis, phi)), v3(*p + *d * u) + Vec3::Z * z);
                        apply_point(&frame, local)
                    }
                }
            };
            // Through the sheet: its middle inside the tool, and a face of it (the tool's walls may
            // run askew to the sheet, so not always both).
            let class = |q: Vec2| [inside(to_world(q, t / 2.0)), inside(to_world(q, 0.0)), inside(to_world(q, t))];
            let regions = trace(&piece, &class, work)?;
            let regions = match &piece.clip {
                Some(clip) if !regions.is_empty() => material_boolean(&regions, std::slice::from_ref(clip), kernel::BoolOp::Intersect, work)?,
                _ => regions,
            };
            out.extend(regions);
        }
        Ok(out)
    }

    /// How a piece is traced: its grid frame and its fold; None for a bend with no width.
    fn traced(&self, cell: &Cell) -> Result<Option<Traced>> {
        let ring: Vec<Vec2> = cell.outer.iter().flat_map(|s| s.polyline(1e-3)).collect();
        match cell.piece {
            Piece::Zone(i) => {
                let Some(f) = self.flanges.get(i) else { return Ok(None) };
                let ba = self.allowance(f);
                if !(ba > 1e-6) || !f.angle.is_finite() || f.angle.abs() < 1e-9 {
                    return Ok(None);
                }
                let (d, n) = (f.d, f.n);
                let (mut ulo, mut uhi) = (f64::MAX, f64::MIN);
                for q in &ring {
                    let u = (*q - f.p).dot(d);
                    ulo = ulo.min(u);
                    uhi = uhi.max(u);
                }
                if ulo > uhi {
                    return Ok(None);
                }
                let (axis_o, k, turn) = self.bend_axis(f);
                let parent = f.parent.map_or(IDENTITY, |j| self.fold(j));
                Ok(Some(Traced {
                    origin: f.p,
                    e1: d,
                    e2: n,
                    lo: Vec2::new(ulo - SEAM_OVERLAP, -SEAM_OVERLAP),
                    hi: Vec2::new(uhi + SEAM_OVERLAP, ba + SEAM_OVERLAP),
                    clip: None,
                    fold: Fold::Bend { parent, origin: axis_o, axis: k, turn, ba, p: f.p, d, n },
                }))
            }
            Piece::Panel(pi) => {
                let (mut lo, mut hi) = (Vec2::new(f64::MAX, f64::MAX), Vec2::new(f64::MIN, f64::MIN));
                for q in &ring {
                    lo = Vec2::new(lo.x.min(q.x), lo.y.min(q.y));
                    hi = Vec2::new(hi.x.max(q.x), hi.y.max(q.y));
                }
                if lo.x > hi.x {
                    return Ok(None);
                }
                let grow = Vec2::new(SEAM_OVERLAP, SEAM_OVERLAP);
                let clip = Region2 { outer: Loop2 { segs: cell.outer.clone() }, holes: cell.holes.iter().map(|segs| Loop2 { segs: segs.clone() }).collect() };
                Ok(Some(Traced {
                    origin: Vec2::ZERO,
                    e1: Vec2::new(1.0, 0.0),
                    e2: Vec2::new(0.0, 1.0),
                    lo: lo - grow,
                    hi: hi + grow,
                    clip: Some(clip),
                    fold: Fold::Panel(self.world(pi)),
                }))
            }
        }
    }
}

/// The regions (flat coordinates) of a piece's extent where `inside` holds.
fn trace(piece: &Traced, class: &dyn Fn(Vec2) -> [bool; 3], work: &mut FlatWork) -> Result<Vec<Region2>> {
    let inside = |q: Vec2| {
        let [m, a, b] = class(q);
        m && (a || b)
    };
    let size = piece.hi - piece.lo;
    if !(size.x > 0.0 && size.y > 0.0) || !size.x.is_finite() || !size.y.is_finite() {
        return Ok(Vec::new());
    }
    // As fine as the budget allows, never finer than FINE_STEP.
    let step = FINE_STEP.max((size.x * size.y / MAX_SAMPLES as f64).sqrt());
    let nx = ((size.x / step).ceil() as usize).max(1);
    let ny = ((size.y / step).ceil() as usize).max(1);
    let samples = (nx + 3).saturating_mul(ny + 3);
    if samples > MAX_SAMPLES * 2 {
        return Err(flat_budget_error());
    }
    work.charge(samples)?;
    let (hx, hy) = (size.x / nx as f64, size.y / ny as f64);
    // Frame coordinates → flat; outside the extent nothing is inside (so every loop closes).
    let at = |x: f64, y: f64| piece.origin + piece.e1 * x + piece.e2 * y;
    let within = |x: f64, y: f64| x >= piece.lo.x && x <= piece.hi.x && y >= piece.lo.y && y <= piece.hi.y;
    let test = |x: f64, y: f64| within(x, y) && inside(at(x, y));
    // Grid points i in 0..=nx+2, j in 0..=ny+2: the ring around the extent is outside.
    let gx = |i: usize| piece.lo.x + (i as f64 - 1.0) * hx;
    let gy = |j: usize| piece.lo.y + (j as f64 - 1.0) * hy;
    let (w, h) = (nx + 3, ny + 3);
    let mut val = vec![false; w * h];
    // How much of the middle surface inside the tool is cut through, and how much only inside
    // the sheet (a bore, a pocket): that is no cut-out.
    let (mut middle, mut through, mut inner) = (0usize, 0usize, 0usize);
    for j in 1..h - 1 {
        for i in 1..w - 1 {
            let (x, y) = (gx(i), gy(j));
            let [m, a, b] = if within(x, y) { class(at(x, y)) } else { [false; 3] };
            val[j * w + i] = m && (a || b);
            middle += usize::from(m);
            through += usize::from(m && a && b);
            inner += usize::from(m && !a && !b);
        }
    }
    if middle == 0 {
        return Ok(Vec::new());
    }
    if inner * 50 > middle || through * 2 < middle {
        return Err(DocError::Invalid(
            "this machining meets a folded bend or a nonparallel panel only in part of the sheet's thickness; cut through the sheet there, or unfold it first"
                .into(),
        ));
    }
    let v = |i: usize, j: usize| val.get(j * w + i).copied().unwrap_or(false);
    // A crossing on a grid edge, by bisection between its inside and outside ends.
    let cross = |(x0, y0): (f64, f64), (x1, y1): (f64, f64)| -> (f64, f64) {
        let (mut a, mut b) = ((x0, y0), (x1, y1));
        if !test(a.0, a.1) {
            std::mem::swap(&mut a, &mut b);
        }
        for _ in 0..BISECT {
            let m = ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
            if test(m.0, m.1) {
                a = m;
            } else {
                b = m;
            }
        }
        ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0)
    };
    // Edge ids: horizontal (i, j)–(i+1, j) is 2·(j·w + i), vertical (i, j)–(i, j+1) is 2·(j·w + i) + 1.
    let hid = |i: usize, j: usize| 2 * (j * w + i);
    let vid = |i: usize, j: usize| 2 * (j * w + i) + 1;
    let mut points: std::collections::HashMap<usize, (f64, f64)> = std::collections::HashMap::new();
    let mut point = |id: usize, a: (f64, f64), b: (f64, f64)| *points.entry(id).or_insert_with(|| cross(a, b));
    // Segments, inside on their left: from an exit crossing to an entry crossing.
    let mut next: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    for j in 0..h - 1 {
        for i in 0..w - 1 {
            let c = [v(i, j), v(i + 1, j), v(i + 1, j + 1), v(i, j + 1)];
            if c.iter().all(|x| *x) || c.iter().all(|x| !*x) {
                continue;
            }
            let p = [(gx(i), gy(j)), (gx(i + 1), gy(j)), (gx(i + 1), gy(j + 1)), (gx(i), gy(j + 1))];
            let ids = [hid(i, j), vid(i + 1, j), hid(i, j + 1), vid(i, j)];
            // Around the square counter-clockwise: corner k to corner k+1 crosses edge k.
            let mut xs: Vec<(usize, bool)> = Vec::new();
            for k in 0..4 {
                let (a, b) = (c[k], c[(k + 1) % 4]);
                if a != b {
                    let id = ids[k];
                    point(id, p[k], p[(k + 1) % 4]);
                    xs.push((id, a)); // a inside: leaving (an exit)
                }
            }
            if xs.len() == 2 {
                let (e, n) = if xs[0].1 { (xs[0].0, xs[1].0) } else { (xs[1].0, xs[0].0) };
                next.insert(e, n);
            } else if xs.len() == 4 {
                // A saddle: the centre decides whether the two inside corners join.
                let centre = test((p[0].0 + p[2].0) / 2.0, (p[0].1 + p[2].1) / 2.0);
                for k in 0..4 {
                    if !xs[k].1 {
                        continue;
                    }
                    let to = if centre { xs[(k + 1) % 4].0 } else { xs[(k + 3) % 4].0 };
                    next.insert(xs[k].0, to);
                }
            }
        }
    }
    // Join the segments into loops.
    let mut loops: Vec<Vec<Vec2>> = Vec::new();
    let mut done: std::collections::HashSet<usize> = std::collections::HashSet::new();
    let mut starts: Vec<usize> = next.keys().copied().collect();
    starts.sort_unstable();
    for s in starts {
        if done.contains(&s) {
            continue;
        }
        let mut lp = Vec::new();
        let mut cur = s;
        for _ in 0..=next.len() {
            if !done.insert(cur) {
                break;
            }
            if let Some(&(x, y)) = points.get(&cur) {
                lp.push(Vec2::new(x, y));
            }
            match next.get(&cur) {
                Some(&n) => cur = n,
                None => break,
            }
        }
        if cur == s && lp.len() >= 3 {
            loops.push(lp);
        }
    }
    let step_len = hx.max(hy);
    let mut outers = Vec::new();
    let mut holes = Vec::new();
    for lp in loops {
        let lp = simplify(&sharpen(&lp, step_len));
        if lp.len() < 3 {
            continue;
        }
        let flat: Vec<Vec2> = lp.iter().map(|q| at(q.x, q.y)).collect();
        let l = Loop2::polygon(&flat);
        let area = l.signed_area();
        if area.abs() < 1e-10 {
            continue;
        }
        // The frame is right-handed in the flat (e2 = e1 turned a quarter left) for a panel; a
        // bend's (d, n) may be either way round.
        let ccw_in_frame = area * (piece.e1.x * piece.e2.y - piece.e1.y * piece.e2.x) > 0.0;
        if ccw_in_frame {
            outers.push(Region2 { outer: l.ccw(), holes: Vec::new() });
        } else {
            holes.push(Region2 { outer: l.ccw(), holes: Vec::new() });
        }
    }
    if outers.is_empty() {
        return Ok(Vec::new());
    }
    let mut regions = material_union(outers, work)?;
    if !holes.is_empty() {
        let holes = material_union(holes, work)?;
        regions = material_boolean(&regions, &holes, kernel::BoolOp::Cut, work)?;
    }
    Ok(regions)
}

/// The corners a grid square cut off, put back: where a loop turns sharply between two
/// crossings, and the straight runs before and after them, carried on, meet within a grid step
/// or so of both, that meeting point goes in between.
fn sharpen(lp: &[Vec2], step: f64) -> Vec<Vec2> {
    let n = lp.len();
    if n < 5 {
        return lp.to_vec();
    }
    let at = |k: isize| lp[k.rem_euclid(n as isize) as usize];
    let mut out = Vec::with_capacity(n + 8);
    for k in 0..n as isize {
        let (a0, a1, b0, b1) = (at(k - 1), at(k), at(k + 1), at(k + 2));
        out.push(a1);
        let (da, db) = (a1 - a0, b1 - b0);
        let (la, lb) = (da.len(), db.len());
        if la < 1e-12 || lb < 1e-12 {
            continue;
        }
        let (ua, ub) = (da * (1.0 / la), db * (1.0 / lb));
        // A turn of more than about 20° between the runs before and after.
        if ua.dot(ub) > 0.94 {
            continue;
        }
        // The runs must be straight: the point before a0 lies on a0–a1 carried back, the one
        // after b1 on b0–b1 carried on.
        let (pa, pb) = (at(k - 2), at(k + 3));
        let off = |p: Vec2, o: Vec2, u: Vec2| ((p - o).x * u.y - (p - o).y * u.x).abs();
        if off(pa, a1, ua) > 1e-6 * (1.0 + step) || off(pb, b0, ub) > 1e-6 * (1.0 + step) {
            continue;
        }
        let den = ua.x * ub.y - ua.y * ub.x;
        if den.abs() < 1e-9 {
            continue;
        }
        let w = b0 - a1;
        let t = (w.x * ub.y - w.y * ub.x) / den;
        let x = a1 + ua * t;
        if t > 0.0 && t < 2.0 * step && x.dist(b0) < 2.0 * step {
            out.push(x);
        }
    }
    out
}

/// The loop without the points in the middle of straight runs.
fn simplify(lp: &[Vec2]) -> Vec<Vec2> {
    let n = lp.len();
    if n < 4 {
        return lp.to_vec();
    }
    let mut out: Vec<Vec2> = Vec::with_capacity(n);
    for k in 0..n {
        let p = lp[(k + n - 1) % n];
        let c = lp[k];
        let q = lp[(k + 1) % n];
        let (u, w) = (c - p, q - p);
        let cross = u.x * w.y - u.y * w.x;
        let len = w.len();
        if len > 1e-12 && (cross / len).abs() < 1e-9 && u.dot(w) > 0.0 && u.len() < len {
            continue;
        }
        if out.last().is_some_and(|l: &Vec2| l.dist(c) < 1e-12) {
            continue;
        }
        out.push(c);
    }
    out
}
