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

/// How one member is cut where it meets others: how far it runs on past its line's start and end
/// (to reach a corner's outside, or to be mitred), the planes it is cut on (each keeping the side
/// its normal points to), and the members it is cut to fit against.
#[derive(Clone, Debug, Default)]
struct MemberTrim {
    start_ext: f64,
    end_ext: f64,
    cuts: Vec<(Vec3, Vec3)>,
    copes: Vec<usize>,
}

/// Where ends meet (mm).
const JOINT_TOL: f64 = 1e-4;
/// Corners sharper or straighter than this (radians) are left square: a mitre would run out.
const MIN_CORNER: f64 = 0.035;

/// The member along `a`–`b`: the section square to the line, run on past its ends as `trim`
/// says and cut on its planes.
fn member_body(id: &str, a: Vec3, b: Vec3, regions: &[Region2], at: Vec2, turn: f64, trim: &MemberTrim) -> Result<kernel::Body> {
    let (x, y) = frame::member_axes(b - a, turn).ok_or_else(|| DocError::Invalid(format!("line `{id}` has no length")))?;
    let plane = Plane::new(a - x * at.x - y * at.y, x, y).ok_or_else(|| DocError::Invalid(format!("line `{id}`: no plane square to it")))?;
    let mut bodies = kernel::extrude(&plane, regions, -trim.start_ext, a.dist(b) + trim.end_ext)?;
    let mut body = match (bodies.pop(), bodies.is_empty()) {
        (Some(body), true) => body,
        _ => return Err(DocError::Invalid(format!("the member on line `{id}` is not one solid"))),
    };
    for (origin, normal) in &trim.cuts {
        let cut = Plane::from_normal(*origin, *normal).ok_or_else(|| DocError::Invalid(format!("line `{id}`: no corner cut")))?;
        body = kernel::half_space(&body, &cut, true)?
            .0
            .ok_or_else(|| DocError::Invalid(format!("the corner cut leaves nothing of the member on line `{id}`")))?;
    }
    Ok(body)
}

/// How each member is cut at corners and where it ends on another line. A corner is two line
/// ends at one point: mitred (both cut on the plane halving it) or butted (the member running
/// through goes on to the other's outside; the other is cut to fit against it). Three or more
/// ends at one point, and a line ending on another's middle, are butted against the first line
/// there.
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
    // How far the section reaches from the line, and its outline (for butt depths).
    let outline: Vec<Vec2> = regions.iter().flat_map(|r| r.outer.polyline(1e-2).iter().copied().collect::<Vec<_>>()).collect();
    let reach = outline.iter().map(|q| q.dist(at)).fold(0.0_f64, f64::max).max(1e-3);
    let rule = |i: usize, j: usize| -> (CornerTrim, Option<usize>) {
        let (Some((ia, ..)), Some((ja, ..))) = (segments.get(i), segments.get(j)) else { return (corners, None) };
        match joints.iter().find(|r| (r.lines[0] == *ia && r.lines[1] == *ja) || (r.lines[0] == *ja && r.lines[1] == *ia)) {
            Some(r) => (r.trim, r.through.as_ref().and_then(|t| segments.iter().position(|(id, ..)| id == t)).filter(|k| *k == i || *k == j)),
            None => (corners, None),
        }
    };
    // Every end: (member, at its start?, the point, the direction from it into the member).
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
        if grouped[k] {
            continue;
        }
        let Some(&(_, _, p, _)) = ends.get(k) else { continue };
        let group: Vec<usize> = (k..ends.len()).filter(|&m| ends.get(m).is_some_and(|e| e.2.dist(p) < JOINT_TOL)).collect();
        for &m in &group {
            if let Some(g) = grouped.get_mut(m) {
                *g = true;
            }
        }
        if group.len() < 2 {
            continue;
        }
        let members: Vec<usize> = group.iter().filter_map(|&m| ends.get(m).map(|e| e.0)).collect();
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
                    let ext = (reach / (angle / 2.0).tan()).min(100.0 * reach) + reach;
                    let Some(nrm) = (u - v).normalized() else { continue };
                    for (m, start, side) in [(i, i_start, nrm), (j, j_start, nrm * -1.0)] {
                        if let Some(t) = plan.get_mut(m) {
                            if start {
                                t.start_ext = t.start_ext.max(ext);
                            } else {
                                t.end_ext = t.end_ext.max(ext);
                            }
                            t.cuts.push((p, side));
                        }
                    }
                }
                CornerTrim::Butt => {
                    let first = through.unwrap_or(if butt_flip { i.max(j) } else { i.min(j) });
                    let (a_m, a_start, ua, b_m) = if first == i { (i, i_start, u, j) } else { (j, j_start, v, i) };
                    // The through member runs on to the far side of the other's section (as it is
                    // placed on its own line: its axes follow that line's direction).
                    let Some((_, ba, bb)) = segments.get(b_m) else { continue };
                    let (bx, by) = frame::member_axes(*bb - *ba, turn).unwrap_or((Vec3::X, Vec3::Y));
                    let beyond = -outline.iter().map(|q| (bx * (q.x - at.x) + by * (q.y - at.y)).dot(ua)).fold(0.0_f64, f64::min);
                    if let Some(t) = plan.get_mut(a_m) {
                        if a_start {
                            t.start_ext = t.start_ext.max(beyond);
                        } else {
                            t.end_ext = t.end_ext.max(beyond);
                        }
                    }
                    if let Some(t) = plan.get_mut(b_m) {
                        t.copes.push(a_m);
                    }
                }
                CornerTrim::None => {}
            }
            continue;
        }
        // Three or more: every later one butts on the first (unless a rule leaves it square).
        let Some(&first) = members.first() else { continue };
        for &m in members.iter().skip(1) {
            if m != first && !rule(first, m).0.is_none() && let Some(t) = plan.get_mut(m) {
                t.copes.push(first);
            }
        }
    }
    // Lines ending on another line's middle: cut to fit against it.
    for &(j, _, p, _) in &ends {
        for (i, (_, a, b)) in segments.iter().enumerate() {
            if i == j || rule(i, j).0.is_none() {
                continue;
            }
            let len = a.dist(*b);
            let Some(d) = (*b - *a).normalized() else { continue };
            let t = (p - *a).dot(d);
            if t > JOINT_TOL && t < len - JOINT_TOL && (p - (*a + d * t)).len() < JOINT_TOL && let Some(m) = plan.get_mut(j) {
                if !m.copes.contains(&i) {
                    m.copes.push(i);
                }
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
