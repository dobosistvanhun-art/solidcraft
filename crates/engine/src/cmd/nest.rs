//! The nesting commands: set the design's job up (`nest.setup`), lay it out (`nest.run`), edit a
//! placed copy by hand (`nest.edit`), read it (`nest.info`) and save a sheet as a cutting DXF
//! (`nest.export_dxf`, the Cutting tab's layers). The work is in `crate::nest`.

use serde_json::{Value, json};
use solidcraft_doc::FeatureKind;
use solidcraft_doc::nesting::{MaterialSheet, NestJob, NestPart, NestQuality, PartSource};
use solidcraft_geom::{Loop2, Vec2};
use solidcraft_io::{DxfEntity, DxfLayer, DxfShape, loop_shapes, write_dxf_drawing};

use super::CommandSpec;
use super::sketch_cut::{INNER_LAYER, OUTER_LAYER};
use crate::nest::{self, Shape};
use crate::params::{bad, bool_, num, str_};
use crate::schema::nest::{NestEditParams, NestExportParams, NestInfoParams, NestReply, NestRunParams, NestSetupParams};
use crate::schema::{TypedContract, output_schema_for};
use crate::{EngineError, Result, Session};

const SETUP: TypedContract<NestSetupParams> = TypedContract::new();
const RUN: TypedContract<NestRunParams> = TypedContract::new();
const EDIT: TypedContract<NestEditParams> = TypedContract::new();
const INFO: TypedContract<NestInfoParams> = TypedContract::new();
const EXPORT: TypedContract<NestExportParams> = TypedContract::new();

/// The layer of the sheet's outline in an exported DXF.
pub const SHEET_LAYER: &str = "SHEET";

pub static COMMANDS: &[CommandSpec] = &[
    CommandSpec::new("nest.edit", "Edit Nested Part", crate::checked!("nest.edit", edit))
        .icon("move")
        .contract(&EDIT)
        .output(output_schema_for::<NestReply>)
        .params("placement: index; x?, y?: its centre on the sheet (mm); angle?: deg; sheet?; locked?: bool; remove?: bool. Answers collides (placements too close) and outside (past the margin)"),
    CommandSpec::new("nest.export_dxf", "Save Nest DXF", crate::checked!("nest.export_dxf", export))
        .icon("dxf")
        .noundo()
        .contract(&EXPORT)
        .output(output_schema_for::<NestReply>)
        .params("sheet?: index (default 0); all?: bool (every sheet, path-1.dxf, path-2.dxf…); path?: file (else the DXF text); sheet_outline?: bool (layer SHEET, default false). Parts where they lie: outer contours on OUTER, holes on INNER, mm"),
    CommandSpec::new("nest.info", "Nesting Info", crate::checked!("nest.info", info))
        .icon("list")
        .noundo()
        .contract(&INFO)
        .output(output_schema_for::<NestReply>)
        .params("brief?: bool (no outlines). The job: sheet, gap, kerf, quality, parts (name, count, area, cut length, error), placements (sheet, x, y, angle, locked, outlines), stats, collisions, outside, variants"),
    CommandSpec::new("nest.run", "Auto Nest", crate::checked!("nest.run", run))
        .icon("pattern_rect")
        .contract(&RUN)
        .output(output_schema_for::<NestReply>)
        .params("variant?: index of the last run's layouts to use instead of running; quality?: fast|normal|precise. Lays the parts out (locked copies stay), keeping the three best layouts as variants"),
    CommandSpec::new("nest.setup", "Nesting", crate::checked!("nest.setup", setup))
        .icon("flat")
        .contract(&SETUP)
        .output(output_schema_for::<NestReply>)
        .params(
            "parts?: [{sketch: id|name | body: sheet metal body, count?: n (default 1), grain?: bool (half turns only), fixed_angle?: bool}] (replaces the list and drops the layout); \
             sheet?: {width, height, margin?} mm; gap?: mm; kerf?: mm (cut width, kept apart from the gap); quality?: fast (15°) | normal (5°) | precise (1°); clear?: bool",
        ),
];

