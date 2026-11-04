//! The occupancy grids the nesting places parts on: a sheet as rows of bits, and a part (turned
//! to one angle) as the cells it takes, spans per row.
//!
//! A part's cells are every cell that comes within `reach` (half the spacing) of its outline or
//! lies inside it, measured exactly on the cell's square. Two parts whose cells don't meet are
//! therefore at least twice `reach` apart: were they closer, the cell holding the point halfway
//! between them would be within `reach` of both. The sheet's grid is inset so that a part whose
//! cells lie on it keeps the margin from the sheet's edges.

use solidcraft_geom::Vec2;

/// A sheet's cells: `w` by `h`, a bit per cell, row by row.
#[derive(Clone, Debug)]
pub struct Grid {
    pub w: usize,
    pub h: usize,
    words: usize,
    bits: Vec<u64>,
}

impl Grid {
    pub fn new(w: usize, h: usize) -> Grid {
        let words = w.div_ceil(64);
        Grid { w, h, words, bits: vec![0; words.saturating_mul(h)] }
    }

    fn row(&self, y: usize) -> &[u64] {
        let start = y.saturating_mul(self.words);
        self.bits.get(start..start.saturating_add(self.words)).unwrap_or(&[])
    }

    /// The last taken cell of row `y` in `a..b`, if any.
    pub fn last_set(&self, y: usize, a: usize, b: usize) -> Option<usize> {
        let b = b.min(self.w);
        if a >= b {
            return None;
        }
        let row = self.row(y);
        let (first, last) = (a / 64, (b - 1) / 64);
        for wi in (first..=last).rev() {
            let mut word = row.get(wi).copied().unwrap_or(0);
            if wi == last {
                let keep = (b - 1) % 64 + 1;
                if keep < 64 {
                    word &= (1u64 << keep) - 1;
                }
            }
            if wi == first {
                word &= !0u64 << (a % 64);
            }
            if word != 0 {
                return Some(wi * 64 + 63 - word.leading_zeros() as usize);
            }
        }
        None
    }

    /// Takes the cells `a..b` of row `y`.
    pub fn set_span(&mut self, y: usize, a: usize, b: usize) {
        let b = b.min(self.w);
        if y >= self.h || a >= b {
            return;
        }
        let start = y.saturating_mul(self.words);
        for x in a..b {
            if let Some(word) = self.bits.get_mut(start + x / 64) {
                *word |= 1u64 << (x % 64);
            }
        }
    }

    /// How many cells are taken.
    pub fn taken(&self) -> usize {
        self.bits.iter().map(|w| w.count_ones() as usize).sum()
    }
}

/// The cells a part takes at one angle: `w` by `h` cells, spans `[a, b)` per row, and where the
/// mask's cell (0, 0) lies against the part's centre (mm): its lower-left corner is the centre
/// plus `origin`.
#[derive(Clone, Debug)]
pub struct Mask {
    pub w: usize,
    pub h: usize,
    pub rows: Vec<Vec<(usize, usize)>>,
    pub origin: Vec2,
}

/// The distance from the axis-aligned square of half size `half` at `c` to the segment `p`–`q`.
fn cell_segment_distance(c: Vec2, half: f64, p: Vec2, q: Vec2) -> f64 {
    let (a, b) = (p - c, q - c);
    let d = b - a;
    // Does the segment enter the square? (Liang–Barsky.)
    let (mut t0, mut t1) = (0.0f64, 1.0f64);
    let mut meets = true;
    for (pp, qq) in [(-d.x, a.x + half), (d.x, half - a.x), (-d.y, a.y + half), (d.y, half - a.y)] {
        if pp == 0.0 {
            if qq < 0.0 {
                meets = false;
                break;
            }
        } else {
            let r = qq / pp;
            if pp < 0.0 {
                t0 = t0.max(r);
            } else {
                t1 = t1.min(r);
            }
            if t0 > t1 {
                meets = false;
                break;
            }
        }
    }
    if meets {
        return 0.0;
    }
    let to_square = |v: Vec2| ((v.x.abs() - half).max(0.0)).hypot((v.y.abs() - half).max(0.0));
    let to_segment = |v: Vec2| {
        let l = d.dot(d);
        let t = if l > 0.0 { ((v - a).dot(d) / l).clamp(0.0, 1.0) } else { 0.0 };
        v.dist(a + d * t)
    };
    let corners = [Vec2::new(-half, -half), Vec2::new(half, -half), Vec2::new(half, half), Vec2::new(-half, half)];
    corners.iter().map(|&k| to_segment(k)).fold(to_square(a).min(to_square(b)), f64::min)
}

/// The most cells a part's mask may have (a part much larger than its sheet's cells).
pub const MAX_MASK_CELLS: usize = 4_000_000;

