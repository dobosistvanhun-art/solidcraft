//! Drawings drawn: every view projected from the current model (visible and hidden edges by the
//! vector hidden-line removal of the view export), sections cut and hatched, details clipped and
//! enlarged, dimensions measured from the model, the frame and the title block — all as sheet
//! primitives (mm on the sheet), which the drawing workspace paints and the PDF and DXF exports
//! write.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use solidcraft_doc::drawing::{DimKind, DimRef, DrawDim, DrawView, Drawing, Projection, ViewKind};
use solidcraft_geom::{Aabb3, Mesh, Vec2, Vec3};
use solidcraft_kernel::EdgeCircle;
use solidcraft_render::Camera;

use super::meshes::{Placement, Scene};
use super::projection;

/// How a line is drawn (ISO 128 line types).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Style {
    /// Visible edges, the frame: wide continuous (0.5 mm).
    Visible,
    /// Hidden edges: narrow dashed.
    Hidden,
    /// Dimension, extension and leader lines, hatching, the title block grid: narrow continuous.
    Thin,
    /// Centre and cutting-plane lines: narrow chain.
    Chain,
    /// The ends of a cutting plane, the frame border: extra wide (0.7 mm).
    Wide,
}

impl Style {
    /// Line width (mm).
    pub fn width(self) -> f64 {
        match self {
            Style::Visible => 0.5,
            Style::Wide => 0.7,
            _ => 0.25,
        }
    }
}

/// Where a text sits against its point.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Align {
    Left,
    Center,
    Right,
}

/// A thing on the sheet (mm, from the lower left corner).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Prim {
    Line {
        a: Vec2,
        b: Vec2,
        style: Style,
    },
    /// One line of text on its baseline, turned `angle_deg` counter-clockwise.
    Text {
        at: Vec2,
        height: f64,
        angle_deg: f64,
        text: String,
        align: Align,
    },
    /// A filled triangle (arrowheads).
    Fill {
        points: Vec<Vec2>,
    },
}

/// A view as drawn: its frame on the sheet (for picking and moving) and where its model
/// points land.
#[derive(Clone, Debug, Serialize)]
pub struct ViewPlaced {
    pub id: u64,
    /// The view's extent on the sheet (min, max).
    pub min: Vec2,
    pub max: Vec2,
    pub scale: f64,
    #[serde(skip)]
    pub basis: Basis,
    /// The picking points (sheet mm) and the model point each stands for.
    #[serde(skip)]
    pub snaps: Vec<Snap>,
    /// What a pointer may pick in it, as drawn (sheet mm): the points (edge ends, circle centres,
    /// virtual sharps) and the edges (straight ones, circles and arcs seen face on). Only what
    /// the view shows: hidden edges only when it draws them.
    pub points: Vec<PickPoint>,
    pub edges: Vec<PickEdge>,
}

/// A point a pointer may pick in a view: `kind` end | center | sharp.
#[derive(Clone, Debug, Serialize)]
pub struct PickPoint {
    pub at: Vec2,
    pub kind: &'static str,
    #[serde(skip)]
    hidden: bool,
}

/// An edge a pointer may pick in a view: a straight one (`a`–`b`) or a circle or arc seen face on
/// (centre `a`, radius `r`, `full` for a whole circle), in sheet mm.
#[derive(Clone, Debug, Serialize)]
pub struct PickEdge {
    pub kind: &'static str,
    pub a: Vec2,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub b: Option<Vec2>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r: Option<f64>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub full: bool,
    #[serde(skip)]
    hidden: bool,
    /// The model edge it is (body, edge index) and, for a straight one, its ends in 3D.
    #[serde(skip)]
    model: Option<(String, usize, Vec3, Vec3)>,
}

/// The drawn lines of a view, bucketed, to tell whether a point lies on one (and on a visible one).
struct LineGrid {
    cell: f64,
    cells: std::collections::HashMap<(i64, i64), Vec<usize>>,
}

impl LineGrid {
    fn new(lines: &[(Vec2, Vec2, Style)], cell: f64) -> LineGrid {
        let mut cells: std::collections::HashMap<(i64, i64), Vec<usize>> = std::collections::HashMap::new();
        let key = |x: f64| (x / cell).floor() as i64;
        for (i, (a, b, style)) in lines.iter().enumerate().take(400_000) {
            // The model's own lines only (not hatching, a detail's circle or centre lines).
            if !matches!(style, Style::Visible | Style::Hidden) {
                continue;
            }
            let (x0, x1) = (key(a.x.min(b.x)), key(a.x.max(b.x)));
            let (y0, y1) = (key(a.y.min(b.y)), key(a.y.max(b.y)));
            if (x1 - x0 + 1) * (y1 - y0 + 1) > 4096 {
                continue;
            }
            for x in x0..=x1 {
                for y in y0..=y1 {
                    cells.entry((x, y)).or_default().push(i);
                }
            }
        }
        LineGrid { cell, cells }
    }

    /// 2 when a visible line passes within `tol` of `p`, 1 when only a hidden one does, else 0.
    fn on(&self, lines: &[(Vec2, Vec2, Style)], p: Vec2, tol: f64) -> u8 {
        let (cx, cy) = ((p.x / self.cell).floor() as i64, (p.y / self.cell).floor() as i64);
        let mut best = 0;
        for x in cx - 1..=cx + 1 {
            for y in cy - 1..=cy + 1 {
                for &i in self.cells.get(&(x, y)).into_iter().flatten() {
                    let Some((a, b, style)) = lines.get(i) else { continue };
                    if seg_dist(p, *a, *b) <= tol {
                        best = best.max(if *style == Style::Hidden { 1 } else { 2 });
                        if best == 2 {
                            return 2;
                        }
                    }
                }
            }
        }
        best
    }
}

fn seg_dist(p: Vec2, a: Vec2, b: Vec2) -> f64 {
    let ab = b - a;
    let l2 = ab.dot(ab);
    let t = if l2 > 0.0 { ((p - a).dot(ab) / l2).clamp(0.0, 1.0) } else { 0.0 };
    (a + ab * t - p).len()
}

/// Middles of straight edges, quadrant points of circles and crossings of straight edges, as
/// snaps with references that follow the model (`mid`, `quad:<k>`, `cross:<edge>:<body>`).
fn reference_snaps(edges: &[PickEdge], basis: &Basis, tol: f64, on: &dyn Fn(Vec2) -> u8) -> Vec<Snap> {
    let mut out = Vec::new();
    let snap = |at: Vec2, body: &str, edge: usize, kind: String, point: Vec3| Snap {
        at,
        r: DimRef { body: body.to_string(), edge: Some(edge), at: kind, point },
        radius: None,
        sharp: false,
    };
    let lines: Vec<(Vec2, Vec2, &str, usize, Vec3, Vec3)> = edges
        .iter()
        .filter(|x| x.kind == "line")
        .filter_map(|x| {
            let (body, i, a3, b3) = x.model.as_ref()?;
            Some((x.a, x.b?, body.as_str(), *i, *a3, *b3))
        })
        .take(4000)
        .collect();
    for (a, b, body, i, a3, b3) in &lines {
        out.push(snap((*a + *b) * 0.5, body, *i, "mid".into(), (*a3 + *b3) * 0.5));
    }
    for x in edges.iter().filter(|x| x.kind == "circle") {
        let (Some((body, i, c3, _)), Some(r)) = (x.model.as_ref(), x.r) else { continue };
        for (k, (d2, d3)) in [
            (Vec2::new(1.0, 0.0), basis.r),
            (Vec2::new(0.0, 1.0), basis.u),
            (Vec2::new(-1.0, 0.0), basis.r * -1.0),
            (Vec2::new(0.0, -1.0), basis.u * -1.0),
        ]
        .into_iter()
        .enumerate()
        {
            let at = x.a + d2 * r;
            if x.full || on(at) > 0 {
                out.push(snap(at, body, *i, format!("quad:{k}"), *c3 + d3 * r));
            }
        }
    }
    // Crossings: inside both edges (where they only meet at an end, that end is a vertex already).
    let mut crossings = 0;
    'outer: for (n, (a, b, body, i, a3, b3)) in lines.iter().enumerate() {
        for (c, d, body2, j, ..) in lines.iter().skip(n + 1) {
            let Some(t) = cross_param(*a, *b, *c, *d) else { continue };
            let at = *a + (*b - *a) * t.0;
            let len = (a.dist(*b)).min(c.dist(*d)).max(tol);
            let inside = |u: f64, l: f64| u * l > tol && (1.0 - u) * l > tol;
            if !(inside(t.0, a.dist(*b)) || inside(t.1, c.dist(*d))) || len <= tol {
                continue;
            }
            if [*a, *b, *c, *d].iter().any(|p| p.dist(at) < tol) {
                continue;
            }
            out.push(snap(at, body, *i, format!("cross:{j}:{body2}"), *a3 + (*b3 - *a3) * t.0));
            crossings += 1;
            if crossings > 20_000 {
                break 'outer;
            }
        }
    }
    out
}

/// Where the segments `a`–`b` and `c`–`d` cross: the parameters along each (0..1), or None.
fn cross_param(a: Vec2, b: Vec2, c: Vec2, d: Vec2) -> Option<(f64, f64)> {
    let (r, q) = (b - a, d - c);
    let den = r.x * q.y - r.y * q.x;
    if den.abs() < 1e-12 * (r.len() * q.len()).max(1e-12) {
        return None;
    }
    let w = c - a;
    let t = (w.x * q.y - w.y * q.x) / den;
    let u = (w.x * r.y - w.y * r.x) / den;
    ((-1e-9..=1.0 + 1e-9).contains(&t) && (-1e-9..=1.0 + 1e-9).contains(&u)).then_some((t, u))
}

/// Where the projections of two lines (through `a`–`b` and `c`–`d`) meet: the parameter along
/// the first, or None when they are parallel.
fn line_param(a: Vec2, b: Vec2, c: Vec2, d: Vec2) -> Option<f64> {
    let (r, q) = (b - a, d - c);
    let den = r.x * q.y - r.y * q.x;
    if den.abs() < 1e-12 * (r.len() * q.len()).max(1e-12) {
        return None;
    }
    let w = c - a;
    Some((w.x * q.y - w.y * q.x) / den)
}

