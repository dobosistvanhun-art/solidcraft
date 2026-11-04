//! Nesting: the design's parts laid out on material sheets so that as little material as can be
//! is left over. A part is a sketch's closed contours or a sheet metal body's flat pattern, taken
//! as they are (outer contours, holes, arcs); the layout keeps its real outline apart from every
//! other by the job's spacing and inside the sheet's margin.
//!
//! The automatic layout places copies one at a time, each at the lowest, then leftmost, place it
//! fits on the first sheet it fits on, trying it at every allowed angle (cells: `raster`). It does
//! that for several orders of the copies and keeps the best layouts. Every layout, automatic or
//! moved by hand, is checked again on the real outlines (`collisions`, `outside`).

pub mod raster;

use std::collections::HashMap;

use solidcraft_doc::ModelState;
use solidcraft_doc::nesting::{NestJob, NestLayout, NestPart, PartSource, Placement};
use solidcraft_geom::{Loop2, Region2, Similarity2, Vec2};

use raster::{Grid, Mask};

/// How closely arcs are followed as polygons (mm). The check allows for it.
const TOL: f64 = 0.02;
/// The most copies a job places.
pub const MAX_COPIES: usize = 5_000;
/// The most sheets a layout uses.
pub const MAX_SHEETS: usize = 500;
/// The most cells a sheet's grid has.
const MAX_GRID_CELLS: f64 = 3_000_000.0;

/// A part as the nesting uses it: its material around its own centre (the middle of its outer
/// contours' bounds), its outer outlines as polygons for collisions, its area and its cut length.
#[derive(Clone, Debug)]
pub struct Shape {
    pub regions: Vec<Region2>,
    /// Holes that are no loops of a region (a sheet's perforations), cut as inner contours.
    pub extra_holes: Vec<Loop2>,
    pub outlines: Vec<Vec<Vec2>>,
    pub area: f64,
    pub cut_length: f64,
    /// Half its bounds' size (mm).
    pub half: Vec2,
}

fn loop_key(ids: &[String]) -> Vec<String> {
    let mut k = ids.to_vec();
    k.sort();
    k
}

fn shifted(l: &Loop2, by: Vec2) -> Loop2 {
    match Similarity2::new(1.0, 0.0, by) {
        Some(m) => Loop2 { segs: l.segs.iter().map(|s| s.similar(&m)).collect() },
        None => l.clone(),
    }
}

/// The part's material: a sketch's regions (each outer contour with its holes; the pieces a hole
/// cuts out left out), or a sheet's flat pattern with its perforations.
pub fn shape(st: &ModelState, part: &NestPart) -> Result<Shape, String> {
    let (regions, extra): (Vec<Region2>, Vec<Loop2>) = match &part.source {
        PartSource::Sketch(id) => {
            let ss = st.sketch(*id).ok_or_else(|| format!("sketch {id} is not in the design"))?;
            let holes: std::collections::HashSet<Vec<String>> = ss.profiles.iter().flat_map(|p| p.hole_curves.iter().map(|h| loop_key(h))).collect();
            let regions: Vec<Region2> =
                ss.profiles.iter().filter(|p| !holes.contains(&loop_key(&p.outer_curves))).map(|p| p.region.clone()).collect();
            (regions, Vec::new())
        }
        PartSource::SheetBody(name) => {
            let sh = st.sheets.iter().find(|s| &s.body == name).ok_or_else(|| format!("no sheet metal body `{name}`"))?;
            let regions = sh.flat_regions().map_err(|e| format!("{name}: {e}"))?;
            let mut extra = Vec::new();
            for p in &sh.perforations {
                for c in sh.perforation_holes(p).unwrap_or_default() {
                    extra.push(match p.hole_corners(c) {
                        Some(k) => Loop2::polygon(&k),
                        None => Loop2::circle(c, p.diameter / 2.0),
                    });
                }
            }
            (regions, extra)
        }
    };
    if regions.is_empty() {
        return Err("it has no closed contour".into());
    }
    let outlines0: Vec<Vec<Vec2>> = regions.iter().map(|r| r.outer.polyline(TOL).into_vec()).filter(|p| p.len() >= 3).collect();
    let (mut lo, mut hi) = (Vec2::new(f64::MAX, f64::MAX), Vec2::new(f64::MIN, f64::MIN));
    for p in outlines0.iter().flatten() {
        lo = Vec2::new(lo.x.min(p.x), lo.y.min(p.y));
        hi = Vec2::new(hi.x.max(p.x), hi.y.max(p.y));
    }
    if !(lo.x < hi.x && lo.y < hi.y) || !lo.is_finite() || !hi.is_finite() {
        return Err("its contours have no area".into());
    }
    let c = (lo + hi) * 0.5;
    let regions: Vec<Region2> =
        regions.iter().map(|r| Region2 { outer: shifted(&r.outer, -c), holes: r.holes.iter().map(|h| shifted(h, -c)).collect() }).collect();
    let extra_holes: Vec<Loop2> = extra.iter().map(|h| shifted(h, -c)).collect();
    let outlines = outlines0.iter().map(|p| p.iter().map(|q| *q - c).collect()).collect();
    let length = |l: &Loop2| l.segs.iter().map(|s| s.length()).sum::<f64>();
    let cut_length =
        regions.iter().map(|r| length(&r.outer) + r.holes.iter().map(length).sum::<f64>()).sum::<f64>() + extra_holes.iter().map(length).sum::<f64>();
    let area = regions.iter().map(Region2::area).sum::<f64>() - extra_holes.iter().map(|h| h.signed_area().abs()).sum::<f64>();
    Ok(Shape { regions, extra_holes, outlines, area, cut_length, half: (hi - lo) * 0.5 })
}

