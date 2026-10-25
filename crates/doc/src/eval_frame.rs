//! Frame members (#4003): one body per straight line of a frame sketch, its cross-section a
//! profile laid square to the line (`frame::member_axes`) and run from one end to the other.

use std::borrow::Cow;
use std::collections::{BTreeMap, HashSet};

use solidcraft_geom::{Aabb2, Plane, Region2, Ring, Vec2, Vec3};
use solidcraft_kernel as kernel;
use solidcraft_sketch::CurveKind;

use super::{ModelBody, ModelState, SolvedSketch, new_name, path_in_frame, select_profiles, sketch_in_feature_frame, val};
use crate::expr::{Kind, Value};
use crate::frame::{self, MAX_MEMBERS, MemberProfile};
use crate::{DocError, Document, Feature, FeatureKind, ProfileSel, Result};

/// Most profiles a profile sketch may have for its material to be worked out.
const MAX_PROFILE_PIECES: usize = 256;
/// Shortest member (mm).
const MIN_LENGTH: f64 = 1e-6;
/// How far (mm) a 3D sketch curve may stray from the line between its ends and still be a line.
const STRAIGHT_TOL: f64 = 1e-3;

/// Evaluate a `FrameMember` feature: one new body per line, each named
/// ([`member_names`], or by position for a copy whose `body_names` were set).
pub(super) fn evaluate(doc: &Document, vals: &BTreeMap<String, Value>, f: &Feature, st: &mut ModelState) -> Result<()> {
    let FeatureKind::FrameMember { path_sketch, lines, profile, anchor, angle, mirror, body_name, names, corners, butt_flip, joints } = &f.kind
    else {
        return Err(DocError::Invalid("not a frame member feature".into()));
    };
    let ps = st.sketch(path_sketch.0).ok_or_else(|| DocError::Unknown(format!("sketch {path_sketch} (it must come earlier in the timeline)")))?;
    // The frame sketch in this feature's component frame: its plane, and its 3D lines.
    let mapped =
        if ps.sketch.wires.is_empty() { Cow::Owned(sketch_in_feature_frame(doc, ps, f.component)?) } else { path_in_frame(doc, ps, f.component)? };
    let segments = member_lines(mapped.as_ref(), lines)?;
    let (mut regions, description) = profile_regions(vals, st, profile)?;
    if *mirror {
        regions = regions.iter().map(frame::mirrored).collect();
    }
    let at = frame::anchor_point(&regions, *anchor);
    let turn = match angle {
        Some(e) => val(vals, e, Kind::Angle)?,
        None => 0.0,
    };
    let plan = corner_plan(&segments, &regions, at, turn, *corners, *butt_flip, joints)?;
    let mut made = Vec::with_capacity(segments.len());
    for (i, (id, a, b)) in segments.iter().enumerate() {
        let trim = plan.get(i).cloned().unwrap_or_default();
        let mut body = member_body(id, *a, *b, &regions, at, turn, &trim)?;
        // Cut to fit against the members it butts on (their outside: a tube's hollow is no room).
        for &other in &trim.copes {
            let Some((oid, oa, ob)) = segments.get(other) else { continue };
            let otrim = plan.get(other).cloned().unwrap_or_default();
            let outer: Vec<Region2> = regions.iter().map(|r| Region2 { outer: r.outer.clone(), holes: Vec::new() }).collect();
            let envelope = member_body(oid, *oa, *ob, &outer, at, turn, &MemberTrim { copes: Vec::new(), ..otrim })?;
            body = kernel::boolean(&body, &envelope, kernel::BoolOp::Cut)?
                .ok_or_else(|| DocError::Invalid(format!("the member on line `{id}` is used up where it meets line `{oid}`")))?;
        }
        made.push(crate::lineage::numbered_tool(body));
    }
    if !f.body_names.is_empty() {
        // A copy (pasted, or in a copied component) names its members by position, as any
        // feature's bodies.
        for (k, t) in made.into_iter().enumerate() {
            let name = new_name(st, f, k);
            st.bodies.push(ModelBody::traced_new(name, t, f.id));
        }
        return Ok(());
    }
    let base = body_name.as_deref().map(str::trim).filter(|n| !n.is_empty()).map_or(description, str::to_string);
    let ids: Vec<String> = segments.into_iter().map(|(id, _, _)| id).collect();
    for (name, t) in member_names(st, &ids, names, &base).into_iter().zip(made) {
        st.bodies.push(ModelBody::traced_new(name, t, f.id));
    }
    Ok(())
}