/// The whole circles of the model's edges (centre, radius, axis): arcs about one centre and axis,
/// of one radius, making a whole turn between them.
fn whole_circles(m: &Model) -> Vec<(Vec3, f64, Vec3)> {
    let mut groups: Vec<(Vec3, f64, Vec3, f64)> = Vec::new();
    for circles in &m.circles {
        for c in circles.iter().flatten().take(20_000) {
            let ax: Vec3 = *c.axis;
            match groups
                .iter_mut()
                .find(|(k, r, a, _)| k.dist(c.center) < 1e-6 * (1.0 + r) && (r - c.radius).abs() < 1e-6 * (1.0 + r) && a.dot(ax).abs() > 0.999999)
            {
                Some(g) => g.3 += c.angle,
                None => groups.push((c.center, c.radius, ax, c.angle)),
            }
        }
    }
    groups.into_iter().filter(|g| g.3 >= std::f64::consts::TAU - 1e-6).map(|(k, r, a, _)| (k, r, a)).collect()
}

/// The points and edges of a view a pointer may pick (view coordinates), each kept only where the
/// view draws a line (`radius`: the model's, for the tolerance).
fn pickables(m: &Model, basis: &Basis, lines: &[(Vec2, Vec2, Style)], snaps: &mut Vec<Snap>, radius: f64) -> (Vec<PickPoint>, Vec<PickEdge>) {
    let tol = (radius * 2e-3).max(1e-4);
    let grid = LineGrid::new(lines, (radius / 40.0).max(1e-3));
    let on = |p: Vec2| grid.on(lines, p, tol);
    let mut edges: Vec<PickEdge> = Vec::new();
    // Whole circles, though the model may hold one as two or more arcs: the arcs about one centre
    // and axis, of one radius, whose angles make a whole turn (in 3D, per body).
    let whole = whole_circles(m);
    let is_whole = |c: &EdgeCircle| {
        whole
            .iter()
            .any(|(k, r, ax)| k.dist(c.center) < 1e-6 * (1.0 + r) && (r - c.radius).abs() < 1e-6 * (1.0 + r) && ax.dot(*c.axis).abs() > 0.999999)
    };
    for ((name, _, mesh), circles) in m.bodies.iter().zip(&m.circles) {
        for (i, e) in mesh.edges.iter().enumerate().take(20_000) {
            let (Some(a3), Some(b3)) = (e.first(), e.last()) else { continue };
            if let Some(c) = circles.get(i).copied().flatten() {
                // Only a circle seen face on draws as one.
                if c.axis.dot(basis.b).abs() < 0.999 {
                    continue;
                }
                let center = basis.project(c.center);
                let mid = basis.project(e.get(e.len() / 2).copied().unwrap_or(c.start));
                let vis = on(mid).max(on(basis.project(c.start)));
                if vis == 0 {
                    continue;
                }
                let full = is_whole(&c);
                if edges
                    .iter()
                    .any(|x| x.kind == "circle" && x.a.dist(center) < tol && x.r.is_some_and(|r| (r - c.radius).abs() < tol) && x.full == full)
                {
                    continue;
                }
                edges.push(PickEdge {
                    kind: "circle",
                    a: center,
                    b: None,
                    r: Some(c.radius),
                    full,
                    hidden: vis < 2,
                    model: Some((name.clone(), i, c.center, c.center)),
                });
                continue;
            }
            // A straight edge: every point of it on the line between its ends.
            let straight = e.len() >= 2 && e.iter().all(|p| p.dist_to_segment(*a3, *b3) < tol);
            if !straight {
                continue;
            }
            let (a, b) = (basis.project(*a3), basis.project(*b3));
            if a.dist(b) < tol * 4.0 {
                continue; // seen end on
            }
            let vis = on((a + b) * 0.5).min(on(a + (b - a) * 0.25).max(on(a + (b - a) * 0.75)));
            if vis == 0 {
                continue;
            }
            if edges
                .iter()
                .any(|x| x.kind == "line" && x.b.is_some_and(|xb| (x.a.dist(a) < tol && xb.dist(b) < tol) || (x.a.dist(b) < tol && xb.dist(a) < tol)))
            {
                continue;
            }
            edges.push(PickEdge { kind: "line", a, b: Some(b), r: None, full: false, hidden: vis < 2, model: Some((name.clone(), i, *a3, *b3)) });
        }
    }
    // A whole circle's seam (its one vertex, and the ends of the seam line of a cylinder through
    // it) is no corner of the part: not a point to measure to.
    let mut seams: Vec<Vec3> = Vec::new();
    for circles in &m.circles {
        for c in circles.iter().flatten().take(20_000) {
            if is_whole(c) {
                seams.push(c.start);
                seams.push(c.end);
            }
        }
    }
    let seam_tol = (radius * 1e-6).max(1e-9);
    // The points drawings measure to besides vertices and centres: the middles of straight
    // edges, the quadrant points of circles seen face on (where they are drawn: an arc's only
    // on it) and where two straight edges cross in the view.
    let extra = reference_snaps(&edges, basis, tol, &on);
    snaps.extend(extra);
    let mut points: Vec<PickPoint> = Vec::new();
    for sn in snaps.iter() {
        if !sn.sharp && sn.radius.is_none() && seams.iter().any(|q| q.dist(sn.r.point) < seam_tol) {
            continue;
        }
        let kind = if sn.sharp {
            "sharp"
        } else if sn.radius.is_some() {
            "center"
        } else if sn.r.at == "mid" {
            "mid"
        } else if sn.r.at.starts_with("quad:") {
            "quad"
        } else if sn.r.at.starts_with("cross:") {
            "cross"
        } else {
            "end"
        };
        let vis = match kind {
            "sharp" => 2,
            // Made from drawn edges: as visible as they are.
            "mid" | "quad" | "cross" => on(sn.at).max(1),
            "center" => edges.iter().filter(|x| x.kind == "circle" && x.a.dist(sn.at) < tol).map(|x| if x.hidden { 1 } else { 2 }).max().unwrap_or(0),
            _ => on(sn.at),
        };
        if vis == 0 || points.iter().any(|x| x.kind == kind && x.at.dist(sn.at) < tol) {
            continue;
        }
        points.push(PickPoint { at: sn.at, kind, hidden: vis < 2 });
        if points.len() > 50_000 {
            break;
        }
    }
    (points, edges)
}

/// A point a dimension may measure, as it lands in a view.
#[derive(Clone, Debug)]
pub struct Snap {
    pub at: Vec2,
    pub r: DimRef,
    /// The circle's radius when it is a circle's centre (diameter and radius dimensions).
    pub radius: Option<f64>,
    /// A virtual sharp: where the edges beside a rounded corner would meet.
    pub sharp: bool,
}

/// What a pick snaps to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickMode {
    /// Edge ends and circle centres.
    Points,
    /// Circles and arcs, by their edge.
    Circles,
    /// The corners rounded corners would have without their rounds.
    Sharps,
}

/// The edges of a round (an arc edge) and the corner they would make without it: the arc's
/// two ends and the point where the straight edges meeting them, carried on, cross.
fn virtual_sharp(circles: &[Option<EdgeCircle>], mesh: &Mesh, i: usize) -> Option<(Vec3, Vec3, Vec3)> {
    let c = circles.get(i).copied().flatten()?;
    if c.angle >= std::f64::consts::TAU - 1e-6 || c.angle < 1e-6 {
        return None;
    }
    let tol = (c.radius * 1e-4).max(1e-7);
    // The straight edge leaving `end` (other than the arc): its direction away from it.
    let away = |end: Vec3| -> Option<Vec3> {
        mesh.edges.iter().enumerate().filter(|(j, _)| *j != i).find_map(|(_, e)| {
            let (a, b) = (*e.first()?, *e.last()?);
            let straight = e.iter().all(|p| p.dist_to_segment(a, b) < tol * 10.0) && a.dist(b) > tol;
            if !straight {
                return None;
            }
            if a.dist(end) < tol {
                (b - a).normalized()
            } else if b.dist(end) < tol {
                (a - b).normalized()
            } else {
                None
            }
        })
    };
    let (d1, d2) = (away(c.start)?, away(c.end)?);
    // The lines `start - t·d1` and `end - u·d2` (carried on back past the arc): their closest points.
    let (p, q) = (c.start, c.end);
    let (a, b, cc) = (d1.dot(d1), d1.dot(d2), d2.dot(d2));
    let w = p - q;
    let (dd, e) = (d1.dot(w), d2.dot(w));
    let den = a * cc - b * b;
    if den.abs() < 1e-12 {
        return None;
    }
    let t = (b * e - cc * dd) / den;
    let u = (a * e - b * dd) / den;
    let (x1, x2) = (p + d1 * t, q + d2 * u);
    (x1.dist(x2) < c.radius * 1e-3 + 1e-6).then_some((p, q, (x1 + x2) * 0.5))
}

/// The sheet drawn.
#[derive(Clone, Debug, Default, Serialize)]
pub struct SheetDrawn {
    pub width: f64,
    pub height: f64,
    pub prims: Vec<Prim>,
    pub views: Vec<ViewPlaced>,
    /// Where each dimension's text sits (sheet mm), for picking it.
    pub dims: Vec<(u64, Vec2)>,
    /// Views that couldn't be drawn, with why.
    pub errors: Vec<(u64, String)>,
}

/// A view's projection: right, up and back (toward the viewer), and the model point at its centre.
#[derive(Clone, Copy, Debug, Default)]
pub struct Basis {
    pub r: Vec3,
    pub u: Vec3,
    pub b: Vec3,
    pub center: Vec3,
}

