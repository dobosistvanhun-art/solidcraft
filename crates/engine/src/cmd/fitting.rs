//! Pipe fittings of sheet: a tee, a cone (concentric or eccentric, a branch into its mantle if
//! wanted) and a mitred elbow, each piece a body with an exact flat pattern; the patterns and
//! their DXF export, one file a piece.

use serde_json::{Value, json};
use solidcraft_doc::expr::{Expr, Kind};
use solidcraft_doc::fitting::{Measured, Pattern};
use solidcraft_doc::{Dir, FeatureKind, FittingType};
use solidcraft_geom::{Vec2, Vec3};

use super::CommandSpec;
use super::features::{add_feature, axis_dir_param, check_expr, point_param};
use crate::params::{bad, expr, str_};
use crate::schema::features::FeatureResult;
use crate::schema::fitting::{ConeParams, ElbowParams, FittingPatternParams, FittingPatternResult, TeeParams};
use crate::schema::{TypedContract, output_schema_for};
use crate::{EngineError, Result, Session};

const TEE_CONTRACT: TypedContract<TeeParams> = TypedContract::new();
const CONE_CONTRACT: TypedContract<ConeParams> = TypedContract::new();
const ELBOW_CONTRACT: TypedContract<ElbowParams> = TypedContract::new();
const PATTERN_CONTRACT: TypedContract<FittingPatternParams> = TypedContract::new();

pub static COMMANDS: &[CommandSpec] = &[
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
    CommandSpec::new("fitting.patterns", "Fitting Flat Patterns", crate::checked!("fitting.patterns", patterns))
        .icon("flat")
        .noundo()
        .doc_only()
        .contract(&PATTERN_CONTRACT)
        .output(output_schema_for::<FittingPatternResult>)
        .params("feature?, piece? → each piece's flat pattern: its size, area, outline, holes and marked lines (mm)"),
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
    let elbow = fitting == FittingType::Elbow;
    let kind = FeatureKind::Fitting {
        base,
        axis: Dir(axis),
        fitting,
        measured: measured(p, cmd)?,
        thickness: needed(s, p, "thickness", l, cmd)?,
        diameter: needed(s, p, "diameter", l, cmd)?,
        top: if fitting == FittingType::Cone { Some(needed(s, p, "top", l, cmd)?) } else { given(s, p, "top", l, cmd)? },
        height: if elbow { None } else { Some(needed(s, p, "height", l, cmd)?) },
        shift: given(s, p, "shift", l, cmd)?,
        branch: if fitting == FittingType::Tee { Some(needed(s, p, "branch", l, cmd)?) } else { given(s, p, "branch", l, cmd)? },
        branch_length: given(s, p, "branch_length", l, cmd)?,
        branch_at: given(s, p, "branch_at", l, cmd)?,
        branch_angle: given(s, p, "branch_angle", Kind::Angle, cmd)?,
        branch_offset: given(s, p, "branch_offset", l, cmd)?,
        bend: given(s, p, "bend", l, cmd)?,
        angle: given(s, p, "angle", Kind::Angle, cmd)?,
        segments: given(s, p, "segments", Kind::Unitless, cmd)?,
        straight: given(s, p, "straight", l, cmd)?,
    };
    add_feature(s, p, kind)
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
fn pieces(s: &Session, p: &Value, cmd: &str) -> Result<(u64, f64, Vec<(String, Pattern, Option<String>)>)> {
    let id = fitting_feature(s, p, cmd)?;
    let all = s.doc.fitting_patterns(id).map_err(|e| bad(cmd, e.to_string()))?;
    let thickness = s.doc.fitting_thickness(id).map_err(|e| bad(cmd, e.to_string()))?;
    // the bodies the feature made, in the order of its pieces
    let st = s.model.state();
    let bodies: Vec<String> = st.bodies.iter().filter(|b| b.feature == id).map(|b| b.name.clone()).collect();
    let mut out: Vec<(String, Pattern, Option<String>)> =
        all.into_iter().enumerate().map(|(i, (name, pat))| (name, pat, bodies.get(i).cloned())).collect();
    if let Some(want) = str_(p, "piece").map(|w| w.trim().to_lowercase()).filter(|w| !w.is_empty()) {
        let number = want.parse::<usize>().ok();
        let picked: Vec<_> = out.into_iter().enumerate().filter(|(i, (name, _, _))| *name == want || number == Some(i + 1)).map(|(_, x)| x).collect();
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

fn piece_json(name: &str, pat: &Pattern, body: &Option<String>) -> serde_json::Map<String, Value> {
    let (w, h) = size(pat);
    let mut m = serde_json::Map::new();
    m.insert("piece".into(), json!(name));
    m.insert("body".into(), json!(body));
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
        .map(|(name, pat, body)| {
            let mut m = piece_json(name, pat, body);
            m.insert("outline".into(), json!(pat.outline.iter().map(|q| xy(*q)).collect::<Vec<_>>()));
            m.insert("holes".into(), json!(pat.holes.iter().map(|h| h.iter().map(|q| xy(*q)).collect::<Vec<_>>()).collect::<Vec<_>>()));
            m.insert("marks".into(), json!(pat.marks.iter().map(|(a, b)| [xy(*a), xy(*b)]).collect::<Vec<_>>()));
            Value::Object(m)
        })
        .collect();
    Ok(json!({"feature": id, "thickness": thickness, "pieces": pieces}))
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
    let texts = list.iter().map(|(_, pat, _)| pattern_dxf(pat)).collect::<Result<Vec<_>>>()?;
    let mut out = Vec::new();
    for ((name, pat, body), text) in list.iter().zip(texts) {
        let mut m = piece_json(name, pat, body);
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
    Ok(json!({"feature": id, "thickness": thickness, "pieces": out}))
}

#[cfg(test)]
#[path = "fitting_tests.rs"]
mod tests;