/// One end of a member: the plane it ends on (a point of it, and its normal pointing out of the
/// member).
#[derive(Clone, Copy, Debug)]
struct End {
    point: Vec3,
    out: Vec3,
}

/// How one member is cut where it meets others: the planes it ends on (none: square at its line's
/// end), and the members it is cut to fit against (round faces, where no plane fits).
#[derive(Clone, Debug, Default)]
struct MemberTrim {
    start: Option<End>,
    end: Option<End>,
    copes: Vec<usize>,
}

/// Where ends meet (mm).
const JOINT_TOL: f64 = 1e-4;
/// Corners sharper or straighter than this (radians) are left square: a mitre would run out.
const MIN_CORNER: f64 = 0.035;
/// An end plane must meet its member's line at least this steeply (the cosine of its tilt).
const MIN_END_COS: f64 = 0.02;

/// The member along `a`–`b`: the section square to the line, from its start plane to its end
/// plane. Built face by face (planes, the round faces' cylinders, and where an end cuts across
/// them the ellipse arcs exactly): no boolean, so an oblique end costs no more than a square one.
/// A section with other curves (a sketch's splines) is cut by booleans instead.
fn member_body(id: &str, a: Vec3, b: Vec3, regions: &[Region2], at: Vec2, turn: f64, trim: &MemberTrim) -> Result<kernel::Body> {
    use solidcraft_geom::Seg2;
    use solidcraft_kernel::{EdgeSpec, FaceSpec, SurfSpec};
    let (x, y) = frame::member_axes(b - a, turn).ok_or_else(|| DocError::Invalid(format!("line `{id}` has no length")))?;
    let d = (b - a).normalized().ok_or_else(|| DocError::Invalid(format!("line `{id}` has no length")))?;
    let start = trim.start.unwrap_or(End { point: a, out: d * -1.0 });
    let end = trim.end.unwrap_or(End { point: b, out: d });
    for e in [start, end] {
        if e.out.dot(d).abs() < MIN_END_COS {
            return Err(DocError::Invalid(format!("the member on line `{id}` would end along its own length")));
        }
    }
    let section = |q: Vec2| a + x * (q.x - at.x) + y * (q.y - at.y);
    let onto = |p: Vec3, e: End| p + d * (e.out.dot(e.point - p) / e.out.dot(d));
    let exact = regions
        .iter()
        .all(|r| std::iter::once(&r.outer).chain(&r.holes).all(|l| l.segs.iter().all(|g| matches!(g, Seg2::Line { .. } | Seg2::Arc { .. }))));
    if !exact {
        return member_body_cut(id, a, b, regions, at, turn, start, end);
    }
    // The section's pieces: lines, and arcs in pieces of at most a quarter turn (each one conic).
    enum Piece {
        Line(Vec2, Vec2),
        Arc { p: Vec2, q: Vec2, apex: Vec2, mid: Vec2, w: f64, tangent: Vec2 },
    }
    let pieces_of = |l: &solidcraft_geom::Loop2| -> Vec<Piece> {
        let mut out = Vec::new();
        for g in &l.segs {
            match *g {
                Seg2::Line { a, b } => out.push(Piece::Line(a, b)),
                Seg2::Arc { center, radius, start, sweep } => {
                    let n = (sweep.abs() / std::f64::consts::FRAC_PI_2).ceil().clamp(1.0, 8.0) as usize;
                    let step = sweep / n as f64;
                    for k in 0..n {
                        let t0 = start + step * k as f64;
                        let h = step / 2.0;
                        let e = |t: f64| Vec2::new(t.cos(), t.sin());
                        let tangent = Vec2::new(-(t0 + h).sin(), (t0 + h).cos()) * step.signum();
                        out.push(Piece::Arc {
                            p: center + e(t0) * radius,
                            q: center + e(t0 + step) * radius,
                            apex: center + e(t0 + h) * (radius / h.cos()),
                            mid: center + e(t0 + h) * radius,
                            w: h.cos().abs(),
                            tangent,
                        });
                    }
                }
                _ => {}
            }
        }
        out
    };
    let loops: Vec<Vec<Piece>> = regions.iter().flat_map(|r| std::iter::once(&r.outer).chain(&r.holes)).map(pieces_of).collect();
    // How far the faces reach along the line (for the round faces' surfaces).
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for pc in loops.iter().flatten() {
        let pts = match pc {
            Piece::Line(p, q) => [*p, *q, *q],
            Piece::Arc { p, q, apex, .. } => [*p, *q, *apex],
        };
        for v in pts {
            for e in [start, end] {
                let t = (onto(section(v), e) - a).dot(d);
                lo = lo.min(t);
                hi = hi.max(t);
            }
        }
    }
    if !(lo.is_finite() && hi.is_finite() && hi > lo) {
        return Err(DocError::Invalid(format!("the member on line `{id}` has no length between its ends")));
    }
    let edge = |pc: &Piece, e: End| -> EdgeSpec {
        match pc {
            Piece::Line(p, q) => EdgeSpec::Line { a: onto(section(*p), e), b: onto(section(*q), e) },
            Piece::Arc { p, q, apex, w, .. } => {
                EdgeSpec::Conic { a: onto(section(*p), e), apex: onto(section(*apex), e), b: onto(section(*q), e), w: *w }
            }
        }
    };
    let mut faces = Vec::new();
    // The ends: the end runs the section's way round (counter-clockwise about the line), the
    // start the other way.
    for (e, forward) in [(end, true), (start, false)] {
        let cap: Vec<Vec<EdgeSpec>> = loops
            .iter()
            .map(|l| {
                let es: Vec<EdgeSpec> = l.iter().map(|pc| edge(pc, e)).collect();
                if forward { es } else { es.iter().rev().map(EdgeSpec::reversed).collect() }
            })
            .collect();
        faces.push(FaceSpec { loops: cap, surface: SurfSpec::Plane, probe: None });
    }
    // The sides: one face a piece, from the start to the end.
    for pc in loops.iter().flatten() {
        let (p, q) = match pc {
            Piece::Line(p, q) => (*p, *q),
            Piece::Arc { p, q, .. } => (*p, *q),
        };
        let (ps, qs, qe, pe) = (onto(section(p), start), onto(section(q), start), onto(section(q), end), onto(section(p), end));
        let lp = vec![edge(pc, start), EdgeSpec::Line { a: qs, b: qe }, edge(pc, end).reversed(), EdgeSpec::Line { a: pe, b: ps }];
        match pc {
            Piece::Line(..) => faces.push(FaceSpec { loops: vec![lp], surface: SurfSpec::Plane, probe: None }),
            Piece::Arc { mid, tangent, .. } => {
                let back = |v: Vec2| section(v) + d * (lo - 1.0);
                let surface = SurfSpec::Extruded { a: back(p), b: back(q), mid: back(*mid), dir: d * (hi - lo + 2.0) };
                let m3 = section(*mid);
                let (ms, me) = (onto(m3, start), onto(m3, end));
                let out = (x * tangent.x + y * tangent.y).cross(d);
                faces.push(FaceSpec { loops: vec![lp], surface, probe: Some(((ms + me) * 0.5, out)) });
            }
        }
    }
    kernel::sew(&faces).map_err(|e| DocError::Invalid(format!("the member on line `{id}`: {e}")))
}

