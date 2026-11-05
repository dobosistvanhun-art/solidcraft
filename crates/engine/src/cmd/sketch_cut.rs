//! A sketch saved as a cutting drawing (laser, plasma, waterjet): each closed contour once, part
//! edges on the OUTER layer and holes on the INNER layer, in millimetres, so a cutter's software
//! can give the two their own cut settings and order. Each contour has a colour of LightBurn's
//! layer palette, red part edges and blue holes unless one is given (`sketch.cut_colour`); a
//! contour of its own colour lies on a layer of that colour (OUTER_ORANGE), and every entity
//! carries the exact colour, so LightBurn puts it on the layer of that colour. AutoCAD 2004.

use std::collections::HashSet;

use serde_json::Value;
use solidcraft_geom::Loop2;
use solidcraft_io::{DxfEntity, DxfLayer, DxfShape, loop_shapes, write_dxf_drawing};
use solidcraft_sketch::{CurveKind, Sketch, cut_colour};

use super::CommandSpec;
use super::sketch_store;
use crate::params::{bad, bool_, str_, string_list};
use crate::schema::sketch_cut::{CutColour, CutColourParams, CutColourResult, CutDxf, CutDxfParams};
use crate::schema::{TypedContract, output_schema_for};
use crate::{EngineError, Result, Session};

const CUT_DXF_CONTRACT: TypedContract<CutDxfParams> = TypedContract::new();
const CUT_COLOUR_CONTRACT: TypedContract<CutColourParams> = TypedContract::new();

/// The layers, in the colours a cutter maps to its own: red outer, blue inner, yellow open.
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
    ),
    CommandSpec::new("sketch.cut_colour", "Cut Colour", crate::checked!("sketch.cut_colour", cut_colour_cmd))
        .icon("sketch_dxf")
        .contract(&CUT_COLOUR_CONTRACT)
        .output(output_schema_for::<CutColourResult>)
        .params(
            "sketch?: id|name (default: the one being edited); curves: [ids]; colour: LightBurn layer colour, a name (black, blue, red, green, yellow, orange, cyan, magenta, light_gray, dark_blue, dark_red, dark_green, olive, brown, sky_blue, purple, gray) or 0..29, or none; contour?: bool (default true: each curve's whole closed contour). The cutting and nesting DXFs give the contour that colour",
        )];

/// The cutting drawing's layers as they are used: OUTER, INNER and OPEN in their own colours,
/// and a layer for each other colour a contour has (OUTER_ORANGE), each with its exact colour.
#[derive(Default)]
pub struct CutLayers {
    layers: Vec<DxfLayer>,
}

impl CutLayers {
    /// The layer a contour of this role (OUTER, INNER, OPEN) and colour goes on, added if new.
    pub fn layer(&mut self, role: &str, colour: Option<u8>) -> String {
        let default = match role {
            OUTER_LAYER => cut_colour::OUTER,
            INNER_LAYER => cut_colour::INNER,
            _ => cut_colour::OPEN,
        };
        let c = colour.filter(|c| cut_colour::rgb(*c).is_some()).unwrap_or(default);
        let name = if c == default { role.to_string() } else { format!("{role}_{}", cut_colour::name(c).unwrap_or_default().to_ascii_uppercase()) };
        if !self.layers.iter().any(|l| l.name == name) {
            self.layers.push(DxfLayer::new(name.clone(), cut_colour::aci(c)).with_true_colour(cut_colour::true_colour(c).unwrap_or_default()));
        }
        name
    }

    /// The layers, OUTER, INNER and OPEN first (each always there), then the others as they came.
    pub fn into_layers(mut self) -> Vec<DxfLayer> {
        for role in [OPEN_LAYER, INNER_LAYER, OUTER_LAYER] {
            self.layer(role, None);
            if let Some(i) = self.layers.iter().position(|l| l.name == role) {
                let l = self.layers.remove(i);
                self.layers.insert(0, l);
            }
        }
        self.layers
    }
}

/// A closed contour's colour: the first colour among its curves (none: its role's).
pub fn contour_colour(sk: &Sketch, ids: &[String]) -> Option<u8> {
    ids.iter().find_map(|id| sk.curve_index(id).and_then(|i| sk.curves.get(i)).and_then(|c| c.cut_colour))
}

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
    let mut colours: Vec<Option<u8>> = Vec::new();
    let mut used: HashSet<&str> = HashSet::new();
    for pr in &ss.profiles {
        let holes = pr.region.holes.iter().zip(pr.hole_curves.iter().map(Vec::as_slice));
        for (lp, ids) in std::iter::once((&pr.region.outer, pr.outer_curves.as_slice())).chain(holes) {
            used.extend(ids.iter().map(String::as_str));
            if seen.insert(loop_key(ids)) {
                loops.push(lp.clone());
                colours.push(contour_colour(sk, ids));
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
    let mut cut = CutLayers::default();
    let (mut outer, mut inner) = (0, 0);
    for (i, lp) in loops.iter().enumerate() {
        let role = if depth(&loops, i).is_multiple_of(2) {
            outer += 1;
            OUTER_LAYER
        } else {
            inner += 1;
            INNER_LAYER
        };
        let layer = cut.layer(role, colours.get(i).copied().flatten());
        entities.extend(loop_shapes(lp).into_iter().map(|shape| DxfEntity { layer: layer.clone(), shape }));
    }
    for &ci in &open_curves {
        let layer = cut.layer(OPEN_LAYER, sk.curves.get(ci).and_then(|c| c.cut_colour));
        entities.extend(open_shapes(sk, ci).into_iter().map(|shape| DxfEntity { layer: layer.clone(), shape }));
    }
    let layers = cut.into_layers();
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

fn cut_colour_cmd(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "sketch.cut_colour";
    let ids = string_list(cmd, p, "curves")?;
    if ids.is_empty() {
        return Err(bad(cmd, "`curves` must list curve ids"));
    }
    let colour = CutColour::read(p.get("colour").unwrap_or(&Value::Null)).map_err(|e| bad(cmd, format!("`colour` {e}")))?.0;
    let whole = bool_(p, "contour").unwrap_or(true);
    let id = sketch_store::target(s, p, cmd)?;
    // Each curve's whole closed contour (the loops it is in), as the sketch is now.
    let mut take: Vec<String> = ids.clone();
    if whole && let Some(ss) = s.model.state().sketch(id) {
        for pr in &ss.profiles {
            for lp in std::iter::once(&pr.outer_curves).chain(pr.hole_curves.iter()) {
                if lp.iter().any(|c| ids.contains(c)) {
                    take.extend(lp.iter().cloned());
                }
            }
        }
    }
    take.sort();
    take.dedup();
    let (curves, info) = super::sketch::edit(s, p, cmd, false, |sk, _| {
        let mut out = Vec::new();
        for cid in &take {
            let i = sk.curve_index(cid).ok_or_else(|| bad(cmd, format!("unknown curve `{cid}`")))?;
            if let Some(c) = sk.curves.get_mut(i) {
                c.cut_colour = colour;
                out.push(cid.clone());
            }
        }
        Ok(out)
    })?;
    let reply = CutColourResult { curves, colour: colour.and_then(cut_colour::name).map(str::to_string), sketch: info, param_warnings: Vec::new() };
    serde_json::to_value(reply).map_err(|e| bad(cmd, e.to_string()))
}

#[cfg(test)]
#[path = "sketch_cut_tests.rs"]
mod tests;