fn rotate(p: Vec2, deg: f64) -> Vec2 {
    let (s, c) = deg.to_radians().sin_cos();
    Vec2::new(c * p.x - s * p.y, s * p.x + c * p.y)
}

/// A shape's outlines turned by `deg` (about its centre).
fn turned(shape: &Shape, deg: f64) -> Vec<Vec<Vec2>> {
    shape.outlines.iter().map(|p| p.iter().map(|q| rotate(*q, deg)).collect()).collect()
}

/// A placed copy's outlines on its sheet.
pub fn placed_outlines(shape: &Shape, p: &Placement) -> Vec<Vec<Vec2>> {
    let at = Vec2::new(p.x, p.y);
    turned(shape, p.angle).into_iter().map(|poly| poly.into_iter().map(|q| q + at).collect()).collect()
}

/// The transform that puts a part's material where a copy lies.
pub fn placement_transform(p: &Placement) -> Option<Similarity2> {
    Similarity2::new(1.0, p.angle.to_radians(), Vec2::new(p.x, p.y))
}

/// The angles a part may take (degrees) at an angle step.
fn angles(part: &NestPart, step: f64) -> Vec<f64> {
    if part.fixed_angle {
        return vec![0.0];
    }
    if part.grain {
        return vec![0.0, 180.0];
    }
    let n = (360.0 / step.clamp(0.5, 90.0)).round().max(1.0) as usize;
    (0..n).map(|i| i as f64 * 360.0 / n as f64).collect()
}

/// The cell size for a sheet: about 800 cells across its longer side at Normal (500 Fast, 1200
/// Precise), never finer than 0.25 mm, and coarse enough to keep the grid bounded.
fn cell_size(job: &NestJob) -> f64 {
    let across = match job.quality {
        solidcraft_doc::nesting::NestQuality::Fast => 500.0,
        solidcraft_doc::nesting::NestQuality::Normal => 800.0,
        solidcraft_doc::nesting::NestQuality::Precise => 1200.0,
    };
    let (w, h) = (job.sheet.width, job.sheet.height);
    let by_size = (w.max(h) / across).max(0.25);
    let by_cells = ((w * h) / MAX_GRID_CELLS).sqrt();
    by_size.max(by_cells)
}

/// The grid a sheet's parts are placed on: its cells, size and where its corner lies on the sheet.
struct Frame {
    cell: f64,
    reach: f64,
    origin: Vec2,
    w: usize,
    h: usize,
}

fn frame(job: &NestJob) -> Result<Frame, String> {
    let (w, h) = (job.sheet.width, job.sheet.height);
    if !(w > 0.0 && h > 0.0 && w.is_finite() && h.is_finite()) || w > 100_000.0 || h > 100_000.0 {
        return Err("the sheet needs a width and a height above 0 (at most 100 m)".into());
    }
    if !(job.sheet.margin >= 0.0) || !(job.spacing() >= 0.0) || !job.spacing().is_finite() || !job.sheet.margin.is_finite() {
        return Err("the margin, the gap and the cut width must be 0 or more".into());
    }
    let cell = cell_size(job);
    // Each part takes the cells within half the spacing of it (and of the arcs' polygons).
    let reach = job.spacing() / 2.0 + TOL;
    // A part's cells reach half the spacing past it; the grid's edge is that far past the margin
    // (beyond the sheet when the margin is smaller), so a part keeps the margin, not the spacing,
    // from the sheet's edge.
    let inset = job.sheet.margin - reach;
    let (gw, gh) = (((w - 2.0 * inset) / cell).floor(), ((h - 2.0 * inset) / cell).floor());
    if !(gw >= 1.0 && gh >= 1.0) {
        return Err("the margins leave no room on the sheet".into());
    }
    Ok(Frame { cell, reach, origin: Vec2::new(inset, inset), w: gw as usize, h: gh as usize })
}

