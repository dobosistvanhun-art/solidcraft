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
use crate::fitting::{Branch, Collar, Elbow, Flange, Mantle, Pattern};
use crate::pipe_standards::{self as ps, Loose, WeldNeck};
use crate::{DocError, Result};

/// A fitting's sizes, worked out: middle-surface radii, the sheet's thickness, where it stands.
pub(crate) struct Sized {
    pub thickness: f64,
    pub shape: Shape,
    pub base: Vec3,
    pub axis: Vec3,
    /// A pipe thread to cut on the fitting: its designation, inside (a socket) or outside (a
    /// nipple), and how far it runs from the fitting's top end (`None`: all its length).
    pub thread: Option<FittingThread>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FittingThread {
    pub designation: String,
    pub inside: bool,
    pub length: Option<f64>,
    /// A point on the threaded face, near the end the thread runs from, in the fitting's frame.
    pub at: Vec3,
    /// Cut into the body, to be seen (a standard fitting's), or only marked (cosmetic).
    pub modeled: bool,
}

pub(crate) enum Shape {
    Body {
        mantle: Mantle,
        branch: Option<Branch>,
    },
    Elbow(Elbow),
    Flange(Flange),
    /// A standard weld neck flange (EN 1092-1 type 11).
    WeldNeck(WeldNeck),
    /// A standard loose flange (type 02) behind its plate collar (type 32).
    Loose(Loose),
    /// A turned tube (a threaded socket or a nipple): its outside and inside radii, its height,
    /// and a top turned down to a radius over a length (a threaded end's thread).
    Tube {
        outside: f64,
        inside: f64,
        height: f64,
        top: Option<(f64, f64)>,
    },
    Collar(Collar),
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
        bore,
        pitch,
        holes,
        hole,
        size,
        thread_length,
        width,
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
    // a standard fitting: every size from the standard's tables
    if let Some(name) = size.as_deref().map(str::trim).filter(|x| !x.is_empty())
        && let Some(s) = standard(*fitting, name, height.as_ref().map(len).transpose()?, base, axis)?
    {
        return Ok(Some(s));
    }
    let thread = size;
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
        FittingType::Flange => {
            let n = opt(holes, Kind::Unitless, 4.0)?;
            if !(n.is_finite() && (0.0..=256.0).contains(&n.round())) {
                return bad("a flange has 0 to 256 bolt holes");
            }
            let f = Flange {
                outside: d0 / 2.0,
                bore: opt(bore, Kind::Length, 0.0)? / 2.0,
                pitch: opt(pitch, Kind::Length, d0 * 0.75)? / 2.0,
                holes: n.round() as usize,
                hole: opt(hole, Kind::Length, d0 * 0.08)? / 2.0,
            };
            f.check().map_err(DocError::Invalid)?;
            Shape::Flange(f)
        }
        FittingType::Collar => {
            let h = match height {
                Some(e) => len(e)?,
                None => return bad("the collar's pipe needs its height"),
            };
            let c = Collar { radius: m(d0)?, height: h, bend: opt(bend, Kind::Length, t)?, width: opt(width, Kind::Length, d0 * 0.25)? };
            c.check(t).map_err(DocError::Invalid)?;
            Shape::Collar(c)
        }
        FittingType::Socket | FittingType::Nipple if thread.is_some() || *fitting == FittingType::Nipple => {
            let Some(name) = thread.as_deref().map(str::trim).filter(|x| !x.is_empty()) else {
                return bad("a threaded end needs its pipe thread, as G1/2");
            };
            let Some(spec) = crate::thread_spec::parse(name) else {
                return bad(format!("`{name}` is no thread this knows (G1/2, Rp1/2, R1/2, M20x1.5, …)"));
            };
            let h = match height {
                Some(e) => len(e)?,
                None => return bad("the fitting needs its length"),
            };
            if !(h.is_finite() && h > 0.0) {
                return bad("the fitting's length must be positive");
            }
            let inside_thread = *fitting == FittingType::Socket;
            let (outside, inside) = if inside_thread {
                // a socket: the thread's bore inside, its own outside diameter
                (d0 / 2.0, opt(bore, Kind::Length, spec.minor())? / 2.0)
            } else {
                // a nipple: the thread's major diameter outside, its bore inside
                (spec.major / 2.0, opt(bore, Kind::Length, spec.major - 2.0 * t)? / 2.0)
            };
            if !(inside > 0.0 && inside < outside && outside.is_finite()) {
                return bad("the fitting's bore must be inside its outside diameter");
            }
            if inside_thread && outside <= spec.major / 2.0 + 0.5 {
                return bad(format!("a {name} socket's outside must be larger than the thread's {:.2} mm", spec.major));
            }
            let length = match thread_length {
                Some(e) => {
                    let l = len(e)?;
                    if !(l.is_finite() && l > 0.0) {
                        return bad("the thread's length must be positive");
                    }
                    (l < h - 1e-9).then_some(l)
                }
                None => None,
            };
            let r = if inside_thread { inside } else { outside };
            let thread = FittingThread {
                designation: name.to_string(),
                inside: inside_thread,
                length,
                at: Vec3::new(r, 0.0, h - h.min(1.0) * 0.5),
                modeled: false,
            };
            return Ok(Some(Sized { thickness: t, shape: Shape::Tube { outside, inside, height: h, top: None }, base, axis, thread: Some(thread) }));
        }
        FittingType::Tee | FittingType::Cone | FittingType::Socket | FittingType::Nipple => {
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
    Ok(Some(Sized { thickness: t, shape, base, axis, thread: None }))
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
            Shape::Flange(f) => vec![("flange".into(), f.pattern())],
            // a loose flange and its collar are flat rings, cut as they are
            Shape::Loose(l) => vec![("flange".into(), loose_plate(l).pattern()), ("collar".into(), collar_plate(l).pattern())],
            // a collar is turned out of a pipe; a weld neck and a tube are turned or forged
            Shape::Collar(_) | Shape::WeldNeck(_) | Shape::Tube { .. } => Vec::new(),
        }
    }