/// [`member_body`] by booleans, for a section with other curves than lines and arcs: run on past
/// both ends and cut on the end planes.
#[allow(clippy::too_many_arguments)]
fn member_body_cut(id: &str, a: Vec3, b: Vec3, regions: &[Region2], at: Vec2, turn: f64, start: End, end: End) -> Result<kernel::Body> {
    let (x, y) = frame::member_axes(b - a, turn).ok_or_else(|| DocError::Invalid(format!("line `{id}` has no length")))?;
    let plane = Plane::new(a - x * at.x - y * at.y, x, y).ok_or_else(|| DocError::Invalid(format!("line `{id}`: no plane square to it")))?;
    let reach = regions.iter().flat_map(|r| r.outer.polyline(1e-2).iter().map(|q| q.dist(at)).collect::<Vec<_>>()).fold(1.0_f64, f64::max);
    let run = reach * 50.0 + start.point.dist(a) + end.point.dist(b);
    let mut bodies = kernel::extrude(&plane, regions, -run, a.dist(b) + run)?;
    let mut body = match (bodies.pop(), bodies.is_empty()) {
        (Some(body), true) => body,
        _ => return Err(DocError::Invalid(format!("the member on line `{id}` is not one solid"))),
    };
    for e in [start, end] {
        let cut = Plane::from_normal(e.point, e.out * -1.0).ok_or_else(|| DocError::Invalid(format!("line `{id}`: no end plane")))?;
        body = kernel::half_space(&body, &cut, true)?
            .0
            .ok_or_else(|| DocError::Invalid(format!("the end cut leaves nothing of the member on line `{id}`")))?;
    }
    Ok(body)
}

