//! Pipe fittings: of sheet a tee, a cone (concentric or eccentric, a branch into its mantle if
//! wanted), a mitred elbow, a socket and a collar (bördel), each piece a body with its flat
//! pattern; a plate flange (EN 1092-1 type 01 sizes, or any), cut flat; a threaded socket and a
//! threaded end (turned, with a pipe thread); the patterns and their DXF export, one file a
//! piece.

use serde_json::{Value, json};
use solidcraft_doc::expr::{Expr, Kind};
use solidcraft_doc::fitting::{Measured, Pattern};
use solidcraft_doc::pipe_standards::{self as ps, Standard};
use solidcraft_doc::{Dir, FeatureKind, FittingType};
use solidcraft_geom::{Vec2, Vec3};

use super::CommandSpec;
use super::features::{add_feature, axis_dir_param, check_expr, point_param};
use crate::params::{bad, expr, str_};
use crate::schema::features::FeatureResult;
use crate::schema::fitting::{
    CollarParams, ConeParams, ElbowParams, FittingPatternParams, FittingPatternResult, FlangeParams, NippleParams, SocketParams, TeeParams,
};
use crate::schema::{TypedContract, output_schema_for};
use crate::{EngineError, Result, Session};

const TEE_CONTRACT: TypedContract<TeeParams> = TypedContract::new();
const CONE_CONTRACT: TypedContract<ConeParams> = TypedContract::new();
const ELBOW_CONTRACT: TypedContract<ElbowParams> = TypedContract::new();
const PATTERN_CONTRACT: TypedContract<FittingPatternParams> = TypedContract::new();
const FLANGE_CONTRACT: TypedContract<FlangeParams> = TypedContract::new();
const SOCKET_CONTRACT: TypedContract<SocketParams> = TypedContract::new();
const NIPPLE_CONTRACT: TypedContract<NippleParams> = TypedContract::new();
const COLLAR_CONTRACT: TypedContract<CollarParams> = TypedContract::new();