    /// Each piece's sheet thickness, in the order of [`Sized::patterns`]: a loose flange's plate
    /// and its collar are cut from plates of their own.
    pub fn piece_thicknesses(&self) -> Vec<f64> {
        match &self.shape {
            Shape::Loose(l) => vec![l.thickness, l.collar_thickness],
            _ => vec![self.thickness; self.patterns().len()],
        }
    }

    /// Whether the patterns are exact developments (a collar's is a blank: it stretches).
    pub fn exact(&self) -> bool {
        !matches!(self.shape, Shape::Collar(_))
    }

    /// The fitting's pipe thread with the point on its face, in world coordinates.
    pub fn thread_at(&self) -> Result<Option<(FittingThread, Vec3)>> {
        let Some(th) = self.thread.clone() else { return Ok(None) };
        let m = self.frame()?;
        let p = th.at;
        let w = Vec3::new(
            m[0][0] * p.x + m[1][0] * p.y + m[2][0] * p.z + m[3][0],
            m[0][1] * p.x + m[1][1] * p.y + m[2][1] * p.z + m[3][1],
            m[0][2] * p.x + m[1][2] * p.y + m[2][2] * p.z + m[3][2],
        );
        Ok(Some((th, w)))
    }

    /// The fitting's frame: its Z along the axis, its X square to it, at its base.
    pub fn frame(&self) -> Result<[[f64; 4]; 4]> {
        let k = self.axis;
        let pick = if k.cross(Vec3::X).len() > 1e-6 { Vec3::X } else { Vec3::Y };
        let x = (pick - k * k.dot(pick)).normalized().ok_or_else(|| DocError::Invalid("the fitting's axis".into()))?;
        let y = k.cross(x);
        Ok([[x.x, x.y, x.z, 0.0], [y.x, y.y, y.z, 0.0], [k.x, k.y, k.z, 0.0], [self.base.x, self.base.y, self.base.z, 1.0]])
    }

    /// The pieces' bodies, placed.
    pub fn bodies(&self) -> Result<Vec<Body>> {
        let local = match &self.shape {
            Shape::Body { mantle, branch } => body_pieces(mantle, branch.as_ref(), self.thickness)?,
            Shape::Elbow(e) => elbow_pieces(e, self.thickness)?,
            Shape::Flange(f) => vec![flange_plate(f, self.thickness)?],
            Shape::WeldNeck(w) => vec![weld_neck_solid(w)?],
            Shape::Loose(l) => {
                // the collar against the face, the flange behind it
                let collar = flange_plate_at(&collar_plate(l), 0.0, l.collar_thickness)?;
                let flange = flange_plate_at(&loose_plate(l), l.collar_thickness, l.collar_thickness + l.thickness)?;
                vec![flange, collar]
            }
            Shape::Tube { outside, inside, height, top: None } => vec![ring(*outside, *inside, 0.0, *height)?],
            Shape::Tube { outside, inside, height, top: Some((r, l)) } => {
                // turned down at its top to the thread's major diameter
                let z = height - l;
                let pts = [
                    Vec2::new(*inside, 0.0),
                    Vec2::new(*outside, 0.0),
                    Vec2::new(*outside, z),
                    Vec2::new(*r, z),
                    Vec2::new(*r, *height),
                    Vec2::new(*inside, *height),
                ];
                vec![revolved(Loop2::polygon(&pts).segs, "threaded end")?]
            }
            Shape::Collar(c) => vec![collar_solid(c, self.thickness)?],
        };
        let m = self.frame()?;
        local.iter().map(|b| Ok(kernel::transform_matrix(b, m)?)).collect()
    }
}