impl Basis {
    pub fn project(&self, p: Vec3) -> Vec2 {
        let d = p - self.center;
        Vec2::new(d.dot(self.r), d.dot(self.u))
    }
    /// The camera that projects like this (orthographic).
    fn camera(&self, radius: f64) -> Camera {
        let b = self.b;
        let pitch = b.z.clamp(-1.0, 1.0).asin();
        let yaw = if b.x.hypot(b.y) > 1e-9 { (-b.x).atan2(-b.y) } else { (-self.r.y).atan2(self.r.x) };
        let mut cam = Camera { target: self.center, yaw, pitch, roll: 0.0, distance: radius * 4.0 + 10.0, fov: 0.0, ..Camera::default() };
        let (r0, u0, _) = cam.basis();
        cam.roll = self.r.dot(u0).atan2(self.r.dot(r0));
        cam
    }
}

fn unit(v: Vec3) -> Vec3 {
    v.normalized().unwrap_or(Vec3::Z)
}

/// The basis of a standard orientation (front looks along +Y with Z up).
pub fn orientation(name: &str) -> Option<(Vec3, Vec3)> {
    // (right, up); back = right × up … as the camera has it: right = up × back.
    let (r, u) = match name.trim().to_ascii_lowercase().as_str() {
        "front" => (Vec3::X, Vec3::Z),
        "back" => (Vec3::X * -1.0, Vec3::Z),
        "right" => (Vec3::Y, Vec3::Z),
        "left" => (Vec3::Y * -1.0, Vec3::Z),
        "top" => (Vec3::X, Vec3::Y),
        "bottom" => (Vec3::X, Vec3::Y * -1.0),
        "iso" => {
            let b = unit(Vec3::new(1.0, -1.0, 1.0));
            let r = unit(Vec3::Z.cross(b));
            return Some((r, unit(b.cross(r))));
        }
        _ => return None,
    };
    Some((r, u))
}

fn with_back(r: Vec3, u: Vec3, center: Vec3) -> Basis {
    // right = up × back, so back = right × up.
    Basis { r, u, b: unit(r.cross(u)), center }
}

/// The view's basis from its parent's (projected views).
fn projected(parent: &Basis, side: &str, method: Projection) -> Option<Basis> {
    let s = match side.trim().to_ascii_lowercase().as_str() {
        "right" => (1.0, 0.0),
        "left" => (-1.0, 0.0),
        "above" | "top" => (0.0, 1.0),
        "below" | "bottom" => (0.0, -1.0),
        "iso" => {
            // A pictorial view: turned 45° about up and tilted down 35.26°.
            let b = unit(parent.b + parent.r + parent.u * std::f64::consts::SQRT_2.recip() * 1.0);
            let r = unit(parent.u.cross(b));
            return Some(Basis { r, u: unit(b.cross(r)), b, center: parent.center });
        }
        _ => return None,
    };
    let toward = parent.r * s.0 + parent.u * s.1;
    let back = if method == Projection::Third { toward } else { toward * -1.0 };
    Some(if s.0 != 0.0 {
        let u = parent.u;
        Basis { r: unit(u.cross(back)), u, b: back, center: parent.center }
    } else {
        let r = parent.r;
        Basis { r, u: unit(back.cross(r)), b: back, center: parent.center }
    })
}

/// The cutting plane of a section: through `a`–`b` of the parent view and its viewing direction;
/// the view looks the way the arrows point. Returns the plane point, the arrow direction and the
/// section view's basis.
fn section_basis(parent: &Basis, a: [f64; 2], b: [f64; 2], flip: bool) -> Option<(Vec3, Vec3, Basis)> {
    let pa = parent.center + parent.r * a[0] + parent.u * a[1];
    let pb = parent.center + parent.r * b[0] + parent.u * b[1];
    let along = (pb - pa).normalized()?;
    let mut arrow = unit(along.cross(parent.b));
    if flip {
        arrow = arrow * -1.0;
    }
    // Looking along the arrows: back is against them. Keep the parent's up where it can.
    let back = arrow * -1.0;
    let up = if parent.u.cross(back).len() > 1e-6 { parent.u } else { parent.b };
    let r = unit(up.cross(back));
    let u = unit(back.cross(r));
    Some((pa, arrow, Basis { r, u, b: back, center: parent.center }))
}

/// The bodies a drawing shows.
pub(crate) struct Model {
    pub bodies: Vec<(String, solidcraft_kernel::Body, Arc<Mesh>)>,
    /// Each body's edges that are circles or arcs (in its edge order).
    pub circles: Vec<Arc<Vec<Option<EdgeCircle>>>>,
    pub bounds: Aabb3,
}

/// How many bodies' circles, and views' lines, are kept between draws.
const KEEP_CIRCLES: usize = 64;
const KEEP_VIEWS: usize = 48;

/// The circles of bodies drawn before, by their mesh (held, so its address isn't reused).
type CircleCache = Vec<(Arc<Mesh>, Arc<Vec<Option<EdgeCircle>>>)>;
static CIRCLES: Mutex<CircleCache> = Mutex::new(Vec::new());

/// Views drawn before: the meshes they showed, what else they depend on, and the result.
type ViewCache = Vec<(Vec<Arc<Mesh>>, String, Arc<ViewGeom>)>;
static VIEWS: Mutex<ViewCache> = Mutex::new(Vec::new());

/// A view projected, in its own coordinates (model mm about its centre): what is drawn again
/// only when the model or the view changed, not for a dimension, a note or a move.
pub(crate) struct ViewGeom {
    basis: Basis,
    lines: Vec<(Vec2, Vec2, Style)>,
    /// Picking points, `at` in view coordinates.
    snaps: Vec<Snap>,
    /// Circles seen face on: their centres (view coordinates) and radii, for centre marks.
    marks: Vec<(Vec2, f64)>,
    /// Texts along lines (a flat pattern's bend notes): where (view coordinates), at what angle.
    notes: Vec<(Vec2, f64, String)>,
    /// What a pointer may pick (view coordinates).
    points: Vec<PickPoint>,
    edges: Vec<PickEdge>,
}

/// At most this many line segments in a flat pattern view.
const MAX_FLAT_LINES: usize = 200_000;

/// The sheet metal body a flat pattern view shows: the one named, else the last one the
/// drawing shows (the last of the design when it shows them all).
pub(crate) fn flat_sheet<'a>(st: &'a solidcraft_doc::ModelState, d: &Drawing, body: &str) -> Result<&'a solidcraft_doc::sheet::SheetBody, String> {
    if body.is_empty() {
        st.sheets
            .iter()
            .rev()
            .find(|x| d.bodies.is_empty() || d.bodies.contains(&x.body))
            .or_else(|| st.sheets.last())
            .ok_or_else(|| "there is no sheet metal body for a flat pattern".to_string())
    } else {
        st.sheets.iter().find(|x| x.body == body).ok_or_else(|| format!("`{body}` is not a sheet metal body"))
    }
}

/// A sheet metal body's flat pattern as a view, about its middle: the outline and cut-outs
/// (visible lines, their ends and arc centres to dimension to), each bend's centre line (a chain
/// line) with its note, and centre marks on the round holes.
fn flat_view(s: &crate::Session, d: &Drawing, body: &str) -> Result<ViewGeom, String> {
    use solidcraft_geom::Seg2;
    let st = s.model.state();
    let sh = flat_sheet(&st, d, body)?;
    let regions = sh.flat_regions().map_err(|e| e.to_string())?;
    let loops: Vec<&solidcraft_geom::Loop2> = regions.iter().flat_map(|r| std::iter::once(&r.outer).chain(&r.holes)).collect();
    let pieces: Vec<Vec<Vec<Vec2>>> = loops.iter().map(|l| l.segs.iter().map(|g| g.polyline(0.01)).collect()).collect();
    let (mut lo, mut hi) = (Vec2::new(f64::INFINITY, f64::INFINITY), Vec2::new(f64::NEG_INFINITY, f64::NEG_INFINITY));
    for p in pieces.iter().flatten().flatten() {
        lo = Vec2::new(lo.x.min(p.x), lo.y.min(p.y));
        hi = Vec2::new(hi.x.max(p.x), hi.y.max(p.y));
    }
    if !(lo.is_finite() && hi.is_finite()) {
        return Err("the flat pattern is empty".into());
    }
    let c = (lo + hi) * 0.5;
    let basis = Basis { r: Vec3::X, u: Vec3::Y, b: Vec3::Z, center: Vec3::new(c.x, c.y, 0.0) };
    let mut lines = Vec::new();
    for w in pieces.iter().flatten().flat_map(|p| p.windows(2)) {
        if let [a, b] = w {
            if lines.len() >= MAX_FLAT_LINES {
                return Err("the flat pattern has too many lines to draw".into());
            }
            lines.push((*a - c, *b - c, Style::Visible));
        }
    }
    let name = sh.body.clone();
    let snap = |p: Vec2, at: String, radius: Option<f64>| Snap {
        at: p - c,
        r: DimRef { body: name.clone(), edge: None, at, point: Vec3::new(p.x, p.y, 0.0) },
        radius,
        sharp: false,
    };
    let mut snaps: Vec<Snap> = Vec::new();
    let mut marks: Vec<(Vec2, f64)> = Vec::new();
    for l in &loops {
        for g in &l.segs {
            match *g {
                Seg2::Line { a, b } => {
                    snaps.push(snap(a, "flat".into(), None));
                    snaps.push(snap(b, "flat".into(), None));
                }
                Seg2::Arc { center, radius, start, sweep } => {
                    let on = |t: f64| center + Vec2::new(t.cos(), t.sin()) * radius;
                    snaps.push(snap(on(start), "flat".into(), None));
                    snaps.push(snap(on(start + sweep), "flat".into(), None));
                    if !snaps.iter().any(|x| x.radius == Some(radius) && (x.at - (center - c)).len() < 1e-9) {
                        snaps.push(snap(center, format!("flat_center:{radius}"), Some(radius)));
                    }
                }
                ref other => {
                    let p = other.polyline(0.01);
                    for q in [p.first(), p.last()].into_iter().flatten() {
                        snaps.push(snap(*q, "flat".into(), None));
                    }
                }
            }
        }
        // A round hole (one or more arcs about one centre making a whole turn): a centre mark.
        if let Some(Seg2::Arc { center, radius, .. }) = l.segs.first() {
            let mut turn = 0.0;
            let round = l.segs.iter().all(|g| match g {
                Seg2::Arc { center: k, radius: r, sweep, .. } if k.dist(*center) < 1e-6 && (r - radius).abs() < 1e-6 => {
                    turn += sweep;
                    true
                }
                _ => false,
            });
            if round && (turn.abs() - std::f64::consts::TAU).abs() < 1e-6 {
                marks.push((*center - c, *radius));
            }
        }
        if snaps.len() > 100_000 {
            break;
        }
    }
    let mut notes = Vec::new();
    for b in sh.bends().iter().take(10_000) {
        let (a, e) = (b.start - c, b.end - c);
        lines.push((a, e, Style::Chain));
        let along = e - a;
        let mut angle = along.y.atan2(along.x).to_degrees();
        if angle > 90.0 + 1e-9 {
            angle -= 180.0;
        } else if angle <= -90.0 + 1e-9 {
            angle += 180.0;
        }
        let dir = if b.up { "UP" } else { "DOWN" };
        notes.push(((a + e) * 0.5, angle, format!("{dir} {}° R{}", number(b.angle_deg), number(b.radius))));
    }
    // Picking: the outline's corners and arc ends, the holes' centres; its straight lines and arcs.
    let mut points: Vec<PickPoint> =
        snaps.iter().map(|sn| PickPoint { at: sn.at, kind: if sn.radius.is_some() { "center" } else { "end" }, hidden: false }).collect();
    points.dedup_by(|a, b| a.kind == b.kind && a.at.dist(b.at) < 1e-6);
    let mut edges: Vec<PickEdge> = Vec::new();
    for l in &loops {
        let round = matches!(l.segs.first(), Some(Seg2::Arc { .. }))
            && l.segs.iter().all(|g| matches!(g, Seg2::Arc { .. }))
            && l.segs.iter().map(|g| if let Seg2::Arc { sweep, .. } = g { *sweep } else { 0.0 }).sum::<f64>().abs() > std::f64::consts::TAU - 1e-6;
        for g in &l.segs {
            match *g {
                Seg2::Line { a, b } => {
                    edges.push(PickEdge { kind: "line", a: a - c, b: Some(b - c), r: None, full: false, hidden: false, model: None });
                    // Its middle, measured to as a flat point.
                    let m = (a + b) * 0.5;
                    points.push(PickPoint { at: m - c, kind: "mid", hidden: false });
                    snaps.push(snap(m, "flat".into(), None));
                }
                Seg2::Arc { center, radius, .. } => {
                    if !edges.iter().any(|x| x.kind == "circle" && x.a.dist(center - c) < 1e-6 && x.r == Some(radius)) {
                        edges.push(PickEdge { kind: "circle", a: center - c, b: None, r: Some(radius), full: round, hidden: false, model: None });
                    }
                }
                _ => {}
            }
        }
        if edges.len() > 50_000 {
            break;
        }
    }
    Ok(ViewGeom { basis, lines, snaps, marks, notes, points, edges })
}