pub static COMMANDS: &[CommandSpec] = &[
    CommandSpec::new("fitting.collar", "Collar", crate::checked!("fitting.collar", collar))
        .at("SHEET METAL", "FITTINGS")
        .icon("lip")
        .contract(&COLLAR_CONTRACT)
        .output(output_schema_for::<FeatureResult>)
        .params(
            "pn? (10 | 16, default 16), dn (10…600), standard? (EN | ISO | DIN), height (the pipe up to the collar): an EN 1092-1 \
             type 33 lapped pipe end for a type 02 loose flange, its pipe, wall and collar face from the tables; or of its own sizes: \
             diameter, height, thickness, width?, bend?, measured?; base?, axis?, name?",
        ),
    CommandSpec::new("fitting.cone", "Cone Fitting", crate::checked!("fitting.cone", cone))
        .at("SHEET METAL", "FITTINGS")
        .icon("cone")
        .contract(&CONE_CONTRACT)
        .output(output_schema_for::<FeatureResult>)
        .params(
            "diameter (bottom), top, height, thickness; shift? (the top's centre moved along the fitting's X: an eccentric cone); \
             branch? (a branch pipe's diameter, set into the mantle) with branch_length?, branch_at?, branch_angle?, branch_offset? as fitting.tee's; \
             measured?: outside (default) | inside | middle; base?, axis?, name? — a cone of sheet with an exact flat pattern \
             (fitting.patterns, fitting.export_dxf), its seam opposite the branch",
        ),
    CommandSpec::new("fitting.elbow", "Elbow", crate::checked!("fitting.elbow", elbow))
        .at("SHEET METAL", "FITTINGS")
        .icon("sweep")
        .contract(&ELBOW_CONTRACT)
        .output(output_schema_for::<FeatureResult>)
        .params(
            "diameter, thickness; angle? (default 90 deg); segments? (2…48 counting the two half ones at the ends, default 5); \
             bend? (the centre line's radius, default 1.5 × diameter); straight? (at each end, default 0); measured?; base? (the start's centre), \
             axis? (the start's direction, default Z; it bends toward the fitting's X), name? — a mitred elbow, one body a segment, \
             each with its flat pattern, its seam on the inside of the bend",
        ),
    CommandSpec::new("fitting.export_dxf", "Export Fitting Patterns as DXF", crate::checked!("fitting.export_dxf", export_dxf))
        .icon("dxf")
        .noundo()
        .contract(&PATTERN_CONTRACT)
        .output(output_schema_for::<FittingPatternResult>)
        .params(
            "feature? (default: the newest fitting), piece? (body | branch | segment N, or its number; default all), path? (a file a piece, \
             `<path>-<piece>.dxf`; without it the DXF texts) → DXF (AutoCAD 2000, mm) of each piece's flat pattern on the sheet's middle surface: \
             layers OUTLINE, CUT (a branch's opening) and MARK (dashed: the generators to roll by, an elbow's middle line)",
        ),
    CommandSpec::new("fitting.flange", "Flange (EN 1092-1)", crate::checked!("fitting.flange", flange))
        .at("SHEET METAL", "FITTINGS")
        .icon("pattern_circ")
        .contract(&FLANGE_CONTRACT)
        .output(output_schema_for::<FeatureResult>)
        .params(
            "type? (01 plate, the default | 02 loose, with its type 32 collar | 11 weld neck), pn? (default 16), dn (10…600), \
             standard? (EN, the default | ISO: the same | DIN: DN 65 PN 10/16 with 4 holes): every size from EN 1092-1's tables; a plate \
             flange may instead take its own: diameter, bore? (0: blind), bolt_circle, holes, hole, thickness, blind?; base? (the \
             centre of its face), axis? (along its neck), name? — plate flanges and loose flanges with their collars cut flat \
             (fitting.export_dxf)",
        ),
    CommandSpec::new("fitting.nipple", "Threaded End", crate::checked!("fitting.nipple", nipple))
        .at("SHEET METAL", "FITTINGS")
        .icon("thread")
        .contract(&NIPPLE_CONTRACT)
        .output(output_schema_for::<FeatureResult>)
        .params(
            "size (a thread size, 1/4…6), height (its length); base?, axis? (towards the thread), name? — medium steel pipe \
             (EN 10255) with its R thread (ISO 7-1) cut at its top; or of its own sizes: thread (G1/2, …), height, thickness, \
             thread_length?, bore?",
        ),
    CommandSpec::new("fitting.patterns", "Fitting Flat Patterns", crate::checked!("fitting.patterns", patterns))
        .icon("flat")
        .noundo()
        .doc_only()
        .contract(&PATTERN_CONTRACT)
        .output(output_schema_for::<FittingPatternResult>)
        .params("feature?, piece? → each piece's flat pattern: its size, area, outline, holes and marked lines (mm)"),
    CommandSpec::new("fitting.socket", "Socket", crate::checked!("fitting.socket", socket))
        .at("SHEET METAL", "FITTINGS")
        .icon("cylinder")
        .contract(&SOCKET_CONTRACT)
        .output(output_schema_for::<FeatureResult>)
        .params(
            "size (a thread size, 3/8…6), form? (full, the default | half); base?, axis?, name? — an EN 10241 socket, its Rp \
             thread cut inside; or of its own sizes: diameter, height, thickness, measured? (of sheet, with its flat pattern), \
             thread?, thread_length?, bore? (turned)",
        ),
    CommandSpec::new("fitting.tee", "Tee", crate::checked!("fitting.tee", tee))
        .at("SHEET METAL", "FITTINGS")
        .icon("pipe")
        .contract(&TEE_CONTRACT)
        .output(output_schema_for::<FeatureResult>)
        .params(
            "diameter, height, branch (its diameter), thickness; top? (the body a cone: its top diameter); shift? (an eccentric cone's); \
             branch_length? (from the body's axis, default twice its radius), branch_at? (the height its axis meets the body's, default half), \
             branch_angle? (from the body's axis, default 90 deg), branch_offset? (off-centre); measured?: outside (default) | inside | middle; \
             base?, axis? (the branch points along the fitting's X), name? — two bodies, the body with its opening and the branch, \
             each with an exact flat pattern (fitting.patterns, fitting.export_dxf), the seams opposite the cut",
        ),
];