/// The solid of revolution of the closed profile `segs` (in the XZ plane: x the radius) about Z.
fn revolved(segs: Vec<solidcraft_geom::Seg2>, what: &str) -> Result<Body> {
    let plane = Plane { origin: Vec3::ZERO, x: Vec3::X, y: Vec3::Z };
    let region = Region2 { outer: Loop2 { segs }, holes: Vec::new() };
    let mut b = kernel::revolve(&plane, &[region], Vec2::new(0.0, 0.0), Vec2::new(0.0, 1.0), std::f64::consts::TAU)?;
    if b.len() == 1 { Ok(b.remove(0)) } else { bad(format!("the fitting's {what} came out in pieces")) }
}

/// A ring (a tube) between radii `inside` and `outside` from height `z0` to `z1`.
fn ring(outside: f64, inside: f64, z0: f64, z1: f64) -> Result<Body> {
    let pts = [Vec2::new(inside, z0), Vec2::new(outside, z0), Vec2::new(outside, z1), Vec2::new(inside, z1)];
    revolved(Loop2::polygon(&pts).segs, "tube")
}

/// A plate flange `t` thick: its plate on the XY plane, up along Z.
fn flange_plate(f: &Flange, t: f64) -> Result<Body> {
    flange_plate_at(f, 0.0, t)
}

/// A plate flange (a ring of plate with its bolt holes) from height `z0` to `z1`.
fn flange_plate_at(f: &Flange, z0: f64, z1: f64) -> Result<Body> {
    let o = Vec2::new(0.0, 0.0);
    let mut holes: Vec<Loop2> = f.hole_centres().into_iter().map(|c| Loop2::circle(c, f.hole)).collect();
    if f.bore > 0.0 {
        holes.push(Loop2::circle(o, f.bore));
    }
    let region = Region2 { outer: Loop2::circle(o, f.outside), holes };
    let plane = Plane { origin: Vec3::ZERO, x: Vec3::X, y: Vec3::Y };
    let mut b = kernel::extrude(&plane, &[region], z0, z1)?;
    if b.len() == 1 { Ok(b.remove(0)) } else { bad("the flange came out in pieces") }
}

/// A loose flange's plate: a ring round its collar's pipe, its bolt holes.
fn loose_plate(l: &Loose) -> Flange {
    Flange { outside: l.outside / 2.0, bore: l.bore / 2.0, pitch: l.pitch / 2.0, holes: usize::from(l.holes), hole: l.hole / 2.0 }
}

/// A loose flange's plate collar (type 32): a ring on the pipe, its face the joint's.
fn collar_plate(l: &Loose) -> Flange {
    Flange { outside: l.collar / 2.0, bore: l.pipe / 2.0, pitch: 0.0, holes: 0, hole: 0.0 }
}

/// A weld neck flange (type 11), its raised face at the base, its neck up the axis: the plate
/// with its raised face, the hub narrowing to the neck, the neck's straight end, bored to the
/// pipe's inside; its bolt holes through the plate.
fn weld_neck_solid(w: &WeldNeck) -> Result<Body> {
    use solidcraft_geom::Seg2;
    let rb = w.neck / 2.0 - w.wall;
    let p = |x: f64, z: f64| Vec2::new(x, z);
    let pts = [
        p(rb, 0.0),
        p(w.face / 2.0, 0.0),
        p(w.face / 2.0, w.face_height),
        p(w.outside / 2.0, w.face_height),
        p(w.outside / 2.0, w.thickness),
        p(w.hub / 2.0, w.thickness),
        p(w.neck / 2.0, w.length - w.straight),
        p(w.neck / 2.0, w.length),
        p(rb, w.length),
    ];
    let segs: Vec<Seg2> = (0..pts.len()).map(|k| Seg2::Line { a: pts[k], b: pts[(k + 1) % pts.len()] }).collect();
    let mut body = revolved(segs, "weld neck")?;
    // the bolt holes: through the plate, from its raised face's height to its back
    let f = Flange { outside: w.outside / 2.0, bore: 0.0, pitch: w.pitch / 2.0, holes: usize::from(w.holes), hole: w.hole / 2.0 };
    let regions: Vec<Region2> = f.hole_centres().into_iter().map(|c| Region2 { outer: Loop2::circle(c, f.hole), holes: Vec::new() }).collect();
    let plane = Plane { origin: Vec3::ZERO, x: Vec3::X, y: Vec3::Y };
    for tool in kernel::extrude(&plane, &regions, -1.0, w.thickness + 1.0)? {
        body = some(kernel::boolean(&body, &tool, BoolOp::Cut)?, "weld neck")?;
    }
    Ok(body)
}