/// Where member `a`'s outside is flat towards `towards` (a unit direction square to its line):
/// the distance of that face from its line, when the section's support that way is a straight
/// side (not a round's single point).
fn flat_side(outline: &[Vec2], ax: Vec3, ay: Vec3, at: Vec2, towards: Vec3) -> (f64, bool) {
    let vals: Vec<f64> = outline.iter().map(|q| (ax * (q.x - at.x) + ay * (q.y - at.y)).dot(towards)).collect();
    let h = vals.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let support: Vec<Vec2> = outline.iter().zip(&vals).filter(|(_, v)| **v > h - 1e-6).map(|(q, _)| *q).collect();
    let flat = support.iter().any(|p| support.iter().any(|q| p.dist(*q) > 1e-3));
    (h, flat)
}

/// How each member is cut at corners and where it ends on another line. A corner is two line
/// ends at one point: mitred (both end on the plane halving it) or butted (the member running
/// through goes on to the other's outside; the other ends on its face, or is cut to fit a round
/// one). Three or more ends at one point, and a line ending on another's middle, are butted
/// against the first line there.
fn corner_plan(
    segments: &[(String, Vec3, Vec3)],
    regions: &[Region2],
    at: Vec2,
    turn: f64,
    corners: frame::CornerTrim,
    butt_flip: bool,
    joints: &[frame::JointRule],
) -> Result<Vec<MemberTrim>> {
    use frame::CornerTrim;
    let n = segments.len();
    let mut plan = vec![MemberTrim::default(); n];
    if corners.is_none() && joints.iter().all(|j| j.trim.is_none()) {
        return Ok(plan);
    }
    let outline: Vec<Vec2> = regions.iter().flat_map(|r| r.outer.polyline(1e-3).iter().copied().collect::<Vec<_>>()).collect();
    let rule = |i: usize, j: usize| -> (CornerTrim, Option<usize>) {
        let (Some((ia, ..)), Some((ja, ..))) = (segments.get(i), segments.get(j)) else { return (corners, None) };
        match joints.iter().find(|r| (r.lines[0] == *ia && r.lines[1] == *ja) || (r.lines[0] == *ja && r.lines[1] == *ia)) {
            Some(r) => (r.trim, r.through.as_ref().and_then(|t| segments.iter().position(|(id, ..)| id == t)).filter(|k| *k == i || *k == j)),
            None => (corners, None),
        }
    };
    let axes = |m: usize| -> Option<(Vec3, Vec3)> { segments.get(m).and_then(|(_, a, b)| frame::member_axes(*b - *a, turn)) };
    let set_end = |plan: &mut Vec<MemberTrim>, m: usize, at_start: bool, e: End| {
        if let Some(t) = plan.get_mut(m) {
            if at_start {
                t.start = Some(e);
            } else {
                t.end = Some(e);
            }
        }
    };
    // Member `b` (its end at `p`, `ub` the way into it) stops against member `a` (along `da`):
    // on `a`'s face where that is flat, else cut to fit `a`'s outside.
    let butt = |plan: &mut Vec<MemberTrim>, a: usize, da: Vec3, b: usize, b_start: bool, p: Vec3, ub: Vec3| {
        let towards = (ub - da * ub.dot(da)).normalized();
        let flat = match (towards, axes(a)) {
            (Some(t), Some((ax, ay))) => {
                let (h, flat) = flat_side(&outline, ax, ay, at, t);
                flat.then_some((t, h))
            }
            _ => None,
        };
        match flat {
            Some((t, h)) if t.dot(ub).abs() >= MIN_END_COS => set_end(plan, b, b_start, End { point: p + t * h, out: t * -1.0 }),
            _ => {
                if let Some(m) = plan.get_mut(b)
                    && !m.copes.contains(&a)
                {
                    m.copes.push(a);
                }
            }
        }
    };
    // Every end: (member, at its start?, the point, the way from it into the member).
    let ends: Vec<(usize, bool, Vec3, Vec3)> = segments
        .iter()
        .enumerate()
        .flat_map(|(i, (_, a, b))| {
            let d = (*b - *a).normalized().unwrap_or(Vec3::X);
            [(i, true, *a, d), (i, false, *b, d * -1.0)]
        })
        .collect();
    let mut grouped = vec![false; ends.len()];
    for k in 0..ends.len() {
        if grouped.get(k).copied().unwrap_or(true) {
            continue;
        }
        let Some(&(_, _, p, _)) = ends.get(k) else { continue };
        let group: Vec<usize> = (k..ends.len()).filter(|&m| ends.get(m).is_some_and(|e| e.2.dist(p) < JOINT_TOL)).collect();
        for &m in &group {
            if let Some(g) = grouped.get_mut(m) {
                *g = true;
            }
        }
        if group.len() == 2 {
            let (Some(&(i, i_start, _, u)), Some(&(j, j_start, _, v))) = (ends.get(group[0]), ends.get(group[1])) else { continue };
            if i == j {
                continue;
            }
            let (trim, through) = rule(i, j);
            let angle = u.dot(v).clamp(-1.0, 1.0).acos();
            if trim.is_none() || angle < MIN_CORNER || angle > std::f64::consts::PI - MIN_CORNER {
                continue;
            }
            match trim {
                CornerTrim::Miter => {
                    let Some(nrm) = (u - v).normalized() else { continue };
                    // Each keeps the side of the halving plane its own line runs into.
                    set_end(&mut plan, i, i_start, End { point: p, out: nrm * -1.0 });
                    set_end(&mut plan, j, j_start, End { point: p, out: nrm });
                }
                CornerTrim::Butt => {
                    let first = through.unwrap_or(if butt_flip { i.max(j) } else { i.min(j) });
                    let (a_m, a_start, ua, b_m, b_start, ub) =
                        if first == i { (i, i_start, u, j, j_start, v) } else { (j, j_start, v, i, i_start, u) };
                    // The through member runs on to the far side of the other's section.
                    let Some((bx, by)) = axes(b_m) else { continue };
                    let beyond = -outline.iter().map(|q| (bx * (q.x - at.x) + by * (q.y - at.y)).dot(ua)).fold(0.0_f64, f64::min);
                    set_end(&mut plan, a_m, a_start, End { point: p - ua * beyond, out: ua * -1.0 });
                    butt(&mut plan, a_m, ua, b_m, b_start, p, ub);
                }
                CornerTrim::None => {}
            }
            continue;
        }
        if group.len() < 2 {
            continue;
        }
        // Three or more: every later one butts on the first (unless a rule leaves it square).
        let Some(&(first, _, _, uf)) = group.first().and_then(|&g| ends.get(g)) else { continue };
        for &g in group.iter().skip(1) {
            let Some(&(m, m_start, _, um)) = ends.get(g) else { continue };
            if m != first && !rule(first, m).0.is_none() {
                butt(&mut plan, first, uf, m, m_start, p, um);
            }
        }
    }
    // Lines ending on another line's middle: stopped on its face, or cut to fit against it.
    for &(j, j_start, p, uj) in &ends {
        for (i, (_, a, b)) in segments.iter().enumerate() {
            if i == j || rule(i, j).0.is_none() {
                continue;
            }
            let len = a.dist(*b);
            let Some(d) = (*b - *a).normalized() else { continue };
            let t = (p - *a).dot(d);
            if t > JOINT_TOL && t < len - JOINT_TOL && (p - (*a + d * t)).len() < JOINT_TOL {
                butt(&mut plan, i, d, j, j_start, p, uj);
            }
        }
    }
    Ok(plan)
}