fn circles_of(body: &solidcraft_kernel::Body, mesh: &Arc<Mesh>) -> Arc<Vec<Option<EdgeCircle>>> {
    if let Ok(c) = CIRCLES.lock()
        && let Some((_, v)) = c.iter().find(|(m, _)| Arc::ptr_eq(m, mesh))
    {
        return v.clone();
    }
    let v = Arc::new(solidcraft_kernel::edge_circles(body));
    if let Ok(mut c) = CIRCLES.lock() {
        if c.len() >= KEEP_CIRCLES {
            c.remove(0);
        }
        c.push((mesh.clone(), v.clone()));
    }
    v
}

/// A view's geometry from the cache, else made by `make` (and kept when it was made).
fn view_cached(m: &Model, key: String, make: impl FnOnce() -> Result<ViewGeom, String>) -> Result<Arc<ViewGeom>, String> {
    let meshes: Vec<Arc<Mesh>> = m.bodies.iter().map(|(.., me)| me.clone()).collect();
    let same = |ms: &[Arc<Mesh>]| ms.len() == meshes.len() && ms.iter().zip(&meshes).all(|(a, b)| Arc::ptr_eq(a, b));
    if let Ok(c) = VIEWS.lock()
        && let Some((.., g)) = c.iter().find(|(ms, k, _)| *k == key && same(ms))
    {
        return Ok(g.clone());
    }
    let g = Arc::new(make()?);
    if let Ok(mut c) = VIEWS.lock() {
        // Views of models no longer shown go first.
        c.retain(|(ms, ..)| same(ms) || ms.iter().all(|x| Arc::strong_count(x) > 1));
        if c.len() >= KEEP_VIEWS {
            c.remove(0);
        }
        c.push((meshes, key, g.clone()));
    }
    Ok(g)
}

pub(crate) fn model(s: &crate::Session, d: &Drawing) -> Result<Model, String> {
    let world = s.world_state();
    let mut bodies = Vec::new();
    let mut circles = Vec::new();
    let mut bounds = Aabb3::EMPTY;
    for b in world.bodies.iter() {
        let wanted = if d.bodies.is_empty() { s.body_visible(&b.name) } else { d.bodies.iter().any(|n| n == &b.name) };
        if !wanted {
            continue;
        }
        let mesh = b.mesh();
        for p in &mesh.positions {
            bounds = bounds.union(&Aabb3 { min: *p, max: *p });
        }
        circles.push(circles_of(&b.body, &mesh));
        bodies.push((b.name.clone(), b.body.clone(), mesh));
    }
    if bodies.is_empty() {
        return Err("no bodies to draw".into());
    }
    Ok(Model { bodies, circles, bounds })
}

fn scene(meshes: Vec<Arc<Mesh>>) -> Scene {
    let identity = solidcraft_doc::IDENTITY;
    let placements = meshes
        .iter()
        .enumerate()
        .map(|(i, m)| Placement { mesh: i, transform: identity, visibility: (0..m.edges.len()).map(|e| m.shaded_edge_visible(e)).collect() })
        .collect();
    let triangles = meshes.iter().map(|m| m.triangles.len()).sum();
    Scene { meshes, placements, triangles, bytes: 0 }
}

/// Model-space lines of one view (u, v in model mm about the basis centre): visible and hidden.
fn lines(meshes: Vec<Arc<Mesh>>, basis: &Basis, radius: f64) -> Result<Vec<(Vec2, Vec2, bool)>, String> {
    let sc = scene(meshes);
    let cam = basis.camera(radius);
    let drawn = projection::drawing(&sc, &[], &cam).map_err(str::to_string)?;
    // The camera's own axes may differ from the basis by the roll's rounding: re-express.
    let (cr, cu, _) = cam.basis();
    let (rr, ru) = (Vec2::new(cr.dot(basis.r), cr.dot(basis.u)), Vec2::new(cu.dot(basis.r), cu.dot(basis.u)));
    let map = |p: Vec2| Vec2::new(rr.x * p.x + ru.x * p.y, rr.y * p.x + ru.y * p.y);
    Ok(drawn.lines.into_iter().map(|l| (map(l.a), map(l.b), l.hidden)).collect())
}

/// The section of `mesh` by the plane through `p` with normal `n`: segments in 3D.
fn slice(mesh: &Mesh, p: Vec3, n: Vec3) -> Vec<(Vec3, Vec3)> {
    let mut out = Vec::new();
    for t in &mesh.triangles {
        let Some(v) = mesh.tri(t) else { continue };
        let d: Vec<f64> = v.iter().map(|q| (*q - p).dot(n)).collect();
        let mut pts = Vec::with_capacity(2);
        for i in 0..3 {
            let (a, b) = (i, (i + 1) % 3);
            let (da, db) = (d[a], d[b]);
            if (da > 0.0) != (db > 0.0) && (da - db).abs() > 1e-15 {
                let t = da / (da - db);
                pts.push(v[a] + (v[b] - v[a]) * t);
            }
        }
        if let [a, b] = pts[..] {
            out.push((a, b));
        }
    }
    out
}

/// Hatching (45°, `pitch` apart) inside the closed outline the segments make (even-odd).
fn hatch(segs: &[(Vec2, Vec2)], pitch: f64) -> Vec<(Vec2, Vec2)> {
    if segs.is_empty() || !(pitch > 0.0) {
        return Vec::new();
    }
    // Rotate by −45° so the hatch lines are horizontal.
    let (c, s) = (std::f64::consts::FRAC_1_SQRT_2, std::f64::consts::FRAC_1_SQRT_2);
    let rot = |p: Vec2| Vec2::new(c * p.x + s * p.y, -s * p.x + c * p.y);
    let back = |p: Vec2| Vec2::new(c * p.x - s * p.y, s * p.x + c * p.y);
    let rs: Vec<(Vec2, Vec2)> = segs.iter().map(|(a, b)| (rot(*a), rot(*b))).collect();
    let (lo, hi) = rs.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), (a, b)| (lo.min(a.y).min(b.y), hi.max(a.y).max(b.y)));
    let mut out = Vec::new();
    let n = ((hi - lo) / pitch).ceil() as i64;
    if !(0..=20_000).contains(&n) {
        return out;
    }
    for k in 0..=n {
        let y = lo + pitch * (k as f64 + 0.5);
        let mut xs: Vec<f64> = rs.iter().filter(|(a, b)| (a.y > y) != (b.y > y)).map(|(a, b)| a.x + (b.x - a.x) * (y - a.y) / (b.y - a.y)).collect();
        xs.sort_by(f64::total_cmp);
        for pair in xs.chunks_exact(2) {
            if let [x0, x1] = pair
                && x1 - x0 > 1e-9
            {
                out.push((back(Vec2::new(*x0, y)), back(Vec2::new(*x1, y))));
            }
        }
    }
    out
}