/// A standard fitting from its size's designation (see [`ps::designation`]); `None` when the
/// designation names no standard size of this kind of fitting (a fitting of its own sizes).
fn standard(fitting: FittingType, name: &str, height: Option<f64>, base: Vec3, axis: Vec3) -> Result<Option<Sized>> {
    let d = ps::designation(name);
    let length = || -> Result<f64> {
        match height {
            Some(h) if h.is_finite() && h > 0.0 => Ok(h),
            Some(_) => bad("the fitting's length must be positive"),
            None => bad("the fitting needs its length"),
        }
    };
    let unknown = |what: &str| -> Result<Option<Sized>> { bad(format!("`{name}` is no standard {what} this knows")) };
    let done = |t: f64, shape: Shape, thread: Option<FittingThread>| Ok(Some(Sized { thickness: t, shape, base, axis, thread }));
    match fitting {
        FittingType::Flange => {
            let (Some(pn), Some(dn)) = (d.pn, d.dn) else { return Ok(None) };
            match d.kind.as_str() {
                "11" => match ps::weld_neck(pn, dn, d.standard) {
                    Some(w) => done(w.thickness, Shape::WeldNeck(w), None),
                    None => unknown("weld neck flange (EN 1092-1 type 11)"),
                },
                "02" => match ps::loose(pn, dn, d.standard) {
                    Some(l) => done(l.thickness, Shape::Loose(l), None),
                    None => unknown("loose flange (EN 1092-1 type 02)"),
                },
                "" | "01" => match crate::flange_catalog::size(pn, dn) {
                    Some(z) => {
                        let holes = if d.standard == ps::Standard::Din && dn == 65 && matches!(pn, 10 | 16) { 4 } else { z.holes };
                        let f = Flange {
                            outside: z.outside / 2.0,
                            bore: z.bore / 2.0,
                            pitch: z.pitch / 2.0,
                            holes: usize::from(holes),
                            hole: z.hole / 2.0,
                        };
                        done(z.thickness, Shape::Flange(f), None)
                    }
                    None => unknown("plate flange (EN 1092-1 type 01)"),
                },
                _ => unknown("flange"),
            }
        }
        // a lapped pipe end (type 33): the pipe turned out to the collar's face, its wall the
        // series' (type 11's neck wall)
        FittingType::Collar => {
            let (Some(pn), Some(dn)) = (d.pn, d.dn) else { return Ok(None) };
            let (Some(l), Some(w)) = (ps::loose(pn, dn, d.standard), ps::weld_neck(16, dn, ps::Standard::En)) else {
                return unknown("lapped pipe end (EN 1092-1 type 33)");
            };
            let t = w.wall;
            let c = Collar { radius: (l.pipe - t) / 2.0, height: length()?, bend: t, width: (l.collar - l.pipe) / 2.0 };
            c.check(t).map_err(DocError::Invalid)?;
            done(t, Shape::Collar(c), None)
        }
        // a threaded socket (EN 10241), Rp inside all through
        FittingType::Socket => {
            // of its own sizes, a socket keeps its pipe thread (`G2`): a standard one its form
            let (Some(size), true) = (d.thread, matches!(d.kind.as_str(), "full" | "half")) else { return Ok(None) };
            let Some(k) = ps::socket(size) else { return unknown("socket (EN 10241)") };
            let thread = ps::thread_name("Rp", size);
            let Some(spec) = crate::thread_spec::parse(&thread) else { return unknown("socket thread") };
            let h = if d.kind == "half" { k.half } else { k.full };
            let (outside, inside) = (k.outside / 2.0, spec.minor() / 2.0);
            let th = FittingThread { designation: thread, inside: true, length: None, at: Vec3::new(inside, 0.0, h / 2.0), modeled: modeled(&spec) };
            done(outside - inside, Shape::Tube { outside, inside, height: h, top: None }, Some(th))
        }
        // a threaded end: medium pipe (EN 10255) with its R thread at its top
        FittingType::Nipple => {
            // of its own sizes, a threaded end keeps its pipe thread (`G1/2`): a standard one its size
            let (Some(size), true) = (d.thread, name.starts_with(|c: char| c.is_ascii_digit())) else { return Ok(None) };
            let Some(pipe) = ps::pipe(size) else { return unknown("threaded end (EN 10255)") };
            let h = length()?;
            if h <= pipe.thread {
                return bad(format!("a {size} threaded end must be longer than its thread, {} mm", pipe.thread));
            }
            // R (ISO 7-1, taper 1:16) is cut as its parallel twin of the same pitch and form (G):
            // over a thread's length the taper is a few tenths of a millimetre, and the kernel cuts
            // a parallel thread reliably
            let thread = ps::thread_name("G", size);
            let Some(spec) = crate::thread_spec::parse(&thread) else { return unknown("threaded end's thread") };
            let (outside, inside) = (pipe.outside / 2.0, pipe.outside / 2.0 - pipe.wall);
            // turned down to the thread's major diameter a pitch further than the thread runs, so
            // the thread runs out before the shoulder
            let major = (spec.major / 2.0).min(outside);
            let top = (major < outside - 1e-6).then_some((major, pipe.thread + spec.pitch));
            if h <= pipe.thread + spec.pitch {
                return bad(format!("a {size} threaded end must be longer than its thread, {} mm", pipe.thread));
            }
            let th = FittingThread {
                designation: thread,
                inside: false,
                length: Some(pipe.thread),
                at: Vec3::new(major, 0.0, h - pipe.thread / 2.0),
                modeled: modeled(&spec),
            };
            done(pipe.wall, Shape::Tube { outside, inside, height: h, top }, Some(th))
        }
        _ => Ok(None),
    }
}