/// The mask of the outlines `polys` (closed polygons around the part's centre, already turned),
/// on cells of `cell` mm whose lattice puts a cell corner at `anchor` (relative to the centre):
/// every cell inside an outline or within `reach` of one. `None` for an empty or too large part.
pub fn mask(polys: &[Vec<Vec2>], cell: f64, reach: f64, anchor: Vec2) -> Option<Mask> {
    if !(cell > 0.0) || !(reach >= 0.0) {
        return None;
    }
    let pts = polys.iter().flatten();
    let (mut lo, mut hi) = (Vec2::new(f64::MAX, f64::MAX), Vec2::new(f64::MIN, f64::MIN));
    for p in pts {
        lo = Vec2::new(lo.x.min(p.x), lo.y.min(p.y));
        hi = Vec2::new(hi.x.max(p.x), hi.y.max(p.y));
    }
    if !(lo.x <= hi.x && lo.y <= hi.y) || !lo.is_finite() || !hi.is_finite() {
        return None;
    }
    // The mask's corner on the lattice, a cell beyond the reach.
    let pad = reach + cell;
    let snap = |v: f64, a: f64| a + ((v - a) / cell).floor() * cell;
    let origin = Vec2::new(snap(lo.x - pad, anchor.x), snap(lo.y - pad, anchor.y));
    let w = ((hi.x + pad - origin.x) / cell).ceil();
    let h = ((hi.y + pad - origin.y) / cell).ceil();
    if !(w >= 1.0 && h >= 1.0) || w * h > MAX_MASK_CELLS as f64 {
        return None;
    }
    let (w, h) = (w as usize, h as usize);
    let mut on = vec![false; w * h];
    let centre = |i: usize, j: usize| Vec2::new(origin.x + (i as f64 + 0.5) * cell, origin.y + (j as f64 + 0.5) * cell);
    // Inside: each outline's own crossings at each row's centre line, filled between pairs.
    for poly in polys {
        for j in 0..h {
            let y = origin.y + (j as f64 + 0.5) * cell;
            let mut xs: Vec<f64> = Vec::new();
            for (k, a) in poly.iter().enumerate() {
                let Some(b) = poly.get((k + 1) % poly.len()) else { continue };
                if (a.y <= y) != (b.y <= y) {
                    xs.push(a.x + (y - a.y) / (b.y - a.y) * (b.x - a.x));
                }
            }
            xs.sort_by(f64::total_cmp);
            for pair in xs.chunks(2) {
                let [x0, x1] = pair else { continue };
                let i0 = ((x0 - origin.x) / cell - 0.5).ceil().max(0.0) as usize;
                let i1 = ((x1 - origin.x) / cell - 0.5).floor();
                if i1 < 0.0 {
                    continue;
                }
                for i in i0..=(i1 as usize).min(w - 1) {
                    if let Some(c) = on.get_mut(j * w + i) {
                        *c = true;
                    }
                }
            }
        }
    }
    // Near the outline: each segment's cells within reach, walked along it.
    let half = cell / 2.0;
    let r = (reach / cell).ceil() as isize + 1;
    for poly in polys {
        for (k, a) in poly.iter().enumerate() {
            let Some(b) = poly.get((k + 1) % poly.len()).copied() else { continue };
            let len = a.dist(b);
            let steps = (len / cell).ceil().max(1.0) as usize;
            let mut last: Option<(isize, isize)> = None;
            for s in 0..=steps {
                let p = *a + (b - *a) * (s as f64 / steps as f64);
                let (ci, cj) = (((p.x - origin.x) / cell).floor() as isize, ((p.y - origin.y) / cell).floor() as isize);
                if last == Some((ci, cj)) {
                    continue;
                }
                last = Some((ci, cj));
                for dj in -r..=r {
                    for di in -r..=r {
                        let (i, j) = (ci + di, cj + dj);
                        if i < 0 || j < 0 || i as usize >= w || j as usize >= h {
                            continue;
                        }
                        let (i, j) = (i as usize, j as usize);
                        let Some(c) = on.get_mut(j * w + i) else { continue };
                        if !*c && cell_segment_distance(centre(i, j), half, *a, b) <= reach {
                            *c = true;
                        }
                    }
                }
            }
        }
    }
    let rows = (0..h)
        .map(|j| {
            let mut spans = Vec::new();
            let mut start = None;
            for i in 0..=w {
                let set = i < w && on.get(j * w + i).copied().unwrap_or(false);
                match (set, start) {
                    (true, None) => start = Some(i),
                    (false, Some(s)) => {
                        spans.push((s, i));
                        start = None;
                    }
                    _ => {}
                }
            }
            spans
        })
        .collect();
    Some(Mask { w, h, rows, origin })
}

/// The lowest, then leftmost, cell where `mask` fits on `grid` (its cell (0, 0) there), counting
/// the work in `budget` (rows checked). `None`: it fits nowhere, or the budget ran out.
pub fn find(grid: &Grid, mask: &Mask, budget: &mut u64) -> Option<(usize, usize)> {
    if mask.w > grid.w || mask.h > grid.h {
        return None;
    }
    for y in 0..=(grid.h - mask.h) {
        let mut x = 0usize;
        while x + mask.w <= grid.w {
            if *budget == 0 {
                return None;
            }
            *budget = budget.saturating_sub(mask.h as u64);
            let mut next = x;
            for (j, spans) in mask.rows.iter().enumerate() {
                for &(a, b) in spans {
                    if let Some(l) = grid.last_set(y + j, x + a, x + b) {
                        next = next.max(l + 1 - a);
                    }
                }
            }
            if next == x {
                return Some((x, y));
            }
            x = next;
        }
    }
    None
}

/// Takes `mask`'s cells on `grid` with its cell (0, 0) at (`x`, `y`) (cells off the grid are
/// left out).
pub fn put(grid: &mut Grid, mask: &Mask, x: isize, y: isize) {
    for (j, spans) in mask.rows.iter().enumerate() {
        let gy = y + j as isize;
        if gy < 0 {
            continue;
        }
        for &(a, b) in spans {
            let (ga, gb) = (x + a as isize, x + b as isize);
            if gb <= 0 {
                continue;
            }
            grid.set_span(gy as usize, ga.max(0) as usize, gb as usize);
        }
    }
}