/// A length (`kind`) the call gives, checked against the design.
fn given(s: &Session, p: &Value, key: &str, kind: Kind, cmd: &str) -> Result<Option<Expr>> {
    match expr(p, key).filter(|e| !e.trim().is_empty()) {
        Some(e) => {
            check_expr(s, &e, kind, cmd, key)?;
            Ok(Some(e))
        }
        None => Ok(None),
    }
}

fn needed(s: &Session, p: &Value, key: &str, kind: Kind, cmd: &str) -> Result<Expr> {
    given(s, p, key, kind, cmd)?.ok_or_else(|| bad(cmd, format!("give the `{key}`")))
}

fn measured(p: &Value, cmd: &str) -> Result<Measured> {
    match str_(p, "measured") {
        None => Ok(Measured::Outside),
        Some(m) => Measured::parse(m).ok_or_else(|| bad(cmd, "`measured` must be outside, inside or middle")),
    }
}

fn add(s: &mut Session, p: &Value, fitting: FittingType, cmd: &str) -> Result<Value> {
    let l = Kind::Length;
    let base = point_param(s, p, "base", cmd)?.unwrap_or_else(|| solidcraft_doc::point_expr(Vec3::ZERO));
    let axis = axis_dir_param(p, cmd)?;
    let t = FittingType::Tee;
    let thread = str_(p, "thread").map(|x| x.trim().to_string()).filter(|x| !x.is_empty());
    if let Some(name) = &thread
        && solidcraft_doc::thread_spec::parse(name).is_none()
    {
        return Err(bad(cmd, format!("unknown thread `{name}` ({})", solidcraft_doc::thread_spec::HELP)));
    }
    let diameter = match (fitting, &thread) {
        // a nipple's outside is its thread's
        (FittingType::Nipple, Some(name)) => {
            let major = solidcraft_doc::thread_spec::parse(name).map_or(0.0, |t| t.major);
            Expr::new(format!("{} mm", round(major)))
        }
        (FittingType::Nipple, None) => return Err(bad(cmd, "give the `thread`, as G1/2")),
        _ => needed(s, p, "diameter", l, cmd)?,
    };
    let height = match fitting {
        FittingType::Elbow | FittingType::Flange => None,
        _ => Some(needed(s, p, "height", l, cmd)?),
    };
    let measured = match (fitting, str_(p, "measured")) {
        (FittingType::Socket, None) => Measured::Inside,
        _ => measured(p, cmd)?,
    };
    let kind = FeatureKind::Fitting {
        base,
        axis: Dir(axis),
        fitting,
        measured,
        thickness: needed(s, p, "thickness", l, cmd)?,
        diameter,
        top: if fitting == FittingType::Cone { Some(needed(s, p, "top", l, cmd)?) } else { given(s, p, "top", l, cmd)? },
        height,
        shift: given(s, p, "shift", l, cmd)?,
        branch: if fitting == t { Some(needed(s, p, "branch", l, cmd)?) } else { given(s, p, "branch", l, cmd)? },
        branch_length: given(s, p, "branch_length", l, cmd)?,
        branch_at: given(s, p, "branch_at", l, cmd)?,
        branch_angle: given(s, p, "branch_angle", Kind::Angle, cmd)?,
        branch_offset: given(s, p, "branch_offset", l, cmd)?,
        bend: given(s, p, "bend", l, cmd)?,
        angle: given(s, p, "angle", Kind::Angle, cmd)?,
        segments: given(s, p, "segments", Kind::Unitless, cmd)?,
        straight: given(s, p, "straight", l, cmd)?,
        bore: given(s, p, "bore", l, cmd)?,
        pitch: given(s, p, "bolt_circle", l, cmd)?,
        holes: given(s, p, "holes", Kind::Unitless, cmd)?,
        hole: given(s, p, "hole", l, cmd)?,
        size: thread,
        thread_length: given(s, p, "thread_length", l, cmd)?,
        width: given(s, p, "width", l, cmd)?,
    };
    add_feature(s, p, kind)
}