fn quality(v: Option<&str>, cmd: &str) -> Result<Option<NestQuality>> {
    match v {
        None => Ok(None),
        Some("fast") => Ok(Some(NestQuality::Fast)),
        Some("normal") => Ok(Some(NestQuality::Normal)),
        Some("precise") => Ok(Some(NestQuality::Precise)),
        Some(_) => Err(bad(cmd, "`quality` is fast, normal or precise")),
    }
}

fn length(p: &Value, key: &str, cmd: &str, positive: bool) -> Result<Option<f64>> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(_) => {
            let v = num(p, key).filter(|v| v.is_finite() && v.abs() <= 100_000.0).ok_or_else(|| bad(cmd, format!("`{key}` must be a number of mm")))?;
            if positive && !(v > 0.0) || !positive && v < 0.0 {
                return Err(bad(cmd, format!("`{key}` must be {} (mm)", if positive { "more than 0" } else { "0 or more" })));
            }
            Ok(Some(v))
        }
    }
}

/// A part's name: its sketch's or body's.
fn part_name(s: &Session, part: &NestPart) -> String {
    match &part.source {
        PartSource::Sketch(id) => s.doc.feature(*id).map_or_else(|| format!("sketch {id}"), |f| f.name.clone()),
        PartSource::SheetBody(name) => name.clone(),
    }
}

fn shapes(s: &Session, job: &NestJob) -> Vec<std::result::Result<Shape, String>> {
    let st = s.model.state();
    job.parts.iter().map(|p| nest::shape(&st, p)).collect()
}

/// The parts that could be read, and the first that could not (its name and why).
fn usable(s: &Session, job: &NestJob) -> std::result::Result<Vec<Shape>, String> {
    let mut out = Vec::new();
    for (part, shape) in job.parts.iter().zip(shapes(s, job)) {
        match shape {
            Ok(x) => out.push(x),
            Err(e) => return Err(format!("part `{}`: {e}", part_name(s, part))),
        }
    }
    Ok(out)
}

fn job(s: &Session, cmd: &str) -> Result<NestJob> {
    s.doc.nesting.clone().ok_or_else(|| bad(cmd, "the design has no nesting job yet (nest.setup)"))
}

fn reply(v: Value) -> Result<Value> {
    Ok(v)
}

fn setup(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "nest.setup";
    if bool_(p, "clear").unwrap_or(false) {
        s.doc_mut().nesting = None;
        return reply(json!({"cleared": true}));
    }
    let mut j = s.doc.nesting.clone().unwrap_or_default();
    if let Some(list) = p.get("parts") {
        let list = list.as_array().ok_or_else(|| bad(cmd, "`parts` must be a list"))?;
        if list.len() > 1000 {
            return Err(bad(cmd, "at most 1000 parts"));
        }
        let mut parts = Vec::new();
        for (i, v) in list.iter().enumerate() {
            let source = match (v.get("sketch"), str_(v, "body")) {
                (Some(sk), None) => {
                    let (key, found) = match sk {
                        Value::Number(n) => (n.to_string(), n.as_u64().and_then(|id| s.doc.feature(id))),
                        Value::String(t) => (t.clone(), s.doc.find_feature(t)),
                        _ => return Err(bad(cmd, format!("part {i}: `sketch` is an id or a name"))),
                    };
                    let f = found.ok_or_else(|| bad(cmd, format!("part {i}: no sketch `{key}`")))?;
                    if !matches!(f.kind, FeatureKind::Sketch { .. }) {
                        return Err(bad(cmd, format!("part {i}: `{key}` is not a sketch")));
                    }
                    PartSource::Sketch(f.id)
                }
                (None, Some(name)) => {
                    if !s.model.state().sheets.iter().any(|sh| sh.body == name) {
                        return Err(bad(cmd, format!("part {i}: no sheet metal body `{name}`")));
                    }
                    PartSource::SheetBody(name.to_string())
                }
                _ => return Err(bad(cmd, format!("part {i}: give `sketch` or `body`"))),
            };
            let count = match v.get("count") {
                None => 1,
                Some(c) => c.as_u64().filter(|c| (1..=nest::MAX_COPIES as u64).contains(c)).ok_or_else(|| bad(cmd, format!("part {i}: `count` is 1…{}", nest::MAX_COPIES)))? as u32,
            };
            parts.push(NestPart { source, count, grain: bool_(v, "grain").unwrap_or(false), fixed_angle: bool_(v, "fixed_angle").unwrap_or(false) });
        }
        if parts != j.parts {
            j.parts = parts;
            j.layout = Default::default();
            j.variants.clear();
        }
    }
    if let Some(sh) = p.get("sheet") {
        let width = length(sh, "width", cmd, true)?.ok_or_else(|| bad(cmd, "`sheet` needs width and height"))?;
        let height = length(sh, "height", cmd, true)?.ok_or_else(|| bad(cmd, "`sheet` needs width and height"))?;
        let margin = length(sh, "margin", cmd, false)?.unwrap_or(j.sheet.margin);
        j.sheet = MaterialSheet { width, height, margin };
    }
    if let Some(g) = length(p, "gap", cmd, false)? {
        j.gap = g;
    }
    if let Some(k) = length(p, "kerf", cmd, false)? {
        j.kerf = k;
    }
    if let Some(q) = quality(str_(p, "quality"), cmd)? {
        j.quality = q;
    }
    s.doc_mut().nesting = Some(j);
    info(s, &json!({"brief": true}))
}