/// The body name of the member on each of `lines`: its own name from `pinned` (by line id) when no
/// other body has it, else the first free one of `base`, `base (2)`, `base (3)`… that no line has
/// pinned. A member therefore keeps its name when other lines are added, deleted or reordered.
pub fn member_names(st: &ModelState, lines: &[String], pinned: &BTreeMap<String, String>, base: &str) -> Vec<String> {
    let reserved: HashSet<&str> = pinned.values().map(String::as_str).collect();
    let mut used: HashSet<String> = HashSet::new();
    let mut out: Vec<Option<String>> = lines
        .iter()
        .map(|line| {
            let n = pinned.get(line).map(|n| n.trim()).filter(|n| !n.is_empty() && st.body(n).is_none())?;
            used.insert(n.to_string()).then(|| n.to_string())
        })
        .collect();
    let base = if base.trim().is_empty() { "Member" } else { base.trim() };
    // Each candidate is refused at most once per body, used or reserved name: past that many,
    // a free one has been found for every member.
    let limit = st.bodies.len() + reserved.len() + lines.len() + 2;
    let mut next = 1usize;
    for slot in out.iter_mut().filter(|n| n.is_none()) {
        while next <= limit {
            let candidate = if next == 1 { base.to_string() } else { format!("{base} ({next})") };
            next += 1;
            if st.body(&candidate).is_none() && !used.contains(&candidate) && !reserved.contains(candidate.as_str()) {
                used.insert(candidate.clone());
                *slot = Some(candidate);
                break;
            }
        }
    }
    out.into_iter().map(|n| n.unwrap_or_else(|| base.to_string())).collect()
}