/// Clip the segment a–b to the circle about `c` of radius `r`.
fn clip_circle(a: Vec2, b: Vec2, c: Vec2, r: f64) -> Option<(Vec2, Vec2)> {
    let d = b - a;
    let f = a - c;
    let (qa, qb, qc) = (d.dot(d), 2.0 * f.dot(d), f.dot(f) - r * r);
    if qa < 1e-18 {
        return (qc <= 0.0).then_some((a, b));
    }
    let disc = qb * qb - 4.0 * qa * qc;
    if disc < 0.0 {
        return None;
    }
    let sq = disc.sqrt();
    let (t0, t1) = (((-qb - sq) / (2.0 * qa)).max(0.0), ((-qb + sq) / (2.0 * qa)).min(1.0));
    (t1 > t0).then(|| (a + d * t0, a + d * t1))
}

/// Points a dimension may measure in a view: edge ends and circle centres of every body.
fn snaps(m: &Model, basis: &Basis, to_sheet: &dyn Fn(Vec2) -> Vec2) -> Vec<Snap> {
    let mut out = Vec::new();
    for ((name, _, mesh), circles) in m.bodies.iter().zip(&m.circles) {
        for (i, e) in mesh.edges.iter().enumerate().take(20_000) {
            let (Some(a), Some(b)) = (e.first(), e.last()) else { continue };
            let circle = circles.get(i).copied().flatten();
            for (p, at) in [(*a, "start"), (*b, "end")] {
                out.push(Snap {
                    at: to_sheet(basis.project(p)),
                    r: DimRef { body: name.clone(), edge: Some(i), at: at.into(), point: p },
                    radius: None,
                    sharp: false,
                });
            }
            if let Some(c) = circle {
                out.push(Snap {
                    at: to_sheet(basis.project(c.center)),
                    r: DimRef { body: name.clone(), edge: Some(i), at: "center".into(), point: c.center },
                    radius: Some(c.radius),
                    sharp: false,
                });
                if let Some((_, _, x)) = virtual_sharp(circles, mesh, i) {
                    out.push(Snap {
                        at: to_sheet(basis.project(x)),
                        r: DimRef { body: name.clone(), edge: Some(i), at: "sharp".into(), point: x },
                        radius: None,
                        sharp: true,
                    });
                }
            }
        }
    }
    out
}

/// Where a dimension's model point is now: its edge's end or circle centre, else where it was.
fn resolve(m: &Model, r: &DimRef, basis: &Basis) -> (Vec3, Option<f64>) {
    // A flat pattern's points are where they were picked (flat coordinates); a hole's centre
    // carries its radius.
    if r.at == "flat" {
        return (r.point, None);
    }
    if let Some(rad) = r.at.strip_prefix("flat_center:") {
        return (r.point, rad.parse::<f64>().ok().filter(|x| x.is_finite() && *x > 0.0 && *x < 1e6));
    }
    let found = m.bodies.iter().zip(&m.circles).find(|((n, ..), _)| *n == r.body).and_then(|((_, _, mesh), circles)| {
        let i = r.edge?;
        let e = mesh.edges.get(i)?;
        match r.at.as_str() {
            "start" => e.first().map(|p| (*p, None)),
            "end" => e.last().map(|p| (*p, None)),
            "center" => circles.get(i).copied().flatten().map(|c| (c.center, Some(c.radius))),
            "sharp" => virtual_sharp(circles, mesh, i).map(|(.., x)| (x, None)),
            "mid" => Some(((*e.first()? + *e.last()?) * 0.5, None)),
            q if q.starts_with("quad:") => {
                let c = circles.get(i).copied().flatten()?;
                let dir = match q.trim_start_matches("quad:") {
                    "0" => basis.r,
                    "1" => basis.u,
                    "2" => basis.r * -1.0,
                    "3" => basis.u * -1.0,
                    _ => return None,
                };
                Some((c.center + dir * c.radius, None))
            }
            q if q.starts_with("cross:") => {
                // Where this edge's line crosses the other's, as the view sees them.
                let (j, body2) = q.trim_start_matches("cross:").split_once(':')?;
                let j: usize = j.parse().ok()?;
                let other = m.bodies.iter().find(|(n, ..)| n == body2)?.2.edges.get(j)?;
                let (a3, b3) = (*e.first()?, *e.last()?);
                let t = line_param(basis.project(a3), basis.project(b3), basis.project(*other.first()?), basis.project(*other.last()?))?;
                (t.is_finite() && t.abs() < 1e3).then(|| (a3 + (b3 - a3) * t, None))
            }
            _ => None,
        }
    });
    found.unwrap_or((r.point, None))
}

/// A measured value as a drawing shows it: up to two decimals, a decimal comma.
pub fn number(v: f64) -> String {
    let t = format!("{:.2}", (v * 100.0).round() / 100.0);
    let t = t.trim_end_matches('0').trim_end_matches('.');
    let t = if t == "-0" { "0" } else { t };
    t.replace('.', ",")
}

/// The text of a dimension: its own (`<>` the value) or the value, then the tolerance.
fn dim_text(d: &DrawDim, value: String) -> String {
    let base = if d.text.trim().is_empty() { value } else { d.text.replace("<>", &value) };
    let tol = d.tolerance.trim();
    if tol.is_empty() { base } else { format!("{base} {}", tol.replace('.', ",")) }
}

struct Out<'a> {
    prims: &'a mut Vec<Prim>,
    /// Arrowheads' length (mm).
    arrow: f64,
}

impl Out<'_> {
    fn line(&mut self, a: Vec2, b: Vec2, style: Style) {
        if a.is_finite() && b.is_finite() {
            self.prims.push(Prim::Line { a, b, style });
        }
    }
    fn text(&mut self, at: Vec2, height: f64, angle_deg: f64, text: impl Into<String>, align: Align) {
        let text = text.into();
        if at.is_finite() && !text.trim().is_empty() {
            self.prims.push(Prim::Text { at, height, angle_deg, text, align });
        }
    }
    /// An arrowhead with its tip at `tip`, pointing along `dir` (`self.arrow` long, 1:3 wide).
    fn arrow(&mut self, tip: Vec2, dir: Vec2) {
        let Some(d) = dir.normalized() else { return };
        let n = Vec2::new(-d.y, d.x);
        let base = tip - d * self.arrow;
        self.prims.push(Prim::Fill { points: vec![tip, base + n * (self.arrow / 6.0), base - n * (self.arrow / 6.0)] });
    }
}

/// A dimension drawn on the sheet.
/// Roughly how wide a dimension text of `chars` characters is (sheet mm).
fn text_width(chars: usize, height: f64) -> f64 {
    chars as f64 * height * 0.62
}