fn run(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "nest.run";
    let mut j = job(s, cmd)?;
    if let Some(k) = p.get("variant") {
        let k = k.as_u64().ok_or_else(|| bad(cmd, "`variant` is an index"))? as usize;
        let v = j.variants.get(k).cloned().ok_or_else(|| bad(cmd, format!("no layout {k} (the last run kept {})", j.variants.len())))?;
        j.layout = v;
        s.doc_mut().nesting = Some(j);
        return info(s, &json!({"brief": true}));
    }
    if let Some(q) = quality(str_(p, "quality"), cmd)? {
        j.quality = q;
    }
    if j.parts.is_empty() {
        return Err(bad(cmd, "the job has no parts (nest.setup parts)"));
    }
    let shapes = usable(s, &j).map_err(|e| bad(cmd, e))?;
    let found = nest::run(&j, &shapes).map_err(|e| bad(cmd, e))?;
    j.layout = found.layouts.first().cloned().unwrap_or_default();
    j.variants = found.layouts;
    s.doc_mut().nesting = Some(j);
    info(s, &json!({"brief": true}))
}

fn edit(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "nest.edit";
    let mut j = job(s, cmd)?;
    let i = p.get("placement").and_then(Value::as_u64).ok_or_else(|| bad(cmd, "`placement` is an index"))? as usize;
    if i >= j.layout.placements.len() {
        return Err(bad(cmd, format!("no placement {i} (the layout has {})", j.layout.placements.len())));
    }
    if bool_(p, "remove").unwrap_or(false) {
        j.layout.placements.remove(i);
        s.doc_mut().nesting = Some(j);
        return info(s, &json!({"brief": true}));
    }
    let coord = |key: &str| -> Result<Option<f64>> {
        match p.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(_) => num(p, key).filter(|v| v.is_finite() && v.abs() <= 1_000_000.0).map(Some).ok_or_else(|| bad(cmd, format!("`{key}` must be a number"))),
        }
    };
    let (x, y, angle) = (coord("x")?, coord("y")?, coord("angle")?);
    let sheet = match p.get("sheet") {
        None | Some(Value::Null) => None,
        Some(v) => Some(v.as_u64().filter(|v| (*v as usize) < nest::MAX_SHEETS).ok_or_else(|| bad(cmd, "`sheet` is an index"))? as usize),
    };
    if let Some(pl) = j.layout.placements.get_mut(i) {
        if let Some(v) = x {
            pl.x = v;
        }
        if let Some(v) = y {
            pl.y = v;
        }
        if let Some(v) = angle {
            pl.angle = v.rem_euclid(360.0);
        }
        if let Some(v) = sheet {
            pl.sheet = v;
        }
        if let Some(v) = bool_(p, "locked") {
            pl.locked = v;
        }
    }
    s.doc_mut().nesting = Some(j.clone());
    let mut out = info(s, &json!({"brief": true}))?;
    let shapes: Vec<Shape> = shapes(s, &j).into_iter().filter_map(std::result::Result::ok).collect();
    if shapes.len() == j.parts.len() {
        let hits: Vec<usize> = nest::collisions(&j, &shapes, &j.layout).into_iter().filter_map(|(a, b)| if a == i { Some(b) } else if b == i { Some(a) } else { None }).collect();
        out["collides"] = json!(hits);
        out["outside"] = json!(nest::outside(&j, &shapes, &j.layout).contains(&i));
    }
    Ok(out)
}