/// The ids of a `FrameMember` feature's lines in `st`, in the order its members are made.
pub fn member_line_ids(st: &ModelState, kind: &FeatureKind) -> Result<Vec<String>> {
    let FeatureKind::FrameMember { path_sketch, lines, .. } = kind else {
        return Err(DocError::Invalid("not a frame member feature".into()));
    };
    let ps = st.sketch(path_sketch.0).ok_or_else(|| DocError::Unknown(format!("sketch {path_sketch}")))?;
    Ok(member_lines(ps, lines)?.into_iter().map(|(id, _, _)| id).collect())
}

/// The member lines of the frame sketch, each as (curve id, start, end) in its frame: the lines
/// asked for, or every straight line drawn in it (not construction, not projected or otherwise
/// linked, and with a length).
fn member_lines(ss: &SolvedSketch, ids: &[String]) -> Result<Vec<(String, Vec3, Vec3)>> {
    let sk = &ss.sketch;
    let mut out = Vec::new();
    if ids.is_empty() {
        for (ci, c) in sk.curves.iter().enumerate() {
            if !c.construction
                && !c.centerline
                && c.link.is_none()
                && matches!(c.kind, CurveKind::Line { .. })
                && let Some((a, b)) = planar_line(ss, ci)
                && a.dist(b) > MIN_LENGTH
            {
                out.push((c.id.clone(), a, b));
            }
        }
        for w in sk.wires.iter().filter(|w| w.link.is_none()) {
            if let Some((a, b)) = straight(&w.pts)
                && a.dist(b) > MIN_LENGTH
            {
                out.push((w.id.clone(), a, b));
            }
        }
        if out.is_empty() {
            return Err(DocError::Invalid(format!("{} has no lines for members", ss.name)));
        }
        if out.len() > MAX_MEMBERS {
            return Err(DocError::Invalid(format!("at most {MAX_MEMBERS} members in one feature (pick the lines)")));
        }
        return Ok(out);
    }
    if ids.len() > MAX_MEMBERS {
        return Err(DocError::Invalid(format!("at most {MAX_MEMBERS} members in one feature")));
    }
    let mut seen = HashSet::new();
    for id in ids {
        if !seen.insert(id.as_str()) {
            return Err(DocError::Invalid(format!("line `{id}` is listed twice")));
        }
        let line = if let Some(wi) = sk.wire_index(id) {
            sk.wires.get(wi).and_then(|w| straight(&w.pts))
        } else {
            let ci = sk.curve_index(id).ok_or_else(|| DocError::Unknown(format!("line `{id}` in {}", ss.name)))?;
            planar_line(ss, ci)
        };
        let (a, b) = line.ok_or_else(|| DocError::Invalid(format!("`{id}` is not a straight line (members run along lines)")))?;
        if !(a.dist(b) > MIN_LENGTH) {
            return Err(DocError::Invalid(format!("line `{id}` has no length")));
        }
        out.push((id.clone(), a, b));
    }
    Ok(out)
}