/// What a run found: layouts, best first, and why copies could not be placed.
pub struct Found {
    pub layouts: Vec<NestLayout>,
}

/// A layout's score, lower better: copies left over, sheets, how high the last sheet is filled.
type Score = (usize, usize, usize, usize);

struct Attempt {
    layout: NestLayout,
    score: Score,
}

/// Lays the job's copies out on sheets, keeping its locked placements where they are, trying
/// several orders, and gives back the best layouts (at most three).
pub fn run(job: &NestJob, shapes: &[Shape]) -> Result<Found, String> {
    let f = frame(job)?;
    let step = job.quality.angle_step_deg();
    let orders = match job.quality {
        solidcraft_doc::nesting::NestQuality::Fast => 3,
        solidcraft_doc::nesting::NestQuality::Normal => 6,
        solidcraft_doc::nesting::NestQuality::Precise => 10,
    };
    // The copies to place: each part's count, less its locked copies.
    let locked: Vec<Placement> = job.layout.placements.iter().filter(|p| p.locked && p.part < shapes.len()).cloned().collect();
    let mut copies: Vec<usize> = Vec::new();
    for (i, part) in job.parts.iter().enumerate() {
        let kept = locked.iter().filter(|p| p.part == i).count();
        for _ in kept..part.count as usize {
            copies.push(i);
            if copies.len() > MAX_COPIES {
                return Err(format!("at most {MAX_COPIES} copies in one job"));
            }
        }
    }
    let mut masks: HashMap<(usize, i64), Option<Mask>> = HashMap::new();
    let area = |i: usize| shapes.get(i).map_or(0.0, |s| s.area);
    let size = |i: usize| shapes.get(i).map_or(Vec2::ZERO, |s| s.half);
    let mut order_lists: Vec<Vec<usize>> = Vec::new();
    let sorted = |key: &dyn Fn(usize) -> f64| {
        let mut v = copies.clone();
        v.sort_by(|a, b| key(*b).total_cmp(&key(*a)).then(a.cmp(b)));
        v
    };
    order_lists.push(sorted(&area));
    order_lists.push(sorted(&|i| size(i).y));
    order_lists.push(sorted(&|i| size(i).x));
    order_lists.push(sorted(&|i| size(i).x.max(size(i).y)));
    // Shuffles of the area order: neighbours swapped at random (a fixed seed, so a run repeats).
    let mut seed: u64 = 0x9E37_79B9_7F4A_7C15;
    while order_lists.len() < orders {
        let mut v = order_lists.first().cloned().unwrap_or_default();
        for k in 1..v.len() {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            if seed.is_multiple_of(3) {
                v.swap(k - 1, k);
            }
        }
        order_lists.push(v);
    }
    order_lists.truncate(orders);
    // The work each attempt may take (rows checked), and all of them together.
    let (per_attempt, total) = (4_000_000_000u64, 6_000_000_000u64);
    let mut spent = 0u64;
    let mut attempts: Vec<Attempt> = Vec::new();
    for (n, order) in order_lists.iter().enumerate() {
        if n > 0 && spent >= total {
            break;
        }
        let mut budget = per_attempt;
        let Some(a) = attempt(job, shapes, &f, step, order, &locked, &mut masks, &mut budget) else {
            spent = spent.saturating_add(per_attempt);
            continue;
        };
        spent = spent.saturating_add(per_attempt - budget);
        if !attempts.iter().any(|b| b.score == a.score) {
            attempts.push(a);
        }
    }
    attempts.sort_by_key(|a| a.score);
    attempts.truncate(3);
    if attempts.is_empty() {
        return Err("the nesting ran out of time; try Fast, or fewer copies".into());
    }
    Ok(Found { layouts: attempts.into_iter().map(|a| a.layout).collect() })
}

fn angle_key(deg: f64) -> i64 {
    (deg * 1000.0).round() as i64
}

