//! A Fitting feature's pieces: their sizes from the feature's values, their bodies (made in the
//! fitting's own frame — the axis along Z, the branch toward +X — then placed), and their flat
//! patterns ([`crate::fitting`]).

use std::collections::BTreeMap;

use solidcraft_geom::{Loop2, Plane, Region2, Vec2, Vec3};
use solidcraft_kernel as kernel;
use solidcraft_kernel::{Body, BoolOp};

use crate::document::{FeatureKind, FittingType};
use crate::eval::val;
use crate::expr::{Expr, Kind, Value};
use crate::fitting::{Branch, Elbow, Mantle, Pattern};
use crate::{DocError, Result};

/// A fitting's sizes, worked out: middle-surface radii, the sheet's thickness, where it stands.
pub(crate) struct Sized {
    pub thickness: f64,
    pub shape: Shape,
    pub base: Vec3,
    pub axis: Vec3,
}

pub(crate) enum Shape {
    Body { mantle: Mantle, branch: Option<Branch> },
    Elbow(Elbow),
}

fn bad<T>(why: impl Into<String>) -> Result<T> {
    Err(DocError::Invalid(why.into()))
}

/// The sizes of a Fitting feature (`None` for another kind of feature).
pub(crate) fn sized(vals: &BTreeMap<String, Value>, kind: &FeatureKind) -> Result<Option<Sized>> {
    let FeatureKind::Fitting {
        base,
        axis,
        fitting,
        measured,
        thickness,
        diameter,
        top,
        height,
        shift,
        branch,
        branch_length,
        branch_at,
        branch_angle,
        branch_offset,
        bend,
        angle,
        segments,
        straight,
    } = kind
    else {
        return Ok(None);
    };
    let len = |e: &Expr| val(vals, e, Kind::Length);
    let opt = |e: &Option<Expr>, k: Kind, d: f64| e.as_ref().map_or(Ok(d), |e| val(vals, e, k));
    let t = len(thickness)?;
    if !(t.is_finite() && t > 0.0 && t < 1000.0) {
        return bad("the sheet's thickness must be positive");
    }
    let m = |d: f64| -> Result<f64> {
        let r = measured.mean(d, t) / 2.0;
        if !(r.is_finite() && r > 0.0) {
            return bad("a diameter must be larger than the sheet's thickness");
        }
        Ok(r)
    };
    let d0 = len(diameter)?;
    let base = crate::eval::point(vals, base)?;
    let axis = axis.0.normalized().ok_or_else(|| DocError::Invalid("the fitting's axis".into()))?;
    let shape = match fitting {
        FittingType::Elbow => {
            let seg = opt(segments, Kind::Unitless, 5.0)?;
            if !(seg.is_finite() && (2.0..=48.0).contains(&seg.round())) {
                return bad("an elbow has 2 to 48 segments");
            }
            let e = Elbow {
                radius: m(d0)?,
                bend: opt(bend, Kind::Length, d0 * 1.5)?,
                angle: opt(angle, Kind::Angle, std::f64::consts::FRAC_PI_2)?,
                segments: seg.round() as usize,
                straight: opt(straight, Kind::Length, 0.0)?,
            };
            e.check().map_err(DocError::Invalid)?;
            Shape::Elbow(e)
        }
        FittingType::Tee | FittingType::Cone => {
            let d1 = opt(top, Kind::Length, d0)?;
            let h = match height {
                Some(e) => len(e)?,
                None => return bad("the fitting needs its height"),
            };
            let mantle = Mantle { r0: m(d0)?, r1: m(d1)?, h, shift: opt(shift, Kind::Length, 0.0)? };
            mantle.check().map_err(DocError::Invalid)?;
            // a cone may have a branch into its mantle as well; a tee must
            let branch = match (fitting, branch) {
                (_, Some(bd)) => {
                    let b = Branch {
                        radius: m(len(bd)?)?,
                        length: opt(branch_length, Kind::Length, mantle.r0.max(mantle.r1) * 2.0)?,
                        at: opt(branch_at, Kind::Length, h / 2.0)?,
                        angle: opt(branch_angle, Kind::Angle, std::f64::consts::FRAC_PI_2)?,
                        offset: opt(branch_offset, Kind::Length, 0.0)?,
                    };
                    b.check(&mantle).map_err(DocError::Invalid)?;
                    Some(b)
                }
                (FittingType::Tee, None) => return bad("a tee needs its branch's diameter"),
                (_, None) => None,
            };
            Shape::Body { mantle, branch }
        }
    };
    Ok(Some(Sized { thickness: t, shape, base, axis }))
}