/// A sketch line's ends in the sketch's frame.
fn planar_line(ss: &SolvedSketch, ci: usize) -> Option<(Vec3, Vec3)> {
    let CurveKind::Line { a, b } = ss.sketch.curves.get(ci)?.kind else { return None };
    let (pa, pb) = (ss.sketch.points.get(a)?.pos, ss.sketch.points.get(b)?.pos);
    Some((ss.plane.to_world(pa), ss.plane.to_world(pb)))
}

/// The ends of a 3D polyline that runs straight from its first point to its last: every point
/// within [`STRAIGHT_TOL`] of the line between them, none behind the start, past the end or back
/// from a point before it.
fn straight(pts: &[Vec3]) -> Option<(Vec3, Vec3)> {
    let (a, b) = (*pts.first()?, *pts.last()?);
    let (len, d) = (a.dist(b), (b - a).normalized()?);
    let mut reached = 0.0_f64;
    for p in pts {
        let q = *p - a;
        let t = q.dot(d);
        if (q - d * t).len() > STRAIGHT_TOL || t < reached - STRAIGHT_TOL || t > len + STRAIGHT_TOL {
            return None;
        }
        reached = reached.max(t);
    }
    Some((a, b))
}

/// The regions of a member's profile, in the profile's own coordinates: one piece of material,
/// holes and all; and what its members are named after when they have no name of their own.
fn profile_regions(vals: &BTreeMap<String, Value>, st: &ModelState, profile: &MemberProfile) -> Result<(Vec<Region2>, String)> {
    let (regions, description) = match profile {
        MemberProfile::Library { profile } => {
            let p = profile.resolve(|name, e| val(vals, e, Kind::Length).map_err(|err| DocError::Invalid(format!("profile `{name}`: {err}"))))?;
            (vec![p.region()?], p.description())
        }
        MemberProfile::Sketch { sketch, profiles } => {
            let ss = st.sketch(sketch.0).ok_or_else(|| DocError::Unknown(format!("sketch {sketch} (it must come earlier in the timeline)")))?;
            let picked = match profiles {
                ProfileSel::All => material(ss)?,
                sel => select_profiles(ss, sel)?,
            };
            (solidcraft_sketch::merge_regions(&picked), ss.name.clone())
        }
    };
    match regions.len() {
        1 => Ok((regions, description)),
        0 => Err(DocError::Invalid("the member profile has no area".into())),
        n => Err(DocError::Invalid(format!("the member profile is {n} separate pieces (pick one with `profiles`)"))),
    }
}

/// The material of a profile sketch: of what was drawn (as `solid.extrude` takes all profiles:
/// projected reference geometry only when the drawn curves make no profile, letters without
/// their counters filled in), each profile inside an even number of other profiles' outlines,
/// so nested loops alternate between material and hole.
fn material(ss: &SolvedSketch) -> Result<Vec<Region2>> {
    let drawn = solidcraft_sketch::find_drawn_profiles(&ss.sketch);
    let ps = solidcraft_sketch::without_text_counters(&ss.sketch, if drawn.is_empty() { ss.profiles.clone() } else { drawn });
    if ps.is_empty() {
        return Err(DocError::Invalid(format!("{} has no closed profiles", ss.name)));
    }
    if ps.len() > MAX_PROFILE_PIECES {
        return Err(DocError::Invalid(format!("{} has more than {MAX_PROFILE_PIECES} profiles (pick the member's with `profiles`)", ss.name)));
    }
    // Outer polylines and their boxes, once.
    let polys: Vec<Ring<Vec2>> = ps.iter().map(|p| p.region.outer.polyline(1e-3)).collect();
    let boxes: Vec<Aabb2> = polys.iter().map(|poly| poly.bounds()).collect();
    Ok(ps
        .iter()
        .enumerate()
        .filter(|(i, p)| {
            let at = p.region.interior_point();
            let depth = polys.iter().zip(&boxes).enumerate().filter(|(j, (poly, bb))| j != i && bb.contains(at) && poly.contains(at)).count();
            depth % 2 == 0
        })
        .map(|(_, p)| p.region.clone())
        .collect())
}