/// One order laid out. `None` when the budget ran out.
fn attempt(
    job: &NestJob,
    shapes: &[Shape],
    f: &Frame,
    step: f64,
    order: &[usize],
    locked: &[Placement],
    masks: &mut HashMap<(usize, i64), Option<Mask>>,
    budget: &mut u64,
) -> Option<Attempt> {
    let mut grids: Vec<Grid> = Vec::new();
    let mut tops: Vec<usize> = Vec::new();
    let mut placements: Vec<Placement> = locked.to_vec();
    // Locked copies first, on their own sheets, their cells on the grid's lattice.
    for p in locked {
        while grids.len() <= p.sheet && grids.len() < MAX_SHEETS {
            grids.push(Grid::new(f.w, f.h));
            tops.push(0);
        }
        let Some(shape) = shapes.get(p.part) else { continue };
        let anchor = f.origin - Vec2::new(p.x, p.y);
        if let (Some(m), Some(g)) = (raster::mask(&turned(shape, p.angle), f.cell, f.reach, anchor), grids.get_mut(p.sheet)) {
            let corner = Vec2::new(p.x, p.y) + m.origin - f.origin;
            let (x, y) = ((corner.x / f.cell).round() as isize, (corner.y / f.cell).round() as isize);
            raster::put(g, &m, x, y);
            if let Some(t) = tops.get_mut(p.sheet) {
                *t = (*t).max((y.max(0) as usize).saturating_add(m.h));
            }
        }
    }
    let mut unplaced = Vec::new();
    for &i in order {
        let (Some(part), Some(shape)) = (job.parts.get(i), shapes.get(i)) else { continue };
        let options = angles(part, step);
        let mut done = false;
        let mut sheet = 0;
        while !done {
            if sheet == grids.len() {
                if grids.len() >= MAX_SHEETS {
                    break;
                }
                grids.push(Grid::new(f.w, f.h));
                tops.push(0);
            }
            let fresh = grids.get(sheet).is_some_and(|g| g.taken() == 0);
            let mut best: Option<((usize, usize), (usize, usize), f64)> = None;
            for &deg in &options {
                let m = masks.entry((i, angle_key(deg))).or_insert_with(|| raster::mask(&turned(shape, deg), f.cell, f.reach, Vec2::ZERO));
                let Some(m) = m.as_ref() else { continue };
                let Some(g) = grids.get(sheet) else { break };
                if let Some((x, y)) = raster::find(g, m, budget) {
                    let score = (y + m.h, x);
                    if best.as_ref().is_none_or(|(s, ..)| score < *s) {
                        best = Some((score, (x, y), deg));
                    }
                }
                if *budget == 0 {
                    return None;
                }
            }
            if let Some((_, (x, y), deg)) = best {
                if let (Some(m), Some(g)) = (masks.get(&(i, angle_key(deg))).and_then(Option::as_ref), grids.get_mut(sheet)) {
                    raster::put(g, m, x as isize, y as isize);
                    if let Some(t) = tops.get_mut(sheet) {
                        *t = (*t).max(y + m.h);
                    }
                    let centre = f.origin + Vec2::new(x as f64 * f.cell, y as f64 * f.cell) - m.origin;
                    placements.push(Placement { part: i, sheet, x: centre.x, y: centre.y, angle: deg, locked: false });
                    done = true;
                }
            } else if fresh {
                // It doesn't fit even on an empty sheet.
                break;
            } else {
                sheet += 1;
            }
        }
        if !done {
            unplaced.push(i);
        }
    }
    // Empty sheets at the end (a part too big for any) are not counted.
    let used = placements.iter().map(|p| p.sheet + 1).max().unwrap_or(0);
    let last_top = used.checked_sub(1).and_then(|s| tops.get(s)).copied().unwrap_or(0);
    let all_tops: usize = tops.iter().take(used).sum();
    Some(Attempt { score: (unplaced.len(), used, last_top, all_tops), layout: NestLayout { placements, unplaced } })
}

/// The least distance between two closed polygons' edges.
fn polygon_distance(a: &[Vec2], b: &[Vec2]) -> f64 {
    let mut best = f64::INFINITY;
    for (i, p) in a.iter().enumerate() {
        let Some(q) = a.get((i + 1) % a.len()) else { continue };
        for (k, r) in b.iter().enumerate() {
            let Some(s) = b.get((k + 1) % b.len()) else { continue };
            best = best.min(segment_distance(*p, *q, *r, *s));
        }
    }
    best
}

fn point_segment(p: Vec2, a: Vec2, b: Vec2) -> f64 {
    let d = b - a;
    let l = d.dot(d);
    let t = if l > 0.0 { ((p - a).dot(d) / l).clamp(0.0, 1.0) } else { 0.0 };
    p.dist(a + d * t)
}