/// A dimension drawn on the sheet. `place`: where it was put (sheet mm), else `offset` decides.
fn draw_dim(out: &mut Out, d: &DrawDim, pts: &[(Vec2, Option<f64>, Vec3)], scale: f64, place: Option<Vec2>, th: f64) {
    let gap = 1.0;
    let over = 2.0;
    match d.kind {
        DimKind::Horizontal | DimKind::Vertical | DimKind::Aligned => {
            let (Some((c1, r1, m1)), Some((c2, r2, m2))) = (pts.first(), pts.get(1)) else { return };
            // To the near or far side of the arcs it measures to: each end moves by its radius
            // along the measuring direction.
            let axis = match d.kind {
                DimKind::Horizontal => Vec2::new(1.0, 0.0),
                DimKind::Vertical => Vec2::new(0.0, 1.0),
                _ => (*c2 - *c1).normalized().unwrap_or(Vec2::new(1.0, 0.0)),
            };
            let sign = if (*c2 - *c1).dot(axis) >= 0.0 { 1.0 } else { -1.0 };
            let side = match d.arc.as_str() {
                "max" => 1.0,
                "min" => -1.0,
                _ => 0.0,
            };
            let (p1, p2) = (&(*c1 - axis * (sign * side * r1.unwrap_or(0.0) * scale)), &(*c2 + axis * (sign * side * r2.unwrap_or(0.0) * scale)));
            let arcs = side != 0.0 && (r1.is_some() || r2.is_some());
            let (dir, value) = match d.kind {
                DimKind::Horizontal => (Vec2::new(1.0, 0.0), (p2.x - p1.x).abs() / scale),
                DimKind::Vertical => (Vec2::new(0.0, 1.0), (p2.y - p1.y).abs() / scale),
                // The distance as the view shows it (two points at different depths measure
                // their projection, as on paper; a flat pattern's points are flat already).
                _ => ((*p2 - *p1).normalized().unwrap_or(axis), (*p2 - *p1).len() / scale),
            };
            let _ = (m1, m2, arcs);
            let n = Vec2::new(-dir.y, dir.x);
            // The dimension line runs through where it was put, else `offset` off the outer of
            // the two points.
            let (s1, s2) = (p1.dot(n), p2.dot(n));
            let level = match place {
                Some(q) => q.dot(n),
                None => (if d.offset >= 0.0 { s1.max(s2) } else { s1.min(s2) }) + d.offset,
            };
            let foot = |p: Vec2| p + n * (level - p.dot(n));
            let (a, b) = (foot(*p1), foot(*p2));
            for (p, f) in [(*p1, a), (*p2, b)] {
                let along = f - p;
                if along.len() > gap {
                    let u = along.normalized().unwrap_or(n);
                    out.line(p + u * gap, f + u * over, Style::Thin);
                }
            }
            out.line(a, b, Style::Thin);
            let ab = b - a;
            if ab.len() > 8.0 {
                out.arrow(a, a - b);
                out.arrow(b, b - a);
            } else {
                // Too short: the arrows point in from outside.
                let u = ab.normalized().unwrap_or(dir);
                out.arrow(a, u);
                out.arrow(b, u * -1.0);
                out.line(a - u * 6.0, a, Style::Thin);
                out.line(b, b + u * 6.0, Style::Thin);
            }
            let mut angle = dir.y.atan2(dir.x).to_degrees();
            if angle > 90.0 + 1e-9 {
                angle -= 180.0;
            } else if angle <= -90.0 + 1e-9 {
                angle += 180.0;
            }
            let up = Vec2::new(-(angle.to_radians().sin()), angle.to_radians().cos());
            let text = dim_text(d, number(value));
            // The text where it was put along the line (the line carried on to it when it is
            // outside), else in the middle.
            let mid = (a + b) * 0.5;
            let at = match (place, ab.normalized()) {
                (Some(q), Some(u)) => {
                    // Between the extension lines it sits in the middle; outside them, where put.
                    let t = (q - a).dot(u);
                    let t = if (0.0..=ab.len()).contains(&t) { ab.len() * 0.5 } else { t };
                    let half = text_width(text.chars().count(), th) * 0.5;
                    if t < -half - 1.0 {
                        out.line(a + u * (t - half), a, Style::Thin);
                    } else if t > ab.len() + half + 1.0 {
                        out.line(b, a + u * (t + half), Style::Thin);
                    }
                    a + u * t
                }
                _ => mid,
            };
            out.text(at + up * 1.0, th, angle, text, Align::Center);
        }
        DimKind::Diameter | DimKind::Radius => {
            let Some((c, radius, _)) = pts.first() else { return };
            let Some(r) = *radius else { return };
            let rs = r * scale;
            // A leader at 45° (turned by the offset's sign), through the centre for a diameter.
            // Toward where it was put, else at 45° (turned by the offset's sign).
            let toward = place.and_then(|q| (q - *c).normalized());
            let dir = toward.unwrap_or_else(|| {
                if d.offset >= 0.0 { Vec2::new(1.0, 1.0) } else { Vec2::new(-1.0, 1.0) }.normalized().unwrap_or(Vec2::new(1.0, 0.0))
            });
            let tip = *c + dir * rs;
            let out_end = match place {
                Some(q) if toward.is_some() && (q - *c).len() > rs + 2.0 => q,
                _ => tip + dir * (8.0 + d.offset.abs()),
            };
            let (value, start) =
                if d.kind == DimKind::Diameter { (format!("Ø{}", number(2.0 * r)), *c - dir * rs) } else { (format!("R{}", number(r)), *c) };
            out.line(start, out_end, Style::Thin);
            out.arrow(tip, dir * -1.0);
            if d.kind == DimKind::Diameter {
                out.arrow(start, dir);
            }
            // The shelf as long as its text.
            let text = dim_text(d, value);
            let len = text_width(text.chars().count(), th) + 2.0;
            let shelf = out_end + Vec2::new(if dir.x >= 0.0 { len } else { -len }, 0.0);
            out.line(out_end, shelf, Style::Thin);
            let align = if dir.x >= 0.0 { Align::Left } else { Align::Right };
            out.text(out_end + Vec2::new(if dir.x >= 0.0 { 1.0 } else { -1.0 }, 1.0), th, 0.0, text, align);
        }
        DimKind::Angle => {
            let (Some((c, ..)), Some((p1, ..)), Some((p2, ..))) = (pts.first(), pts.get(1), pts.get(2)) else { return };
            let (Some(u1), Some(u2)) = ((*p1 - *c).normalized(), (*p2 - *c).normalized()) else { return };
            let a1 = u1.y.atan2(u1.x);
            let mut sweep = u2.y.atan2(u2.x) - a1;
            while sweep <= -std::f64::consts::PI {
                sweep += std::f64::consts::TAU;
            }
            while sweep > std::f64::consts::PI {
                sweep -= std::f64::consts::TAU;
            }
            let r = place.map(|q| (q - *c).len()).filter(|r| *r > 2.0).unwrap_or((10.0 + d.offset.abs()).max(5.0));
            let k = 24;
            let at = |t: f64| *c + Vec2::new((a1 + sweep * t).cos(), (a1 + sweep * t).sin()) * r;
            for i in 0..k {
                out.line(at(i as f64 / k as f64), at((i + 1) as f64 / k as f64), Style::Thin);
            }
            let s = sweep.signum();
            out.arrow(at(0.0), Vec2::new(-(a1).sin(), a1.cos()) * -s);
            let a2 = a1 + sweep;
            out.arrow(at(1.0), Vec2::new(-a2.sin(), a2.cos()) * s);
            out.text(
                at(0.5) + (at(0.5) - *c).normalized().unwrap_or(Vec2::new(0.0, 1.0)) * 2.0,
                th,
                0.0,
                dim_text(d, format!("{}°", number(sweep.abs().to_degrees()))),
                Align::Center,
            );
        }
    }
}

/// The frame (ISO 5457: 20 mm binding margin on the left, 10 mm elsewhere) and the title block.
fn frame_and_title(out: &mut Out, d: &Drawing, w: f64, h: f64, material: &str) {
    let (l, r, b, t) = (20.0, w - 10.0, 10.0, h - 10.0);
    out.line(Vec2::new(l, b), Vec2::new(r, b), Style::Wide);
    out.line(Vec2::new(r, b), Vec2::new(r, t), Style::Wide);
    out.line(Vec2::new(r, t), Vec2::new(l, t), Style::Wide);
    out.line(Vec2::new(l, t), Vec2::new(l, b), Style::Wide);
    // The title block: 180 mm wide in the lower right corner.
    let bw = 180.0f64.min(r - l);
    let x0 = r - bw;
    let rows = [0.0, 8.0, 16.0, 24.0, 36.0];
    let top = b + rows[4];
    out.line(Vec2::new(x0, b), Vec2::new(x0, top), Style::Visible);
    out.line(Vec2::new(x0, top), Vec2::new(r, top), Style::Visible);
    for y in &rows[1..4] {
        out.line(Vec2::new(x0, b + y), Vec2::new(r, b + y), Style::Thin);
    }
    let cols = [0.0, 45.0, 90.0, 135.0];
    for x in &cols[1..] {
        out.line(Vec2::new(x0 + x, b), Vec2::new(x0 + x, b + rows[3]), Style::Thin);
    }
    let tb = &d.title;
    let mat = if tb.material.trim().is_empty() { material.to_string() } else { tb.material.clone() };
    let (sw, sh) = d.sheet.dims();
    let cells: [[(&str, String); 4]; 3] = [
        [("Drawn", tb.drawn_by.clone()), ("Checked", tb.checked_by.clone()), ("Date", tb.date.clone()), ("Revision", tb.revision.clone())],
        [
            ("Material", mat),
            ("Scale", d.scale.clone()),
            ("Sheet", format!("{} ({sw:.0}×{sh:.0})", d.sheet.size)),
            ("Tolerances", tb.tolerances.clone()),
        ],
        [("Company", tb.company.clone()), ("Drawing no.", tb.number.clone()), ("", String::new()), ("", String::new())],
    ];
    for (row, cells) in cells.iter().enumerate() {
        let y = b + rows[row];
        for (col, (label, value)) in cells.iter().enumerate() {
            let x = x0 + cols[col] + 1.5;
            if !label.is_empty() {
                out.text(Vec2::new(x, y + 5.6), 1.8, 0.0, *label, Align::Left);
            }
            out.text(Vec2::new(x, y + 1.6), 2.5, 0.0, value.clone(), Align::Left);
        }
    }
    // The projection symbol in the third row's last cells.
    let sym = Vec2::new(x0 + cols[2] + 22.5, b + rows[2] + 4.0);
    projection_symbol(out, sym, d.projection);
    out.text(Vec2::new(x0 + 1.5, b + rows[3] + 7.0), 1.8, 0.0, "Title", Align::Left);
    out.text(
        Vec2::new(x0 + 1.5, b + rows[3] + 2.5),
        5.0,
        0.0,
        if tb.title.trim().is_empty() { d.name.clone() } else { tb.title.clone() },
        Align::Left,
    );
}

/// The ISO 5456-2 symbol of the projection method: a truncated cone, its end view to the right
/// (first angle) or left (third angle).
fn projection_symbol(out: &mut Out, c: Vec2, method: Projection) {
    let (side, end) = if method == Projection::First {
        (c - Vec2::new(5.0, 0.0), c + Vec2::new(6.0, 0.0))
    } else {
        (c + Vec2::new(5.0, 0.0), c - Vec2::new(6.0, 0.0))
    };
    let (big, small, len) = (2.5, 1.25, 6.0);
    let dir = if method == Projection::First { 1.0 } else { -1.0 };
    let (x0, x1) = (side.x - len / 2.0 * dir, side.x + len / 2.0 * dir);
    out.line(Vec2::new(x0, side.y + small), Vec2::new(x1, side.y + big), Style::Thin);
    out.line(Vec2::new(x0, side.y - small), Vec2::new(x1, side.y - big), Style::Thin);
    out.line(Vec2::new(x0, side.y + small), Vec2::new(x0, side.y - small), Style::Thin);
    out.line(Vec2::new(x1, side.y + big), Vec2::new(x1, side.y - big), Style::Thin);
    for r in [big, small] {
        for i in 0..24 {
            let (a, b) = (i as f64 / 24.0 * std::f64::consts::TAU, (i + 1) as f64 / 24.0 * std::f64::consts::TAU);
            out.line(end + Vec2::new(a.cos(), a.sin()) * r, end + Vec2::new(b.cos(), b.sin()) * r, Style::Thin);
        }
    }
}