impl Sized {
    /// The pieces' names and flat patterns, in the order of the bodies.
    pub fn patterns(&self) -> Vec<(String, Pattern)> {
        match &self.shape {
            Shape::Body { mantle, branch: None } => vec![("body".into(), mantle.pattern(None))],
            Shape::Body { mantle, branch: Some(b) } => {
                vec![("body".into(), mantle.pattern(Some(&b.on_mantle(mantle)))), ("branch".into(), b.pattern(mantle))]
            }
            Shape::Elbow(e) => (0..e.segments).map(|k| (format!("segment {}", k + 1), e.pattern(k))).collect(),
        }
    }

    /// The pieces' bodies, placed.
    pub fn bodies(&self) -> Result<Vec<Body>> {
        let local = match &self.shape {
            Shape::Body { mantle, branch } => body_pieces(mantle, branch.as_ref(), self.thickness)?,
            Shape::Elbow(e) => elbow_pieces(e, self.thickness)?,
        };
        // the fitting's frame: its Z along the axis, its X square to it
        let k = self.axis;
        let pick = if k.cross(Vec3::X).len() > 1e-6 { Vec3::X } else { Vec3::Y };
        let x = (pick - k * k.dot(pick)).normalized().ok_or_else(|| DocError::Invalid("the fitting's axis".into()))?;
        let y = k.cross(x);
        let m = [[x.x, x.y, x.z, 0.0], [y.x, y.y, y.z, 0.0], [k.x, k.y, k.z, 0.0], [self.base.x, self.base.y, self.base.z, 1.0]];
        local.iter().map(|b| Ok(kernel::transform_matrix(b, m)?)).collect()
    }
}

fn some(b: Option<Body>, what: &str) -> Result<Body> {
    b.ok_or_else(|| DocError::Invalid(format!("the fitting's {what} came out empty")))
}

/// The solid between the mantle's two surfaces `ri(s)`…`ro(s)` (a pipe or a cone wall), or the
/// whole solid inside radius `ro` (`ri` None).
fn mantle_solid(m: &Mantle, out: f64, inn: Option<f64>) -> Result<Body> {
    // The sheet's thickness is square to the mantle, so across it (radially) a sloping wall is
    // thicker: by 1 / cos of its slope (an eccentric cone's, on the average round it).
    let lean = (m.r1 - m.r0).powi(2) + m.shift.powi(2) / 2.0;
    let k = (1.0 + lean / (m.h * m.h)).sqrt();
    let (out, inn) = (out * k, inn.map(|i| i * k));
    let (r0o, r1o) = (m.r0 + out, m.r1 + out);
    if m.shift.abs() < 1e-9 {
        // a body of revolution: exact pipes and cones
        let plane = Plane { origin: Vec3::ZERO, x: Vec3::X, y: Vec3::Z };
        let pts = match inn {
            Some(i) => vec![Vec2::new(m.r0 - i, 0.0), Vec2::new(r0o, 0.0), Vec2::new(r1o, m.h), Vec2::new(m.r1 - i, m.h)],
            None => vec![Vec2::new(0.0, 0.0), Vec2::new(r0o, 0.0), Vec2::new(r1o, m.h), Vec2::new(0.0, m.h)],
        };
        let region = Region2 { outer: Loop2::polygon(&pts), holes: Vec::new() };
        let mut b = kernel::revolve(&plane, &[region], Vec2::new(0.0, 0.0), Vec2::new(0.0, 1.0), std::f64::consts::TAU)?;
        return if b.len() == 1 { Ok(b.remove(0)) } else { bad("the fitting's body came out in pieces") };
    }
    // an eccentric cone: lofted between its two circles
    let ring = |r: f64, i: Option<f64>| Region2 {
        outer: Loop2::circle(Vec2::new(0.0, 0.0), r),
        holes: i.map(|i| vec![Loop2::circle(Vec2::new(0.0, 0.0), i)]).unwrap_or_default(),
    };
    let bottom = Plane { origin: Vec3::ZERO, x: Vec3::X, y: Vec3::Y };
    let top = Plane { origin: Vec3::new(m.shift, 0.0, m.h), x: Vec3::X, y: Vec3::Y };
    Ok(kernel::loft_regions(&[(bottom, ring(r0o, inn.map(|i| m.r0 - i))), (top, ring(r1o, inn.map(|i| m.r1 - i)))])?)
}

/// A pipe's wall from `base` along `dir`, `len` long.
fn tube(base: Vec3, dir: Vec3, r: f64, t: f64, len: f64) -> Result<Body> {
    let outer = kernel::cylinder(base, dir, r + t / 2.0, len)?;
    let inner = kernel::cylinder(base - dir * 1.0, dir, r - t / 2.0, len + 2.0)?;
    some(kernel::boolean(&outer, &inner, BoolOp::Cut)?, "pipe")
}