fn segment_distance(a: Vec2, b: Vec2, c: Vec2, d: Vec2) -> f64 {
    let cross = |o: Vec2, p: Vec2, q: Vec2| (p - o).cross(q - o);
    let (d1, d2, d3, d4) = (cross(c, d, a), cross(c, d, b), cross(a, b, c), cross(a, b, d));
    if ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0)) && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0)) {
        return 0.0;
    }
    point_segment(a, c, d).min(point_segment(b, c, d)).min(point_segment(c, a, b)).min(point_segment(d, a, b))
}

fn inside(p: Vec2, poly: &[Vec2]) -> bool {
    let mut inside = false;
    for (i, a) in poly.iter().enumerate() {
        let Some(b) = poly.get((i + 1) % poly.len()) else { continue };
        if (a.y > p.y) != (b.y > p.y) && p.x < a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x) {
            inside = !inside;
        }
    }
    inside
}

fn bounds(polys: &[Vec<Vec2>]) -> (Vec2, Vec2) {
    let (mut lo, mut hi) = (Vec2::new(f64::MAX, f64::MAX), Vec2::new(f64::MIN, f64::MIN));
    for p in polys.iter().flatten() {
        lo = Vec2::new(lo.x.min(p.x), lo.y.min(p.y));
        hi = Vec2::new(hi.x.max(p.x), hi.y.max(p.y));
    }
    (lo, hi)
}

/// Placements (by index) closer to each other than the job's spacing, or overlapping, on their
/// real outlines.
pub fn collisions(job: &NestJob, shapes: &[Shape], layout: &NestLayout) -> Vec<(usize, usize)> {
    let need = job.spacing() - 2.0 * TOL - 1e-6;
    let placed: Vec<Option<(usize, Vec<Vec<Vec2>>, (Vec2, Vec2))>> = layout
        .placements
        .iter()
        .map(|p| {
            let o = placed_outlines(shapes.get(p.part)?, p);
            let b = bounds(&o);
            Some((p.sheet, o, b))
        })
        .collect();
    let mut out = Vec::new();
    for (i, a) in placed.iter().enumerate() {
        let Some((sa, oa, (la, ha))) = a else { continue };
        for (k, b) in placed.iter().enumerate().skip(i + 1) {
            let Some((sb, ob, (lb, hb))) = b else { continue };
            if sa != sb || la.x > hb.x + need || lb.x > ha.x + need || la.y > hb.y + need || lb.y > ha.y + need {
                continue;
            }
            let near = oa.iter().any(|p| ob.iter().any(|q| polygon_distance(p, q) < need.max(1e-9)));
            let within = oa.iter().any(|p| ob.iter().any(|q| p.first().is_some_and(|v| inside(*v, q)) || q.first().is_some_and(|v| inside(*v, p))));
            if near || within {
                out.push((i, k));
            }
        }
    }
    out
}

/// Placements (by index) that reach past the sheet's margin.
pub fn outside(job: &NestJob, shapes: &[Shape], layout: &NestLayout) -> Vec<usize> {
    let m = job.sheet.margin - TOL - 1e-6;
    let (w, h) = (job.sheet.width, job.sheet.height);
    layout
        .placements
        .iter()
        .enumerate()
        .filter(|(_, p)| {
            shapes.get(p.part).is_none_or(|s| {
                let (lo, hi) = bounds(&placed_outlines(s, p));
                lo.x < m || lo.y < m || hi.x > w - m || hi.y > h - m
            })
        })
        .map(|(i, _)| i)
        .collect()
}

/// What a layout uses and leaves: sheets, their area, the parts' area, how much of the sheets
/// it is (%), the rest, and the cut length (mm).
pub fn stats(job: &NestJob, shapes: &[Shape], layout: &NestLayout) -> serde_json::Value {
    let sheets = layout.placements.iter().map(|p| p.sheet + 1).max().unwrap_or(0);
    let sheet_area = sheets as f64 * job.sheet.width * job.sheet.height;
    let parts_area: f64 = layout.placements.iter().filter_map(|p| shapes.get(p.part)).map(|s| s.area).sum();
    let cut: f64 = layout.placements.iter().filter_map(|p| shapes.get(p.part)).map(|s| s.cut_length).sum();
    let used = if sheet_area > 0.0 { parts_area / sheet_area * 100.0 } else { 0.0 };
    let round = |v: f64| (v * 1000.0).round() / 1000.0;
    serde_json::json!({
        "sheets": sheets,
        "placed": layout.placements.len(),
        "unplaced": layout.unplaced.len(),
        "sheet_area_mm2": round(sheet_area),
        "parts_area_mm2": round(parts_area),
        "waste_mm2": round(sheet_area - parts_area),
        "utilisation_percent": round(used),
        "cut_length_mm": round(cut),
    })
}

#[cfg(test)]
#[path = "nest_tests.rs"]
mod tests;