/// A standard fitting: its size's designation (every size from the standard's tables), its
/// outside diameter and wall shown as its own (the timeline's), its length where it has one.
fn add_standard(s: &mut Session, p: &Value, fitting: FittingType, size: String, diameter: f64, thickness: f64, cmd: &str) -> Result<Value> {
    let base = point_param(s, p, "base", cmd)?.unwrap_or_else(|| solidcraft_doc::point_expr(Vec3::ZERO));
    let axis = axis_dir_param(p, cmd)?;
    let height = match fitting {
        FittingType::Nipple | FittingType::Collar => Some(needed(s, p, "height", Kind::Length, cmd)?),
        _ => None,
    };
    let mm = |v: f64| Expr::new(format!("{} mm", round(v)));
    let kind = FeatureKind::Fitting {
        base,
        axis: Dir(axis),
        fitting,
        measured: Measured::Outside,
        thickness: mm(thickness),
        diameter: mm(diameter),
        top: None,
        height,
        shift: None,
        branch: None,
        branch_length: None,
        branch_at: None,
        branch_angle: None,
        branch_offset: None,
        bend: None,
        angle: None,
        segments: None,
        straight: None,
        bore: None,
        pitch: None,
        holes: None,
        hole: None,
        size: Some(size),
        thread_length: None,
        width: None,
    };
    add_feature(s, p, kind)
}

/// A whole number given as a number or as text.
fn whole(v: &Value) -> Option<u16> {
    v.as_u64().or_else(|| v.as_str().and_then(|t| t.trim().parse::<u64>().ok())).map(|v| v.min(u64::from(u16::MAX)) as u16)
}

/// A standard flange's or collar's class and size: `pn` (default 16) and `dn`, checked against
/// the type's tables (`01`, `02`, `11`), and the standard (`EN`, `ISO`: EN 1092-1's sizes;
/// `DIN`: DIN 2501's).
fn class_and_size(p: &Value, kind: &str, cmd: &str) -> Result<(u16, u16, Standard)> {
    let pn = p.get("pn").filter(|v| !v.is_null()).map_or(Some(16), whole).unwrap_or(0);
    let classes = ps::flange_classes(kind);
    if !classes.contains(&pn) {
        let have: Vec<String> = classes.iter().map(u16::to_string).collect();
        return Err(bad(cmd, format!("type {kind} comes in PN {}", have.join(", "))));
    }
    let dn = p.get("dn").and_then(whole).unwrap_or(0);
    let dns = ps::flange_sizes(kind, pn);
    if !dns.contains(&dn) {
        let have: Vec<String> = dns.iter().map(u16::to_string).collect();
        return Err(bad(cmd, format!("type {kind} PN {pn} has no DN {dn} (DN {})", have.join(", "))));
    }
    let standard = match str_(p, "standard").map(|t| t.trim().to_ascii_uppercase()).as_deref() {
        None | Some("" | "EN" | "ISO" | "EN 1092-1" | "ISO 7005-1") => Standard::En,
        Some("DIN" | "DIN 2501") => Standard::Din,
        Some(other) => return Err(bad(cmd, format!("unknown standard `{other}` (EN, ISO, DIN)"))),
    };
    Ok((pn, dn, standard))
}