/// Draw a drawing from the current model.
pub(crate) fn draw(s: &crate::Session, d: &Drawing) -> SheetDrawn {
    let (w, h) = d.sheet.dims();
    let mut drawn = SheetDrawn { width: w, height: h, ..Default::default() };
    if !d.is_sane() {
        drawn.errors.push((0, "the drawing holds invalid numbers".into()));
        return drawn;
    }
    let m = match model(s, d) {
        Ok(m) => Some(m),
        Err(e) => {
            if !d.views.is_empty() {
                drawn.errors.push((0, e));
            }
            None
        }
    };
    let material = m
        .as_ref()
        .and_then(|m| {
            let names: Vec<String> = m
                .bodies
                .iter()
                .map(|(n, ..)| s.doc.body_material_in(n, &s.model.state().sheets))
                .filter(|m| m != solidcraft_doc::DEFAULT_MATERIAL)
                .collect();
            names.first().filter(|f| names.iter().all(|n| n == *f)).cloned()
        })
        .unwrap_or_default();
    let mut prims = Vec::new();
    let mut out = Out { prims: &mut prims, arrow: d.dim_text() * 3.0 / 3.5 };
    frame_and_title(&mut out, d, w, h, &material);
    let Some(m) = m else {
        drawn.prims = prims;
        return drawn;
    };
    let center = (m.bounds.min + m.bounds.max) * 0.5;
    let radius = (m.bounds.max - m.bounds.min).len().max(1.0) * 0.5;
    // The views in an order where parents come first.
    let mut bases: BTreeMap<u64, Basis> = BTreeMap::new();
    let mut order: Vec<&DrawView> = Vec::new();
    let mut left: Vec<&DrawView> = d.views.iter().collect();
    for _ in 0..d.views.len() + 1 {
        let (ready, rest): (Vec<&DrawView>, Vec<&DrawView>) = left.into_iter().partition(|v| match &v.kind {
            ViewKind::Base { .. } | ViewKind::Flat { .. } => true,
            ViewKind::Projected { parent, .. } | ViewKind::Section { parent, .. } | ViewKind::Detail { parent, .. } => {
                order.iter().any(|o| o.id == *parent)
            }
        });
        if ready.is_empty() {
            for v in &rest {
                drawn.errors.push((v.id, "its parent view is missing".into()));
            }
            break;
        }
        order.extend(ready);
        left = rest;
    }
    for v in order {
        let scale = d.view_scale(v);
        let at = Vec2::new(v.at[0], v.at[1]);
        let to_sheet = |p: Vec2| at + p * scale;
        let parent = |id: u64| bases.get(&id).copied();
        let parent_basis = match &v.kind {
            ViewKind::Projected { parent: p, .. } | ViewKind::Section { parent: p, .. } | ViewKind::Detail { parent: p, .. } => parent(*p),
            _ => None,
        };
        // What the view's lines depend on besides the model: its kind, its parent's projection,
        // the model's extent and (for hatching) its scale.
        let key = format!("{:?}|{:?}|{:?}|{:?}|{}", v.kind, parent_basis, center, radius, scale);
        let make = || -> Result<ViewGeom, String> {
            let result: Result<(Basis, Vec<(Vec2, Vec2, Style)>), String> = (|| {
                match &v.kind {
                    ViewKind::Flat { .. } => Err("a flat pattern is not projected".to_string()),
                    ViewKind::Base { orientation: o } => {
                        let (r, u) = orientation(o).ok_or_else(|| format!("unknown orientation `{o}`"))?;
                        let basis = with_back(r, u, center);
                        let ls = lines(m.bodies.iter().map(|(.., me)| me.clone()).collect(), &basis, radius)?;
                        Ok((basis, ls.into_iter().map(|(a, b, hid)| (a, b, if hid { Style::Hidden } else { Style::Visible })).collect()))
                    }
                    ViewKind::Projected { parent: p, side } => {
                        let pb = parent(*p).ok_or("its parent view is missing")?;
                        let basis = projected(&pb, side, d.projection).ok_or_else(|| format!("unknown side `{side}`"))?;
                        let ls = lines(m.bodies.iter().map(|(.., me)| me.clone()).collect(), &basis, radius)?;
                        Ok((basis, ls.into_iter().map(|(a, b, hid)| (a, b, if hid { Style::Hidden } else { Style::Visible })).collect()))
                    }
                    ViewKind::Section { parent: p, a, b, flip } => {
                        let pb = parent(*p).ok_or("its parent view is missing")?;
                        let (p0, arrow, basis) = section_basis(&pb, *a, *b, *flip).ok_or("the cutting line has no length")?;
                        // Keep the material the arrows point at; the cut faces are hatched.
                        let plane_x = unit(arrow.cross(pb.b));
                        let plane = solidcraft_geom::Plane { origin: p0, x: plane_x, y: pb.b };
                        let mut kept = Vec::new();
                        let mut cut = Vec::new();
                        for (_, body, mesh) in &m.bodies {
                            let pieces = solidcraft_kernel::split_by_plane(body, &plane).map_err(|e| e.to_string())?;
                            for piece in pieces {
                                let pm = piece.display_mesh(radius * 2e-4).map_err(|e| e.to_string())?;
                                let n = pm.positions.len().max(1) as f64;
                                let c = pm.positions.iter().fold(Vec3::ZERO, |acc, p| acc + *p) * (1.0 / n);
                                if (c - p0).dot(arrow) > 0.0 {
                                    kept.push(Arc::new(pm));
                                }
                            }
                            cut.extend(slice(mesh, p0, arrow));
                        }
                        let mut ls: Vec<(Vec2, Vec2, Style)> = if kept.is_empty() {
                            Vec::new()
                        } else {
                            lines(kept, &basis, radius)?.into_iter().filter(|l| !l.2).map(|(a, b, _)| (a, b, Style::Visible)).collect()
                        };
                        let segs: Vec<(Vec2, Vec2)> = cut.iter().map(|(a, b)| (basis.project(*a), basis.project(*b))).collect();
                        ls.extend(hatch(&segs, 3.0 / scale).into_iter().map(|(a, b)| (a, b, Style::Thin)));
                        Ok((basis, ls))
                    }
                    ViewKind::Detail { parent: p, center: c, radius: r } => {
                        let pb = parent(*p).ok_or("its parent view is missing")?;
                        let cc = Vec2::new(c[0], c[1]);
                        let ls = lines(m.bodies.iter().map(|(.., me)| me.clone()).collect(), &pb, radius)?;
                        let clipped = ls
                            .into_iter()
                            .filter_map(|(a, b, hid)| {
                                clip_circle(a, b, cc, *r).map(|(a, b)| (a - cc, b - cc, if hid { Style::Hidden } else { Style::Visible }))
                            })
                            .collect::<Vec<_>>();
                        let mut ls = clipped;
                        for i in 0..48 {
                            let (t0, t1) = (i as f64 / 48.0 * std::f64::consts::TAU, (i + 1) as f64 / 48.0 * std::f64::consts::TAU);
                            ls.push((Vec2::new(t0.cos(), t0.sin()) * *r, Vec2::new(t1.cos(), t1.sin()) * *r, Style::Thin));
                        }
                        let basis = Basis { center: pb.center + pb.r * c[0] + pb.u * c[1], ..pb };
                        Ok((basis, ls))
                    }
                }
            })();
            let (basis, lines) = result?;
            let mut snaps = snaps(&m, &basis, &|p| p);
            // Centre marks on the circles seen face on (holes, bosses).
            let mut marks: Vec<(Vec2, f64)> = Vec::new();
            if !matches!(v.kind, ViewKind::Detail { .. }) {
                for ((.., mesh), circles) in m.bodies.iter().zip(&m.circles) {
                    for c in circles.iter().take(mesh.edges.len().min(20_000)).flatten() {
                        if c.axis.dot(basis.b).abs() < 0.999 {
                            continue;
                        }
                        let p = basis.project(c.center);
                        if marks.iter().any(|(q, _)| q.dist(p) < 1e-3) {
                            continue;
                        }
                        marks.push((p, c.radius));
                    }
                }
            }
            let (points, edges) = pickables(&m, &basis, &lines, &mut snaps, radius);
            Ok(ViewGeom { basis, lines, snaps, marks, notes: Vec::new(), points, edges })
        };
        // A flat pattern is drawn from the sheet metal body each time (cheap: no projection).
        let geom = match &v.kind {
            ViewKind::Flat { body } => flat_view(s, d, body).map(Arc::new),
            _ => view_cached(&m, key, make),
        };
        match geom {
            Ok(g) => {
                let basis = g.basis;
                let mut min = Vec2::new(f64::INFINITY, f64::INFINITY);
                let mut max = Vec2::new(f64::NEG_INFINITY, f64::NEG_INFINITY);
                for &(a, b, style) in &g.lines {
                    if style == Style::Hidden && !v.hidden {
                        continue;
                    }
                    let (a, b) = (to_sheet(a), to_sheet(b));
                    min = Vec2::new(min.x.min(a.x).min(b.x), min.y.min(a.y).min(b.y));
                    max = Vec2::new(max.x.max(a.x).max(b.x), max.y.max(a.y).max(b.y));
                    out.line(a, b, style);
                }
                // Centre marks: two chain lines across each circle seen face on.
                let mut seen: Vec<Vec2> = Vec::new();
                for &(c, r) in &g.marks {
                    if r * scale < 1.0 {
                        continue;
                    }
                    let p = to_sheet(c);
                    if seen.iter().any(|q| q.dist(p) < 0.1) {
                        continue;
                    }
                    seen.push(p);
                    let reach = r * scale + 3.0;
                    out.line(p - Vec2::new(reach, 0.0), p + Vec2::new(reach, 0.0), Style::Chain);
                    out.line(p - Vec2::new(0.0, reach), p + Vec2::new(0.0, reach), Style::Chain);
                }
                // Notes along lines (bend notes), a little above them.
                for (p, angle, text) in &g.notes {
                    let up = Vec2::new(-angle.to_radians().sin(), angle.to_radians().cos());
                    out.text(to_sheet(*p) + up * 1.0, 2.5, *angle, text.clone(), Align::Center);
                }
                if !min.is_finite() {
                    min = at - Vec2::new(5.0, 5.0);
                    max = at + Vec2::new(5.0, 5.0);
                }
                // A section or detail view carries its name below it.
                if !v.label.is_empty() {
                    let name = match &v.kind {
                        ViewKind::Section { .. } => format!("{0}–{0}", v.label),
                        ViewKind::Detail { .. } => format!("{} ({})", v.label, scale_text(scale)),
                        _ => v.label.clone(),
                    };
                    out.text(Vec2::new((min.x + max.x) * 0.5, min.y - 8.0), 5.0, 0.0, name, Align::Center);
                }
                let snaps = g.snaps.iter().map(|sn| Snap { at: to_sheet(sn.at), ..sn.clone() }).collect();
                let points = g.points.iter().filter(|x| v.hidden || !x.hidden).map(|x| PickPoint { at: to_sheet(x.at), ..x.clone() }).collect();
                let edges = g
                    .edges
                    .iter()
                    .filter(|x| v.hidden || !x.hidden)
                    .map(|x| PickEdge { a: to_sheet(x.a), b: x.b.map(&to_sheet), r: x.r.map(|r| r * scale), ..x.clone() })
                    .collect();
                drawn.views.push(ViewPlaced { id: v.id, min, max, scale, basis, snaps, points, edges });
                bases.insert(v.id, basis);
            }
            Err(e) => drawn.errors.push((v.id, e)),
        }
    }
    // What the views mark on their parents: cutting lines and detail circles.
    for v in &d.views {
        let (Some(pv), parent_id) = (match &v.kind {
            ViewKind::Section { parent, .. } | ViewKind::Detail { parent, .. } => (drawn.views.iter().find(|x| x.id == *parent).cloned(), *parent),
            _ => (None, 0),
        }) else {
            continue;
        };
        let _ = parent_id;
        let pat = |p: [f64; 2]| {
            let pv_at = d.view(pv.id).map(|x| Vec2::new(x.at[0], x.at[1])).unwrap_or(Vec2::ZERO);
            pv_at + Vec2::new(p[0], p[1]) * pv.scale
        };
        match &v.kind {
            ViewKind::Section { a, b, flip, .. } => {
                let (pa, pb) = (pat(*a), pat(*b));
                let Some(along) = (pb - pa).normalized() else { continue };
                // The arrows' direction on the parent sheet: the plane's normal seen in the parent.
                let mut n = Vec2::new(along.y, -along.x);
                if *flip {
                    n = n * -1.0;
                }
                let (ea, eb) = (pa - along * 4.0, pb + along * 4.0);
                out.line(ea, eb, Style::Chain);
                out.line(ea, ea + along * 6.0, Style::Wide);
                out.line(eb - along * 6.0, eb, Style::Wide);
                for e in [ea + along * 2.0, eb - along * 2.0] {
                    out.line(e, e + n * 8.0, Style::Thin);
                    out.arrow(e + n * 8.0, n);
                    out.text(e + n * 10.0 + Vec2::new(0.0, 0.0), 5.0, 0.0, v.label.clone(), Align::Center);
                }
            }
            ViewKind::Detail { center, radius, .. } => {
                let c = pat(*center);
                let r = radius * pv.scale;
                for i in 0..48 {
                    let (t0, t1) = (i as f64 / 48.0 * std::f64::consts::TAU, (i + 1) as f64 / 48.0 * std::f64::consts::TAU);
                    out.line(c + Vec2::new(t0.cos(), t0.sin()) * r, c + Vec2::new(t1.cos(), t1.sin()) * r, Style::Thin);
                }
                out.text(c + Vec2::new(r * 0.75 + 2.0, r * 0.75 + 2.0), 5.0, 0.0, v.label.clone(), Align::Left);
            }
            _ => {}
        }
    }
    // Dimensions: their model points, where they are now, projected in their view.
    for dim in &d.dims {
        let Some(pv) = drawn.views.iter().find(|x| x.id == dim.view) else {
            drawn.errors.push((dim.id, "its view is missing".into()));
            continue;
        };
        let at = d.view(pv.id).map(|x| Vec2::new(x.at[0], x.at[1])).unwrap_or(Vec2::ZERO);
        let pts: Vec<(Vec2, Option<f64>, Vec3)> = dim
            .refs
            .iter()
            .map(|r| {
                let (p, radius) = resolve(&m, r, &pv.basis);
                (at + pv.basis.project(p) * pv.scale, radius, p)
            })
            .collect();
        // A virtual sharp: thin lines carry the edges on from the round to the corner, and a dot marks it.
        for r in dim.refs.iter().filter(|r| r.at == "sharp") {
            let Some((s0, s1, x)) =
                m.bodies.iter().zip(&m.circles).find(|((n, ..), _)| *n == r.body).and_then(|((_, _, me), c)| virtual_sharp(c, me, r.edge?))
            else {
                continue;
            };
            let sp = |p: Vec3| at + pv.basis.project(p) * pv.scale;
            let xs = sp(x);
            for e in [s0, s1] {
                let es = sp(e);
                let past = (xs - es).normalized().map(|u| xs + u * 1.5).unwrap_or(xs);
                out.line(es, past, Style::Thin);
            }
            out.prims.push(Prim::Fill { points: (0..8).map(|k| xs + Vec2::new((k as f64 * 0.785).cos(), (k as f64 * 0.785).sin()) * 0.5).collect() });
        }
        let before = out.prims.len();
        let place = dim.at.map(|p| at + Vec2::new(p[0], p[1]) * pv.scale);
        draw_dim(&mut out, dim, &pts, pv.scale, place, d.dim_text());
        if let Some(Prim::Text { at, .. }) = out.prims.get(before..).and_then(|ps| ps.iter().rev().find(|p| matches!(p, Prim::Text { .. }))) {
            drawn.dims.push((dim.id, *at));
        }
    }
    for n in &d.notes {
        for (i, line) in n.text.lines().enumerate().take(200) {
            out.text(Vec2::new(n.at[0], n.at[1] - i as f64 * n.height * 1.6), n.height, 0.0, line, Align::Left);
        }
    }
    drawn.prims = prims;
    drawn
}