/// A standard fitting's thread is cut into the model up to 2" (G2, 59.6 mm): a larger one takes
/// minutes to cut, and is shown as a cosmetic thread (the same designation and length).
fn modeled(spec: &crate::thread_spec::ThreadSpec) -> bool {
    spec.major <= 60.0
}

/// A pipe with its top turned out into a collar: the wall up to the bend, round the bend (a
/// quarter torus, exact) and the collar flat across the top.
fn collar_solid(c: &Collar, t: f64) -> Result<Body> {
    use solidcraft_geom::Seg2;
    use std::f64::consts::FRAC_PI_2;
    let (ri, ro) = (c.radius - t / 2.0, c.radius + t / 2.0);
    // the bend's centre: out from the pipe's outside by its inside radius, at the pipe's top
    let centre = Vec2::new(ro + c.bend, c.height);
    let (rb_in, rb_out) = (c.bend, c.bend + t);
    let rim = ro + c.width;
    let top = c.height + rb_out;
    let line = |a: Vec2, b: Vec2| Seg2::Line { a, b };
    let mut segs = vec![line(Vec2::new(ri, 0.0), Vec2::new(ro, 0.0)), line(Vec2::new(ro, 0.0), Vec2::new(ro, c.height))];
    if rb_in > 1e-9 {
        // the bend's inside, from the pipe's outside round to the collar's underside
        segs.push(Seg2::Arc { center: centre, radius: rb_in, start: std::f64::consts::PI, sweep: -FRAC_PI_2 });
    }
    segs.push(line(Vec2::new(ro + c.bend, c.height + rb_in), Vec2::new(rim, c.height + rb_in)));
    segs.push(line(Vec2::new(rim, c.height + rb_in), Vec2::new(rim, top)));
    segs.push(line(Vec2::new(rim, top), Vec2::new(ro + c.bend, top)));
    // the bend's outside, back down to the pipe's inside
    segs.push(Seg2::Arc { center: centre, radius: rb_out, start: FRAC_PI_2, sweep: FRAC_PI_2 });
    segs.push(line(Vec2::new(ri, c.height), Vec2::new(ri, 0.0)));
    revolved(segs, "collar")
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

    /// Each piece's sheet thickness of a Fitting feature, in the order of its patterns.
    pub fn fitting_piece_thicknesses(&self, feature: u64) -> Result<Vec<f64>> {
        let f = self.features.iter().find(|f| f.id == feature).ok_or_else(|| DocError::Unknown(format!("feature {feature}")))?;
        match sized(&self.evaluation_values(), &f.kind)? {
            Some(s) => Ok(s.piece_thicknesses()),
            None => bad("not a pipe fitting"),
        }
    }

    /// Whether a Fitting feature's flat patterns are exact developments.
    pub fn fitting_exact(&self, feature: u64) -> Result<bool> {
        let f = self.features.iter().find(|f| f.id == feature).ok_or_else(|| DocError::Unknown(format!("feature {feature}")))?;
        match sized(&self.evaluation_values(), &f.kind)? {
            Some(s) => Ok(s.exact()),
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