/// A plate flange: the standard's sizes for `pn`, `dn`, each overridden by the call's own.
fn flange(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "fitting.flange";
    let kind = str_(p, "type").map(str::trim).unwrap_or("01");
    if !matches!(kind, "01" | "02" | "11") {
        return Err(bad(cmd, format!("unknown flange type `{kind}` (01 plate, 02 loose, 11 weld neck)")));
    }
    // a standard flange: every size from its tables (a plate flange may take its own instead)
    let own = ["diameter", "bore", "bolt_circle", "holes", "hole", "thickness", "blind"].iter().any(|k| p.get(*k).is_some_and(|v| !v.is_null()));
    if p.get("dn").is_some_and(|v| !v.is_null()) && !(kind == "01" && own) {
        let (pn, dn, standard) = class_and_size(p, kind, cmd)?;
        let (outside, thickness) = match kind {
            "11" => ps::weld_neck(pn, dn, standard).map(|w| (w.outside, w.thickness)),
            "02" => ps::loose(pn, dn, standard).map(|l| (l.outside, l.thickness)),
            _ => solidcraft_doc::flange_catalog::size(pn, dn).map(|z| (z.outside, z.thickness)),
        }
        .ok_or_else(|| bad(cmd, format!("type {kind} PN {pn} DN {dn}")))?;
        let din = if standard == Standard::Din { " DIN" } else { "" };
        return add_standard(s, p, FittingType::Flange, format!("{kind} PN{pn} DN{dn}{din}"), outside, thickness, cmd);
    }
    if kind != "01" {
        return Err(bad(cmd, format!("a type {kind} flange is made to its standard: give its `dn` (and `pn`)")));
    }
    let mut q = p.clone();
    let Some(m) = q.as_object_mut() else { return Err(bad(cmd, "the parameters must be an object")) };
    let pn = p.get("pn").filter(|v| !v.is_null()).map_or(Some(16), whole).unwrap_or(0);
    if !solidcraft_doc::flange_catalog::CLASSES.contains(&pn) {
        return Err(bad(cmd, "`pn` must be 6, 10 or 16"));
    }
    if let Some(dn) = p.get("dn").filter(|v| !v.is_null()) {
        let dn = whole(dn).unwrap_or(0);
        let Some(size) = solidcraft_doc::flange_catalog::size(pn, dn) else {
            let have: Vec<String> = solidcraft_doc::flange_catalog::sizes(pn).iter().map(u16::to_string).collect();
            return Err(bad(cmd, format!("PN {pn} has no DN {dn} (DN {})", have.join(", "))));
        };
        let mut fill = |k: &str, v: f64| {
            if m.get(k).is_none_or(|x| x.is_null() || x.as_str().is_some_and(|t| t.trim().is_empty())) {
                m.insert(k.into(), json!(v));
            }
        };
        fill("diameter", size.outside);
        fill("bore", size.bore);
        fill("bolt_circle", size.pitch);
        fill("holes", f64::from(size.holes));
        fill("hole", size.hole);
        fill("thickness", size.thickness);
    } else {
        for k in ["diameter", "bolt_circle", "holes", "hole", "thickness"] {
            if m.get(k).is_none_or(Value::is_null) {
                return Err(bad(cmd, format!("give the `dn` (with `pn`) or the `{k}`")));
            }
        }
    }
    if p.get("blind").and_then(Value::as_bool).unwrap_or(false) {
        m.remove("bore");
    }
    for k in ["pn", "dn", "blind", "type", "standard"] {
        m.remove(k);
    }
    add(s, &q, FittingType::Flange, cmd)
}

/// A socket: a standard threaded one (EN 10241, `size` and `form`), or one of sheet (or turned,
/// with a pipe thread inside) of its own sizes.
fn socket(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "fitting.socket";
    let Some(size) = str_(p, "size").map(str::trim).filter(|t| !t.is_empty()) else { return add(s, p, FittingType::Socket, cmd) };
    let Some(k) = ps::socket(size) else {
        return Err(bad(cmd, format!("no EN 10241 socket of size `{size}` ({})", ps::socket_sizes().join(", "))));
    };
    let form = str_(p, "form").map(str::trim).unwrap_or("full");
    if !matches!(form, "full" | "half") {
        return Err(bad(cmd, "`form` is full or half"));
    }
    let size = ps::thread_size(size).unwrap_or_default();
    let minor = solidcraft_doc::thread_spec::parse(&ps::thread_name("Rp", size)).map_or(0.0, |t| t.minor());
    add_standard(s, p, FittingType::Socket, format!("{size} {form}"), k.outside, (k.outside - minor) / 2.0, cmd)
}

/// A threaded end: medium steel pipe (EN 10255) of thread size `size`, `height` long, its R thread
/// (ISO 7-1) at its top.
fn nipple(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "fitting.nipple";
    let Some(size) = str_(p, "size").map(str::trim).filter(|t| !t.is_empty()) else { return add(s, p, FittingType::Nipple, cmd) };
    let Some(pipe) = ps::pipe(size) else {
        return Err(bad(cmd, format!("no EN 10255 pipe of thread size `{size}` ({})", ps::pipe_sizes().join(", "))));
    };
    add_standard(s, p, FittingType::Nipple, pipe.size.to_string(), pipe.outside, pipe.wall, cmd)
}