/// `2:1`, `1:2` from a factor.
pub fn scale_text(s: f64) -> String {
    if s >= 1.0 { format!("{}:1", number(s)) } else { format!("1:{}", number(1.0 / s)) }
}

/// The model point nearest to a sheet point in a view (within `reach` mm): for a diameter or
/// radius only circle centres, measured to their circle.
pub(crate) fn pick(view: &ViewPlaced, at: Vec2, reach: f64, mode: PickMode) -> Option<Snap> {
    let dist = |s: &Snap| -> f64 {
        match (mode, s.radius, s.sharp) {
            (PickMode::Circles, Some(r), _) => ((s.at - at).len() - r * view.scale).abs(),
            (PickMode::Circles, None, _) => f64::INFINITY,
            (PickMode::Sharps, _, true) => (s.at - at).len(),
            (PickMode::Sharps, _, false) => f64::INFINITY,
            (PickMode::Points, _, false) => (s.at - at).len(),
            (PickMode::Points, _, true) => f64::INFINITY,
        }
    };
    view.snaps.iter().filter(|s| dist(s) <= reach).min_by(|a, b| dist(a).total_cmp(&dist(b))).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Vec3, b: Vec3) -> bool {
        (a - b).len() < 1e-9
    }

    #[test]
    fn projected_views_follow_the_projection_method() {
        let (r, u) = orientation("front").unwrap();
        let front = with_back(r, u, Vec3::ZERO);
        assert!(close(front.b, Vec3::new(0.0, -1.0, 0.0)), "the front view looks along +Y: {:?}", front.b);
        // First angle: the view to the right of the front one looks from the left (-X).
        let right_first = projected(&front, "right", Projection::First).unwrap();
        assert!(close(right_first.b, Vec3::new(-1.0, 0.0, 0.0)) && close(right_first.u, Vec3::Z), "{right_first:?}");
        // Third angle: from the right; the view below the front one in first angle is the top view.
        let right_third = projected(&front, "right", Projection::Third).unwrap();
        assert!(close(right_third.b, Vec3::X) && close(right_third.r, Vec3::Y), "{right_third:?}");
        let below_first = projected(&front, "below", Projection::First).unwrap();
        assert!(close(below_first.b, Vec3::Z) && close(below_first.r, Vec3::X) && close(below_first.u, Vec3::Y), "{below_first:?}");
        // Every basis is right-handed (right = up × back) and orthonormal.
        for b in [front, right_first, right_third, below_first, projected(&front, "iso", Projection::First).unwrap()] {
            assert!(close(b.u.cross(b.b), b.r) && (b.r.len() - 1.0).abs() < 1e-9 && b.r.dot(b.u).abs() < 1e-9, "{b:?}");
        }
    }

    #[test]
    fn a_camera_projects_like_its_basis() {
        for o in ["front", "back", "left", "right", "top", "bottom", "iso"] {
            let (r, u) = orientation(o).unwrap();
            let b = with_back(r, u, Vec3::new(1.0, 2.0, 3.0));
            let (cr, cu, cb) = b.camera(10.0).basis();
            assert!(close(cr, b.r) && close(cu, b.u) && close(cb, b.b), "{o}: {cr:?} {cu:?} {cb:?} vs {b:?}");
        }
    }

    #[test]
    fn hatching_fills_a_square_and_clipping_keeps_inside_a_circle() {
        let sq = [
            (Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0)),
            (Vec2::new(10.0, 0.0), Vec2::new(10.0, 10.0)),
            (Vec2::new(10.0, 10.0), Vec2::new(0.0, 10.0)),
            (Vec2::new(0.0, 10.0), Vec2::new(0.0, 0.0)),
        ];
        let h = hatch(&sq, 1.0);
        assert!(h.len() > 10);
        assert!(h.iter().all(|(a, b)| [a, b].iter().all(|p| p.x > -1e-6 && p.x < 10.0 + 1e-6 && p.y > -1e-6 && p.y < 10.0 + 1e-6)));
        // Lines at 45°.
        assert!(h.iter().all(|(a, b)| ((b.y - a.y) - (b.x - a.x)).abs() < 1e-9));
        let c = clip_circle(Vec2::new(-10.0, 0.0), Vec2::new(10.0, 0.0), Vec2::ZERO, 5.0).unwrap();
        assert!((c.0.x + 5.0).abs() < 1e-9 && (c.1.x - 5.0).abs() < 1e-9);
        assert!(clip_circle(Vec2::new(-10.0, 9.0), Vec2::new(10.0, 9.0), Vec2::ZERO, 5.0).is_none());
        assert_eq!(number(12.0), "12");
        assert_eq!(number(12.345), "12,35");
        assert_eq!(number(-0.001), "0");
        assert_eq!(scale_text(0.5), "1:2");
        assert_eq!(scale_text(2.0), "2:1");
    }
}