fn round(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

fn info(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "nest.info";
    let j = job(s, cmd)?;
    let brief = bool_(p, "brief").unwrap_or(false);
    let read = shapes(s, &j);
    let parts: Vec<Value> = j
        .parts
        .iter()
        .zip(&read)
        .map(|(part, shape)| {
            let mut v = json!({"name": part_name(s, part), "count": part.count, "grain": part.grain, "fixed_angle": part.fixed_angle});
            match &part.source {
                PartSource::Sketch(id) => v["sketch"] = json!(id),
                PartSource::SheetBody(b) => v["body"] = json!(b),
            }
            match shape {
                Ok(x) => {
                    v["area_mm2"] = json!(round(x.area));
                    v["cut_length_mm"] = json!(round(x.cut_length));
                    v["size"] = json!([round(x.half.x * 2.0), round(x.half.y * 2.0)]);
                }
                Err(e) => v["error"] = json!(e),
            }
            v
        })
        .collect();
    let ok: Vec<Shape> = read.into_iter().filter_map(std::result::Result::ok).collect();
    let complete = ok.len() == j.parts.len();
    let placements: Vec<Value> = j
        .layout
        .placements
        .iter()
        .enumerate()
        .map(|(i, pl)| {
            let mut v = json!({"index": i, "part": pl.part, "sheet": pl.sheet, "x": round(pl.x), "y": round(pl.y), "angle": round(pl.angle), "locked": pl.locked});
            if !brief && complete && let Some(sh) = ok.get(pl.part) {
                v["outlines"] = json!(nest::placed_outlines(sh, pl).iter().map(|poly| poly.iter().map(|q| [round(q.x), round(q.y)]).collect::<Vec<_>>()).collect::<Vec<_>>());
            }
            v
        })
        .collect();
    let mut out = json!({
        "sheet": {"width": j.sheet.width, "height": j.sheet.height, "margin": j.sheet.margin},
        "gap": j.gap,
        "kerf": j.kerf,
        "quality": match j.quality { NestQuality::Fast => "fast", NestQuality::Normal => "normal", NestQuality::Precise => "precise" },
        "parts": parts,
        "placements": placements,
        "unplaced": j.layout.unplaced,
    });
    if complete {
        out["stats"] = nest::stats(&j, &ok, &j.layout);
        out["collisions"] = json!(nest::collisions(&j, &ok, &j.layout).into_iter().map(|(a, b)| [a, b]).collect::<Vec<_>>());
        out["outside"] = json!(nest::outside(&j, &ok, &j.layout));
        out["variants"] = json!(j.variants.iter().map(|v| nest::stats(&j, &ok, v)).collect::<Vec<_>>());
    }
    Ok(out)
}

/// One sheet of the layout as DXF entities: every copy's loops where it lies.
fn sheet_entities(j: &NestJob, shapes: &[Shape], sheet: usize, outline: bool) -> Vec<DxfEntity> {
    let mut out = Vec::new();
    for pl in j.layout.placements.iter().filter(|p| p.sheet == sheet) {
        let (Some(shape), Some(m)) = (shapes.get(pl.part), nest::placement_transform(pl)) else { continue };
        let moved = |l: &Loop2| Loop2 { segs: l.segs.iter().map(|s| s.similar(&m)).collect() };
        for r in &shape.regions {
            out.extend(loop_shapes(&moved(&r.outer)).into_iter().map(|shape| DxfEntity { layer: OUTER_LAYER.to_string(), shape }));
            for h in &r.holes {
                out.extend(loop_shapes(&moved(h)).into_iter().map(|shape| DxfEntity { layer: INNER_LAYER.to_string(), shape }));
            }
        }
        for h in &shape.extra_holes {
            out.extend(loop_shapes(&moved(h)).into_iter().map(|shape| DxfEntity { layer: INNER_LAYER.to_string(), shape }));
        }
    }
    if outline {
        let (w, h) = (j.sheet.width, j.sheet.height);
        out.push(DxfEntity { layer: SHEET_LAYER.into(), shape: DxfShape::Polyline { points: vec![Vec2::ZERO, Vec2::new(w, 0.0), Vec2::new(w, h), Vec2::new(0.0, h), Vec2::ZERO] } });
    }
    out
}

fn export(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "nest.export_dxf";
    let j = job(s, cmd)?;
    let shapes = usable(s, &j).map_err(|e| bad(cmd, e))?;
    let sheets = j.sheet_count();
    if sheets == 0 {
        return Err(bad(cmd, "nothing is laid out yet (nest.run)"));
    }
    let outline = bool_(p, "sheet_outline").unwrap_or(false);
    let layers = [DxfLayer::new(OUTER_LAYER, 1), DxfLayer::new(INNER_LAYER, 5), DxfLayer::new(SHEET_LAYER, 8)];
    let mut warnings = Vec::new();
    let hits = nest::collisions(&j, &shapes, &j.layout);
    if !hits.is_empty() {
        warnings.push(format!("{} pairs of parts are closer than the gap", hits.len()));
    }
    let off = nest::outside(&j, &shapes, &j.layout);
    if !off.is_empty() {
        warnings.push(format!("{} parts reach past the sheet's margin", off.len()));
    }
    let text = |sheet: usize| write_dxf_drawing(&layers, &sheet_entities(&j, &shapes, sheet, outline)).map_err(|e| bad(cmd, e.to_string()));
    let path = str_(p, "path").filter(|x| !x.trim().is_empty());
    if bool_(p, "all").unwrap_or(false) {
        let path = path.ok_or_else(|| bad(cmd, "`all` writes a file per sheet: give `path`"))?;
        let (stem, ext) = match path.rsplit_once('.') {
            Some((a, b)) if !b.contains('/') && !b.contains('\\') => (a.to_string(), format!(".{b}")),
            _ => (path.to_string(), ".dxf".to_string()),
        };
        let mut written = Vec::new();
        for k in 0..sheets {
            let file = format!("{stem}-{}{ext}", k + 1);
            solidcraft_io::vfs::write(&file, text(k)?.as_bytes()).map_err(|e| EngineError::Other(format!("{file}: {e}")))?;
            written.push(file);
        }
        return Ok(json!({"paths": written, "sheets": sheets, "warnings": warnings}));
    }
    let sheet = match p.get("sheet") {
        None | Some(Value::Null) => 0,
        Some(v) => v.as_u64().ok_or_else(|| bad(cmd, "`sheet` is an index"))? as usize,
    };
    if sheet >= sheets {
        return Err(bad(cmd, format!("no sheet {sheet} (the layout uses {sheets})")));
    }
    let dxf = text(sheet)?;
    match path {
        Some(path) => {
            solidcraft_io::vfs::write(path, dxf.as_bytes()).map_err(|e| EngineError::Other(format!("{path}: {e}")))?;
            Ok(json!({"path": path, "sheet": sheet, "sheets": sheets, "warnings": warnings}))
        }
        None => Ok(json!({"dxf": dxf, "sheet": sheet, "sheets": sheets, "warnings": warnings})),
    }
}

#[cfg(test)]
#[path = "nest_tests.rs"]
mod tests;