/// A collar (bördel): a standard lapped pipe end (EN 1092-1 type 33, by `pn` and `dn`, for a type
/// 02 loose flange), or one of its own sizes.
fn collar(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "fitting.collar";
    if p.get("dn").is_some_and(|v| !v.is_null()) {
        let (pn, dn, standard) = class_and_size(p, "02", cmd)?;
        let (Some(l), Some(w)) = (ps::loose(pn, dn, standard), ps::weld_neck(16, dn, Standard::En)) else {
            return Err(bad(cmd, format!("PN {pn} DN {dn}")));
        };
        let din = if standard == Standard::Din { " DIN" } else { "" };
        return add_standard(s, p, FittingType::Collar, format!("PN{pn} DN{dn}{din}"), l.pipe, w.wall, cmd);
    }
    add(s, p, FittingType::Collar, cmd)
}

fn tee(s: &mut Session, p: &Value) -> Result<Value> {
    add(s, p, FittingType::Tee, "fitting.tee")
}

fn cone(s: &mut Session, p: &Value) -> Result<Value> {
    add(s, p, FittingType::Cone, "fitting.cone")
}

fn elbow(s: &mut Session, p: &Value) -> Result<Value> {
    add(s, p, FittingType::Elbow, "fitting.elbow")
}

/// The fitting a call means: the one it names, else the newest.
fn fitting_feature(s: &Session, p: &Value, cmd: &str) -> Result<u64> {
    let id = match p.get("feature").filter(|v| !v.is_null()) {
        Some(v) => super::edit::feature_id(s, Some(v), cmd)?,
        None => s
            .doc
            .features
            .iter()
            .rev()
            .find(|f| matches!(f.kind, FeatureKind::Fitting { .. }))
            .map(|f| f.id)
            .ok_or_else(|| bad(cmd, "the design has no pipe fitting"))?,
    };
    match s.doc.feature(id).map(|f| &f.kind) {
        Some(FeatureKind::Fitting { .. }) => Ok(id),
        _ => Err(bad(cmd, format!("feature {id} is no pipe fitting"))),
    }
}

/// The pieces a call asks for, each with its pattern and the body it is in the model.
fn pieces(s: &Session, p: &Value, cmd: &str) -> Result<(u64, f64, Vec<(String, Pattern, Option<String>, f64)>)> {
    let id = fitting_feature(s, p, cmd)?;
    let all = s.doc.fitting_patterns(id).map_err(|e| bad(cmd, e.to_string()))?;
    let thickness = s.doc.fitting_thickness(id).map_err(|e| bad(cmd, e.to_string()))?;
    let each = s.doc.fitting_piece_thicknesses(id).map_err(|e| bad(cmd, e.to_string()))?;
    // the bodies the feature made, in the order of its pieces
    let st = s.model.state();
    let bodies: Vec<String> = st.bodies.iter().filter(|b| b.feature == id).map(|b| b.name.clone()).collect();
    let mut out: Vec<(String, Pattern, Option<String>, f64)> =
        all.into_iter().enumerate().map(|(i, (name, pat))| (name, pat, bodies.get(i).cloned(), each.get(i).copied().unwrap_or(thickness))).collect();
    if let Some(want) = str_(p, "piece").map(|w| w.trim().to_lowercase()).filter(|w| !w.is_empty()) {
        let number = want.parse::<usize>().ok();
        let picked: Vec<_> =
            out.into_iter().enumerate().filter(|(i, (name, _, _, _))| *name == want || number == Some(i + 1)).map(|(_, x)| x).collect();
        if picked.is_empty() {
            return Err(bad(cmd, format!("the fitting has no piece `{want}`")));
        }
        out = picked;
    }
    Ok((id, thickness, out))
}

fn size(pat: &Pattern) -> (f64, f64) {
    let (lo, hi) = pat.outline.iter().fold((Vec2::new(f64::MAX, f64::MAX), Vec2::new(f64::MIN, f64::MIN)), |(lo, hi), q| {
        (Vec2::new(lo.x.min(q.x), lo.y.min(q.y)), Vec2::new(hi.x.max(q.x), hi.y.max(q.y)))
    });
    if pat.outline.is_empty() { (0.0, 0.0) } else { (hi.x - lo.x, hi.y - lo.y) }
}

fn round(v: f64) -> f64 {
    (v * 1e6).round() / 1e6
}

fn xy(q: Vec2) -> [f64; 2] {
    [round(q.x), round(q.y)]
}