fn body_pieces(m: &Mantle, branch: Option<&Branch>, t: f64) -> Result<Vec<Body>> {
    let wall = mantle_solid(m, t / 2.0, Some(t / 2.0))?;
    let Some(b) = branch else { return Ok(vec![wall]) };
    let start = Vec3::new(0.0, b.offset, b.at);
    let (s, c) = b.angle.sin_cos();
    let dir = Vec3::new(s, 0.0, c);
    // the body with the branch's opening: the branch's bore through its wall
    let bore = kernel::cylinder(start, dir, b.radius - t / 2.0, b.length)?;
    let body = some(kernel::boolean(&wall, &bore, BoolOp::Cut)?, "body")?;
    // the branch set on the body: its pipe, less what lies inside the body's outside
    let pipe = tube(start, dir, b.radius, t, b.length)?;
    let solid = mantle_solid(m, t / 2.0, None)?;
    let branch = some(kernel::boolean(&pipe, &solid, BoolOp::Cut)?, "branch")?;
    Ok(vec![body, branch])
}

/// A mitred elbow's segments: each a straight pipe along the bend's tangent at its middle,
/// cut by the planes through the bend's axis halfway to its neighbours. The bend starts at
/// the origin along +Z and turns toward +X about an axis along Y through (bend, 0, 0).
fn elbow_pieces(e: &Elbow, t: f64) -> Result<Vec<Body>> {
    let arc = |u: f64| Vec3::new(e.bend * (1.0 - u.cos()), 0.0, e.bend * u.sin());
    let tangent = |u: f64| Vec3::new(u.sin(), 0.0, u.cos());
    let n = e.segments;
    let step = e.step();
    let big = (e.bend + e.radius) * 4.0 + e.straight * 2.0 + 10.0;
    // the half space on the side `normal` points to, its face through `at`
    let half = |at: Vec3, normal: Vec3| -> Result<Body> {
        let b = kernel::box_solid(Vec3::new(-big, -big, 0.0), Vec3::new(big, big, 2.0 * big))?;
        let v = Vec3::Y;
        let u = v.cross(normal);
        let m = [[u.x, u.y, u.z, 0.0], [v.x, v.y, v.z, 0.0], [normal.x, normal.y, normal.z, 0.0], [at.x, at.y, at.z, 1.0]];
        Ok(kernel::transform_matrix(&b, m)?)
    };
    let mut out = Vec::with_capacity(n);
    for k in 0..n {
        let mid = k as f64 * step;
        let lo = if k == 0 { 0.0 } else { mid - step / 2.0 };
        let hi = if k + 1 == n { e.angle } else { mid + step / 2.0 };
        let dir = tangent(mid);
        let pipe = tube(arc(mid) - dir * big / 2.0, dir, e.radius, t, big)?;
        let lo_at = arc(lo) - if k == 0 { tangent(lo) * e.straight } else { Vec3::ZERO };
        let hi_at = arc(hi) + if k + 1 == n { tangent(hi) * e.straight } else { Vec3::ZERO };
        let a = some(kernel::boolean(&pipe, &half(lo_at, tangent(lo))?, BoolOp::Intersect)?, "segment")?;
        out.push(some(kernel::boolean(&a, &half(hi_at, tangent(hi) * -1.0)?, BoolOp::Intersect)?, "segment")?);
    }
    Ok(out)
}

impl crate::Document {
    /// A Fitting feature's pieces: each one's name and flat pattern, in the order of the bodies
    /// it makes (a tee: its body, then its branch; an elbow: its segments from the start).
    pub fn fitting_patterns(&self, feature: u64) -> Result<Vec<(String, Pattern)>> {
        let f = self.features.iter().find(|f| f.id == feature).ok_or_else(|| DocError::Unknown(format!("feature {feature}")))?;
        let vals = self.evaluation_values();
        match sized(&vals, &f.kind)? {
            Some(s) => Ok(s.patterns()),
            None => bad("not a pipe fitting"),
        }
    }

    /// The sheet thickness of a Fitting feature.
    pub fn fitting_thickness(&self, feature: u64) -> Result<f64> {
        let f = self.features.iter().find(|f| f.id == feature).ok_or_else(|| DocError::Unknown(format!("feature {feature}")))?;
        match sized(&self.evaluation_values(), &f.kind)? {
            Some(s) => Ok(s.thickness),
            None => bad("not a pipe fitting"),
        }
    }
}
