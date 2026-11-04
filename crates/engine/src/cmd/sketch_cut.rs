//! A sketch saved as a cutting drawing (laser, plasma, waterjet): each closed contour once, part
//! edges on the OUTER layer and holes on the INNER layer, AutoCAD 2000 in millimetres, so a
//! cutter's software can give the two their own cut settings and order.

use std::collections::HashSet;

use serde_json::Value;
use solidcraft_geom::Loop2;
use solidcraft_io::{DxfEntity, DxfLayer, DxfShape, loop_shapes, write_dxf_drawing};
use solidcraft_sketch::{CurveKind, Sketch};

use super::CommandSpec;
use super::sketch_store;
use crate::params::{bad, str_};
use crate::schema::sketch_cut::{CutDxf, CutDxfParams};
use crate::schema::{TypedContract, output_schema_for};
use crate::{EngineError, Result, Session};

const CUT_DXF_CONTRACT: TypedContract<CutDxfParams> = TypedContract::new();

/// The layers, with AutoCAD colours a cutter maps to its own: red outer, blue inner, yellow open.
pub const OUTER_LAYER: &str = "OUTER";
pub const INNER_LAYER: &str = "INNER";
pub const OPEN_LAYER: &str = "OPEN";

pub static COMMANDS: &[CommandSpec] = &[CommandSpec::new("sketch.export_cut_dxf", "Save Cut DXF", crate::checked!("sketch.export_cut_dxf", cut_dxf_cmd))
    .icon("sketch_dxf")
    .noundo()
    .contract(&CUT_DXF_CONTRACT)
    .output(output_schema_for::<CutDxf>)
    .params(
        "sketch?: id|name (default: the one being edited, else the newest); path?: file to write (else the DXF text is returned); allow_open?: bool (default false: open ends or curves in no closed contour are an error). Each closed contour once: part edges (inside an even number of other contours) on layer OUTER, holes on INNER, open curves on OPEN; construction curves are left out. Returns outer, inner, open_ends, open_curves",
    )];

/// The contour's curve ids, sorted, so a loop that is one region's hole and another's outer
/// edge is saved once.
fn loop_key(ids: &[String]) -> Vec<String> {
    let mut k = ids.to_vec();
    k.sort();
    k
}

/// How many of the other contours hold this one (a point on it, strictly inside them).
fn depth(loops: &[Loop2], i: usize) -> usize {
    let Some(p) = loops.get(i).and_then(|l| l.segs.first()).map(|s| s.start()) else {
        return 0;
    };
    loops.iter().enumerate().filter(|&(j, l)| j != i && l.contains(p)).count()
}

/// The entities of a curve that is in no closed contour, as the plain sketch DXF writes it.
fn open_shapes(sk: &Sketch, ci: usize) -> Vec<DxfShape> {
    let pt = |k: usize| sk.point(k).unwrap_or_default();
    match sk.curves.get(ci).map(|c| &c.kind) {
        Some(CurveKind::Line { a, b }) => vec![DxfShape::Line(pt(*a), pt(*b))],
        Some(CurveKind::Circle { c, r }) => vec![DxfShape::Circle { c: pt(*c), r: *r }],
        _ => {
            let points = sk.polyline(ci);
            if points.len() < 2 { Vec::new() } else { vec![DxfShape::Polyline { points }] }
        }
    }
}

fn cut_dxf_cmd(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "sketch.export_cut_dxf";
    let allow_open = p.get("allow_open").and_then(Value::as_bool).unwrap_or(false);
    let id = sketch_store::target_or_newest(s, p, cmd)?;
    let st = s.model.state();
    let ss = st.sketch(id).ok_or_else(|| EngineError::Other("the sketch has not been evaluated".into()))?;
    let sk = &ss.sketch;

    let mut seen = HashSet::new();
    let mut loops: Vec<Loop2> = Vec::new();
    let mut used: HashSet<&str> = HashSet::new();
    for pr in &ss.profiles {
        let holes = pr.region.holes.iter().zip(pr.hole_curves.iter().map(Vec::as_slice));
        for (lp, ids) in std::iter::once((&pr.region.outer, pr.outer_curves.as_slice())).chain(holes) {
            used.extend(ids.iter().map(String::as_str));
            if seen.insert(loop_key(ids)) {
                loops.push(lp.clone());
            }
        }
    }
    let open_curves: Vec<usize> = sk
        .curves
        .iter()
        .enumerate()
        .filter(|(_, c)| !(c.construction || c.centerline || c.infinite) && !used.contains(c.id.as_str()))
        .map(|(i, _)| i)
        .collect();
    let open_ends = ss.ends.open.len();
    if !allow_open && (open_ends > 0 || !open_curves.is_empty()) {
        return Err(bad(
            cmd,
            format!(
                "the drawing has {open_ends} open ends and {} curves in no closed contour; close them (sketch.loose_ends shows where) or pass allow_open: true to save them on the OPEN layer",
                open_curves.len()
            ),
        ));
    }
    if loops.is_empty() && open_curves.is_empty() {
        return Err(bad(cmd, "the sketch has no closed contour to cut"));
    }

    let mut entities = Vec::new();
    let (mut outer, mut inner) = (0, 0);
    for (i, lp) in loops.iter().enumerate() {
        let layer = if depth(&loops, i).is_multiple_of(2) {
            outer += 1;
            OUTER_LAYER
        } else {
            inner += 1;
            INNER_LAYER
        };
        entities.extend(loop_shapes(lp).into_iter().map(|shape| DxfEntity { layer: layer.to_string(), shape }));
    }
    for &ci in &open_curves {
        entities.extend(open_shapes(sk, ci).into_iter().map(|shape| DxfEntity { layer: OPEN_LAYER.to_string(), shape }));
    }
    let layers = [DxfLayer::new(OUTER_LAYER, 1), DxfLayer::new(INNER_LAYER, 5), DxfLayer::new(OPEN_LAYER, 2)];
    let text = write_dxf_drawing(&layers, &entities).map_err(|e| bad(cmd, e.to_string()))?;
    let mut reply = CutDxf { dxf: None, path: None, outer, inner, open_ends, open_curves: open_curves.len(), param_warnings: Vec::new() };
    match str_(p, "path").filter(|x| !x.trim().is_empty()) {
        Some(path) => {
            solidcraft_io::vfs::write(path, text.as_bytes()).map_err(|e| EngineError::Other(format!("{path}: {e}")))?;
            reply.path = Some(path.to_string());
        }
        None => reply.dxf = Some(text),
    }
    serde_json::to_value(reply).map_err(|e| bad(cmd, e.to_string()))
}

#[cfg(test)]
#[path = "sketch_cut_tests.rs"]
mod tests;