fn piece_json(name: &str, pat: &Pattern, body: &Option<String>, thickness: f64) -> serde_json::Map<String, Value> {
    let (w, h) = size(pat);
    let mut m = serde_json::Map::new();
    m.insert("piece".into(), json!(name));
    m.insert("body".into(), json!(body));
    m.insert("thickness".into(), json!(round(thickness)));
    m.insert("width".into(), json!(round(w)));
    m.insert("height".into(), json!(round(h)));
    m.insert("area".into(), json!(round(pat.area())));
    m
}

fn patterns(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "fitting.patterns";
    let (id, thickness, list) = pieces(s, p, cmd)?;
    let pieces: Vec<Value> = list
        .iter()
        .map(|(name, pat, body, t)| {
            let mut m = piece_json(name, pat, body, *t);
            m.insert("outline".into(), json!(pat.outline.iter().map(|q| xy(*q)).collect::<Vec<_>>()));
            m.insert("holes".into(), json!(pat.holes.iter().map(|h| h.iter().map(|q| xy(*q)).collect::<Vec<_>>()).collect::<Vec<_>>()));
            m.insert("marks".into(), json!(pat.marks.iter().map(|(a, b)| [xy(*a), xy(*b)]).collect::<Vec<_>>()));
            Value::Object(m)
        })
        .collect();
    let exact = s.doc.fitting_exact(id).unwrap_or(true);
    Ok(json!({"feature": id, "thickness": thickness, "exact": exact, "pieces": pieces}))
}

/// A piece's flat pattern as a DXF: the holes first (layer CUT), then the outline (OUTLINE),
/// then the marked lines (MARK, dashed).
pub fn pattern_dxf(pat: &Pattern) -> Result<String> {
    use solidcraft_io::{DxfEntity, DxfLayer, DxfShape};
    let closed = |pts: &[Vec2]| {
        let mut v = pts.to_vec();
        if let (Some(a), Some(b)) = (v.first().copied(), v.last().copied())
            && a.dist(b) > 1e-9
        {
            v.push(a);
        }
        DxfShape::Polyline { points: v }
    };
    let mut entities = Vec::new();
    for h in &pat.holes {
        entities.push(DxfEntity { layer: "CUT".into(), shape: closed(h) });
    }
    entities.push(DxfEntity { layer: "OUTLINE".into(), shape: closed(&pat.outline) });
    for (a, b) in &pat.marks {
        entities.push(DxfEntity { layer: "MARK".into(), shape: DxfShape::Line(*a, *b) });
    }
    let layers = [DxfLayer::new("OUTLINE", 7), DxfLayer::new("CUT", 4), DxfLayer::new("MARK", 1).dashed()];
    Ok(solidcraft_io::write_dxf_drawing(&layers, &entities)?)
}

/// `<path>-<piece>.dxf` (the piece's spaces as dashes).
fn piece_path(path: &str, piece: &str) -> String {
    let p = std::path::Path::new(path);
    let stem = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "fitting".into());
    let ext = p.extension().map(|e| e.to_string_lossy().to_string()).unwrap_or_else(|| "dxf".into());
    p.with_file_name(format!("{stem}-{}.{ext}", piece.replace(' ', "-"))).to_string_lossy().to_string()
}

fn export_dxf(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "fitting.export_dxf";
    let (id, thickness, list) = pieces(s, p, cmd)?;
    let path = str_(p, "path").map(str::trim).filter(|x| !x.is_empty());
    // every file is made before the first is written
    let texts = list.iter().map(|(_, pat, _, _)| pattern_dxf(pat)).collect::<Result<Vec<_>>>()?;
    let mut out = Vec::new();
    for ((name, pat, body, t), text) in list.iter().zip(texts) {
        let mut m = piece_json(name, pat, body, *t);
        match path {
            Some(path) => {
                let file = piece_path(path, name);
                solidcraft_io::vfs::write(&file, text.as_bytes()).map_err(|e| EngineError::Other(format!("{file}: {e}")))?;
                m.insert("path".into(), json!(file));
            }
            None => {
                m.insert("dxf".into(), json!(text));
            }
        }
        out.push(Value::Object(m));
    }
    let exact = s.doc.fitting_exact(id).unwrap_or(true);
    Ok(json!({"feature": id, "thickness": thickness, "exact": exact, "pieces": out}))
}

#[cfg(test)]
#[path = "fitting_tests.rs"]
mod tests;
