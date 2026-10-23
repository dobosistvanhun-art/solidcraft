//! Drawings: sheets of views of the model with dimensions and notes, exported as PDF or DXF.
//! The commands change what a drawing is made of; `drawing.render` draws it from the current
//! model (see `view_dxf/drawing_views.rs`).

use serde_json::{Value, json};
use solidcraft_doc::drawing::{
    DimKind, DimRef, DrawDim, DrawNote, DrawView, Drawing, MAX_ITEMS, Projection, SCALES, SHEET_SIZES, ViewKind, parse_scale,
};
use solidcraft_geom::Vec2;

use super::CommandSpec;
use super::view_dxf::drawing_views::{self as dv, Align, Prim, SheetDrawn, Style};
use crate::params::{bad, bool_, num, str_};
use crate::{EngineError, Result, Session};

const TAB: &str = "DRAWING";

pub static COMMANDS: &[CommandSpec] = &[
    CommandSpec::new("drawing.create", "New Drawing", create).at(TAB, "SHEET").icon("sheet").params(
        "name?; size?: A4|A3|A2|A1|A0 (default A3); landscape? (default true); scale?: 1:1, 1:2, 2:1… (default: the largest that fits the views); \
         projection?: first (ISO E, default) | third (ISO A); views?: standard (front, top, side and iso, default) | none → drawing id",
    ),
    CommandSpec::new("drawing.edit", "Sheet and Title Block", edit).at(TAB, "SHEET").icon("edit").doc_only().params(
        "drawing (id or name); name?, size?, landscape?, scale?, projection?; title?: {title, number, company, drawn_by, checked_by, date, material, revision, tolerances}; bodies?: [names] (empty: all visible)",
    ),
    CommandSpec::new("drawing.delete", "Delete Drawing", delete).icon("delete").doc_only().params("drawing (id or name)"),
    CommandSpec::new("drawing.list", "List Drawings", list).icon("list").noundo().doc_only().params("→ drawings: id, name, sheet, scale, views, dimensions"),
    CommandSpec::new("drawing.view.base", "Base View", view_base).at(TAB, "VIEWS").icon("camera").params(
        "drawing; orientation: front|back|top|bottom|left|right|iso (default front); at?: [x, y] sheet mm (default: the free middle); scale?; hidden? → view id",
    ),
    CommandSpec::new("drawing.view.projected", "Projected View", view_projected).at(TAB, "VIEWS").icon("project").params(
        "drawing; parent (view id); side: right|left|above|below|iso; at?: [x, y] (default: beside the parent); scale?; hidden? → view id",
    ),
    CommandSpec::new("drawing.view.section", "Section View", view_section).at(TAB, "VIEWS").icon("section").params(
        "drawing; parent (view id); line: [[x, y], [x, y]] on the sheet (the cutting plane through it, square to the sheet); flip? (look the other way); \
         at?: [x, y] (default: beside the parent); label? (default: the next free letter) → view id",
    ),
    CommandSpec::new("drawing.view.detail", "Detail View", view_detail).at(TAB, "VIEWS").icon("inspect").params(
        "drawing; parent (view id); center: [x, y] on the sheet; radius: sheet mm; scale? (default twice the parent's); at?; label? → view id",
    ),
    CommandSpec::new("drawing.view.edit", "Edit View", view_edit).icon("edit").params(
        "drawing; view; at?: [x, y]; scale? (empty: the drawing's); hidden?; flip? (section); orientation? (base view); label?",
    ),
    CommandSpec::new("drawing.view.delete", "Delete View", view_delete).icon("delete").params("drawing; view — with the views made from it and their dimensions"),
    CommandSpec::new("drawing.dimension", "Dimension", dimension).at(TAB, "ANNOTATE").icon("dimension").params(
        "drawing; view; kind: horizontal|vertical|aligned|diameter|radius|angle; points: [[x, y], …] on the sheet, each snapped to the nearest \
         edge end or circle centre of the model in that view (diameter, radius: one point on the circle; angle: the vertex, then a point on each side); \
         offset?: sheet mm (default 10); text? (`<>` is the value); tolerance? (±0.1, +0.2/-0.1, H7); \
         arc?: center (default) | min | max — a linear dimension to circles or arcs (its points on them) measures between their centres, near sides or far sides; \
         sharp — its points are virtual sharps (click at the corner a round took away): the corner as if it weren't rounded → dimension id",
    ),
    CommandSpec::new("drawing.note", "Note", note).at(TAB, "ANNOTATE").icon("text").params("drawing; at: [x, y]; text (lines); height? (mm, default 3.5) → note id"),
    CommandSpec::new("drawing.dimension.horizontal", "Horizontal Dimension", dim_horizontal).at(TAB, "ANNOTATE").icon("dimension").params("as drawing.dimension, kind horizontal"),
    CommandSpec::new("drawing.dimension.vertical", "Vertical Dimension", dim_vertical).at(TAB, "ANNOTATE").icon("dimension").params("as drawing.dimension, kind vertical"),
    CommandSpec::new("drawing.dimension.aligned", "Aligned Dimension", dim_aligned).at(TAB, "ANNOTATE").icon("dimension").params("as drawing.dimension, kind aligned"),
    CommandSpec::new("drawing.dimension.radius", "Radius Dimension", dim_radius).at(TAB, "ANNOTATE").icon("dimension").params("as drawing.dimension, kind radius: one point on the arc"),
    CommandSpec::new("drawing.dimension.diameter", "Diameter Dimension", dim_diameter).at(TAB, "ANNOTATE").icon("dimension").params("as drawing.dimension, kind diameter: one point on the circle"),
    CommandSpec::new("drawing.dimension.angle", "Angle Dimension", dim_angle).at(TAB, "ANNOTATE").icon("dimension").params("as drawing.dimension, kind angle: the corner, then a point on each side"),
    CommandSpec::new("drawing.dimension.arcs", "Arc to Arc Dimension", dim_arcs).at(TAB, "ANNOTATE").icon("dimension").params(
        "as drawing.dimension between two circles or arcs (a point on each): kind? horizontal (default) | vertical | aligned; arc?: max (over their far sides, default) | min (between their near sides) | center",
    ),
    CommandSpec::new("drawing.dimension.edit", "Edit Dimension", dimension_edit).icon("edit").params("drawing; dimension; offset?; text?; tolerance?; arc?: center|min|max"),
    CommandSpec::new("drawing.dimension.delete", "Delete Dimension", dimension_delete).icon("delete").params("drawing; dimension"),
    CommandSpec::new("drawing.note.edit", "Edit Note", note_edit).icon("edit").params("drawing; note; at?; text?; height?"),
    CommandSpec::new("drawing.note.delete", "Delete Note", note_delete).icon("delete").params("drawing; note"),
    CommandSpec::new("drawing.render", "Draw Sheet", render).noundo().icon("flat").params(
        "drawing → width, height (mm), prims: lines (style: visible|hidden|thin|chain|wide), texts and fills in sheet mm, views (frames), errors",
    ),
    CommandSpec::new("drawing.export_pdf", "Export Drawing as PDF", export_pdf).at(TAB, "EXPORT").icon("export").noundo().params("drawing; path? (.pdf; else → pdf_base64)"),
    CommandSpec::new("drawing.export_dxf", "Export Drawing as DXF", export_dxf).at(TAB, "EXPORT").icon("dxf").noundo().params(
        "drawing; path? (.dxf; else → dxf text). Layers VISIBLE, HIDDEN, THIN, CENTER, FRAME, TEXT, mm at 1:1 on the sheet",
    ),
];

fn find(s: &Session, p: &Value, cmd: &str) -> Result<usize> {
    let v = p.get("drawing").ok_or_else(|| bad(cmd, "`drawing` (id or name) is required"))?;
    let i = match v {
        Value::Number(n) => n.as_u64().and_then(|id| s.doc.drawings.iter().position(|d| d.id == id)),
        Value::String(name) => s.doc.drawings.iter().position(|d| d.name == *name),
        _ => None,
    };
    i.ok_or_else(|| bad(cmd, "no such drawing"))
}

fn id_of(p: &Value, k: &str, cmd: &str) -> Result<u64> {
    p.get(k).and_then(Value::as_u64).ok_or_else(|| bad(cmd, format!("`{k}` (an id) is required")))
}

fn point(v: Option<&Value>) -> Option<[f64; 2]> {
    let a = v?.as_array()?;
    let (x, y) = (a.first()?.as_f64()?, a.get(1)?.as_f64()?);
    (x.is_finite() && y.is_finite() && x.abs() < 1e5 && y.abs() < 1e5).then_some([x, y])
}

fn check_scale(text: &str, cmd: &str) -> Result<String> {
    parse_scale(text).map(|_| text.trim().to_string()).ok_or_else(|| bad(cmd, format!("`{text}` is not a scale (such as 1:2 or 2:1)")))
}

fn check_size(text: &str, cmd: &str) -> Result<String> {
    SHEET_SIZES
        .iter()
        .find(|(n, ..)| n.eq_ignore_ascii_case(text.trim()))
        .map(|(n, ..)| n.to_string())
        .ok_or_else(|| bad(cmd, "`size` must be A4, A3, A2, A1 or A0"))
}

fn projection(p: &Value, cmd: &str) -> Result<Option<Projection>> {
    match str_(p, "projection") {
        None => Ok(None),
        Some(t) => match t.trim().to_ascii_lowercase().as_str() {
            "first" | "iso e" | "e" => Ok(Some(Projection::First)),
            "third" | "iso a" | "a" => Ok(Some(Projection::Third)),
            _ => Err(bad(cmd, "`projection` must be first or third")),
        },
    }
}

/// The free area for views (inside the frame, above the title block).
fn area(d: &Drawing) -> (f64, f64, f64, f64) {
    let (w, h) = d.sheet.dims();
    (20.0 + 10.0, 10.0 + 36.0 + 12.0, w - 10.0 - 10.0, h - 10.0 - 10.0)
}

/// The standard views (front, top, side, iso) laid out on the sheet at the largest scale that fits.
fn standard_views(s: &Session, d: &mut Drawing, fixed_scale: bool, cmd: &str) -> Result<()> {
    let m = dv::model(s, d).map_err(|e| bad(cmd, e))?;
    let size = m.bounds.max - m.bounds.min;
    let (sx, sy, sz) = (size.x.max(1e-3), size.y.max(1e-3), size.z.max(1e-3));
    // The iso view's extent: the bounding box's corners seen from the iso direction.
    let (ir, iu) = dv::orientation("iso").unwrap_or((solidcraft_geom::Vec3::X, solidcraft_geom::Vec3::Z));
    let (mut iw, mut ih) = (0.0f64, 0.0f64);
    let (lo, hi) = (m.bounds.min, m.bounds.max);
    let corners: Vec<solidcraft_geom::Vec3> = (0..8)
        .map(|k| {
            solidcraft_geom::Vec3::new(
                if k & 1 == 0 { lo.x } else { hi.x },
                if k & 2 == 0 { lo.y } else { hi.y },
                if k & 4 == 0 { lo.z } else { hi.z },
            )
        })
        .collect();
    for a in &corners {
        for b in &corners {
            iw = iw.max((*a - *b).dot(ir));
            ih = ih.max((*a - *b).dot(iu));
        }
    }
    let (x0, y0, x1, y1) = area(d);
    let gap = 25.0;
    // The right column holds the side view above and the iso view beside the top view.
    let col = |s: f64| sy.max(iw) * s;
    let fits = |s: f64| sx * s + col(s) + 2.0 * gap <= (x1 - x0) && sz * s + sy.max(ih) * s + 2.0 * gap <= (y1 - y0);
    if !fixed_scale {
        d.scale = SCALES.iter().find(|t| parse_scale(t).is_some_and(fits)).unwrap_or(&"1:100").to_string();
    }
    let s = d.scale_factor();
    let third = d.projection == Projection::Third;
    // First angle: the top view below the front one, the view from the left to its right.
    // The group of views centred in the free area.
    let row = sy.max(ih) * s;
    let (cw, ch) = (sx * s + gap + col(s), sz * s + gap + row);
    let (left, bottom) = (x0 + ((x1 - x0) - cw).max(0.0) / 2.0, y0 + ((y1 - y0) - ch).max(0.0) / 2.0);
    let top_edge = bottom + ch;
    let fx = left + sx * s / 2.0;
    let side_x = fx + sx * s / 2.0 + gap + col(s) / 2.0;
    let (fy, ty) = if third {
        (bottom + sz * s / 2.0, bottom + sz * s + gap + row / 2.0)
    } else {
        (top_edge - sz * s / 2.0, top_edge - sz * s - gap - row / 2.0)
    };
    let front = d.take_id();
    d.views.push(DrawView {
        id: front,
        kind: ViewKind::Base { orientation: "front".into() },
        at: [fx, fy],
        scale: None,
        label: String::new(),
        hidden: false,
    });
    let top = d.take_id();
    d.views.push(DrawView {
        id: top,
        kind: ViewKind::Projected { parent: front, side: if third { "above" } else { "below" }.into() },
        at: [fx, ty],
        scale: None,
        label: String::new(),
        hidden: false,
    });
    let side = d.take_id();
    d.views.push(DrawView {
        id: side,
        kind: ViewKind::Projected { parent: front, side: "right".into() },
        at: [side_x, fy],
        scale: None,
        label: String::new(),
        hidden: false,
    });
    let iso = d.take_id();
    d.views.push(DrawView {
        id: iso,
        kind: ViewKind::Base { orientation: "iso".into() },
        at: [side_x, ty],
        scale: None,
        label: String::new(),
        hidden: false,
    });
    Ok(())
}

fn create(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "drawing.create";
    if s.doc.drawings.len() >= MAX_ITEMS {
        return Err(bad(cmd, "too many drawings"));
    }
    let id = s.doc.drawings.iter().map(|d| d.id).max().unwrap_or(0) + 1;
    let name =
        str_(p, "name").map(str::trim).filter(|n| !n.is_empty() && n.len() <= 128).map(str::to_string).unwrap_or_else(|| format!("Drawing{id}"));
    if s.doc.drawings.iter().any(|d| d.name == name) {
        return Err(bad(cmd, format!("there is a drawing `{name}` already")));
    }
    let mut d = Drawing::new(id, &name);
    if let Some(sz) = str_(p, "size") {
        d.sheet.size = check_size(sz, cmd)?;
    }
    if let Some(l) = bool_(p, "landscape") {
        d.sheet.landscape = l;
    }
    if let Some(pr) = projection(p, cmd)? {
        d.projection = pr;
    }
    let fixed = match str_(p, "scale") {
        Some(t) => {
            d.scale = check_scale(t, cmd)?;
            true
        }
        None => false,
    };
    d.title.title = s.doc.name.clone();
    d.title.date = today();
    match str_(p, "views").unwrap_or("standard") {
        "standard" => standard_views(s, &mut d, fixed, cmd)?,
        "none" => {}
        other => return Err(bad(cmd, format!("unknown `views`: {other} (standard or none)"))),
    }
    s.doc_mut().drawings.push(d);
    Ok(json!({"drawing": id, "name": name}))
}

/// Today as YYYY-MM-DD (UTC).
fn today() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let days = (secs / 86_400) as i64;
    // Civil from days (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

fn edit(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "drawing.edit";
    let i = find(s, p, cmd)?;
    let mut d = s.doc.drawings.get(i).cloned().ok_or_else(|| bad(cmd, "no such drawing"))?;
    if let Some(n) = str_(p, "name").map(str::trim) {
        if n.is_empty() || n.len() > 128 {
            return Err(bad(cmd, "`name` must be 1–128 characters"));
        }
        if s.doc.drawings.iter().any(|x| x.name == n && x.id != d.id) {
            return Err(bad(cmd, format!("there is a drawing `{n}` already")));
        }
        d.name = n.to_string();
    }
    if let Some(sz) = str_(p, "size") {
        d.sheet.size = check_size(sz, cmd)?;
    }
    if let Some(l) = bool_(p, "landscape") {
        d.sheet.landscape = l;
    }
    if let Some(t) = str_(p, "scale") {
        d.scale = check_scale(t, cmd)?;
    }
    if let Some(pr) = projection(p, cmd)? {
        d.projection = pr;
    }
    if let Some(t) = p.get("title").and_then(Value::as_object) {
        for (k, slot) in d.title.fields_mut() {
            if let Some(v) = t.get(k) {
                let v = v.as_str().ok_or_else(|| bad(cmd, format!("title.{k} must be text")))?;
                if v.len() > 500 {
                    return Err(bad(cmd, format!("title.{k} is too long")));
                }
                *slot = v.to_string();
            }
        }
    }
    if let Some(b) = p.get("bodies") {
        let list = b.as_array().ok_or_else(|| bad(cmd, "`bodies` must be a list of names"))?;
        d.bodies = list.iter().filter_map(Value::as_str).map(str::to_string).take(MAX_ITEMS).collect();
    }
    if let Some(slot) = s.doc_mut().drawings.get_mut(i) {
        *slot = d;
    }
    Ok(json!({"drawing": s.doc.drawings.get(i).map(|d| d.id)}))
}

fn delete(s: &mut Session, p: &Value) -> Result<Value> {
    let i = find(s, p, "drawing.delete")?;
    let d = s.doc_mut().drawings.remove(i);
    Ok(json!({"deleted": d.id}))
}

fn list(s: &mut Session, _p: &Value) -> Result<Value> {
    let ds: Vec<Value> = s
        .doc
        .drawings
        .iter()
        .map(|d| json!({"id": d.id, "name": d.name, "size": d.sheet.size, "landscape": d.sheet.landscape, "scale": d.scale, "projection": d.projection, "views": d.views.len(), "dimensions": d.dims.len(), "notes": d.notes.len()}))
        .collect();
    Ok(json!({"drawings": ds}))
}

/// Change drawing `i` with `f` (which returns the result).
fn change(s: &mut Session, i: usize, f: impl FnOnce(&mut Drawing) -> Result<Value>) -> Result<Value> {
    let mut d = s.doc.drawings.get(i).cloned().ok_or_else(|| EngineError::Other("no such drawing".into()))?;
    let out = f(&mut d)?;
    if let Some(slot) = s.doc_mut().drawings.get_mut(i) {
        *slot = d;
    }
    Ok(out)
}

fn common_view_params(v: &mut DrawView, p: &Value, cmd: &str) -> Result<()> {
    if let Some(t) = str_(p, "scale") {
        v.scale = if t.trim().is_empty() { None } else { Some(check_scale(t, cmd)?) };
    }
    if let Some(h) = bool_(p, "hidden") {
        v.hidden = h;
    }
    if let Some(at) = p.get("at") {
        v.at = point(Some(at)).ok_or_else(|| bad(cmd, "`at` must be [x, y] on the sheet (mm)"))?;
    }
    Ok(())
}

fn add_view(
    s: &mut Session,
    p: &Value,
    cmd: &str,
    kind: ViewKind,
    default_at: impl FnOnce(&Session, &Drawing) -> [f64; 2],
    label: bool,
) -> Result<Value> {
    let i = find(s, p, cmd)?;
    let d0 = s.doc.drawings.get(i).cloned().ok_or_else(|| bad(cmd, "no such drawing"))?;
    if d0.views.len() >= MAX_ITEMS {
        return Err(bad(cmd, "too many views"));
    }
    let at = default_at(s, &d0);
    change(s, i, |d| {
        let id = d.take_id();
        let mut v = DrawView { id, kind, at, scale: None, label: if label { d.next_label() } else { String::new() }, hidden: false };
        if let Some(l) = str_(p, "label").map(str::trim).filter(|l| !l.is_empty() && l.len() <= 8) {
            v.label = l.to_string();
        }
        common_view_params(&mut v, p, cmd)?;
        d.views.push(v);
        Ok(json!({"view": id}))
    })
}

fn parent_of(s: &Session, p: &Value, cmd: &str) -> Result<u64> {
    let i = find(s, p, cmd)?;
    let parent = id_of(p, "parent", cmd)?;
    s.doc.drawings.get(i).and_then(|d| d.view(parent)).map(|v| v.id).ok_or_else(|| bad(cmd, "no such parent view"))
}

/// Where a new view goes next to `parent` on `side` (a gap of 25 mm past the parent's frame).
fn beside(s: &Session, d: &Drawing, parent: u64, side: &str) -> [f64; 2] {
    let drawn = dv::draw(s, d);
    let Some(pv) = drawn.views.iter().find(|v| v.id == parent) else {
        return d.view(parent).map(|v| v.at).unwrap_or([100.0, 100.0]);
    };
    let c = Vec2::new((pv.min.x + pv.max.x) / 2.0, (pv.min.y + pv.max.y) / 2.0);
    let (hw, hh) = ((pv.max.x - pv.min.x) / 2.0, (pv.max.y - pv.min.y) / 2.0);
    let at = match side {
        "left" => c - Vec2::new(2.0 * hw + 25.0, 0.0),
        "above" => c + Vec2::new(0.0, 2.0 * hh + 25.0),
        "below" => c - Vec2::new(0.0, 2.0 * hh + 25.0),
        _ => c + Vec2::new(2.0 * hw + 25.0, 0.0),
    };
    [at.x, at.y]
}

fn view_base(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "drawing.view.base";
    let o = str_(p, "orientation").unwrap_or("front").trim().to_ascii_lowercase();
    if dv::orientation(&o).is_none() {
        return Err(bad(cmd, "`orientation` must be front, back, top, bottom, left, right or iso"));
    }
    add_view(
        s,
        p,
        cmd,
        ViewKind::Base { orientation: o },
        |_, d| {
            let (x0, y0, x1, y1) = area(d);
            [(x0 + x1) / 2.0, (y0 + y1) / 2.0]
        },
        false,
    )
}

fn view_projected(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "drawing.view.projected";
    let parent = parent_of(s, p, cmd)?;
    let side = str_(p, "side").unwrap_or("right").trim().to_ascii_lowercase();
    if !["right", "left", "above", "below", "iso"].contains(&side.as_str()) {
        return Err(bad(cmd, "`side` must be right, left, above, below or iso"));
    }
    let place = if side == "iso" { "right".to_string() } else { side.clone() };
    add_view(s, p, cmd, ViewKind::Projected { parent, side }, |s, d| beside(s, d, parent, &place), false)
}

/// A sheet point in a view's own coordinates (model mm about its centre).
fn to_view(d: &Drawing, view: u64, at: [f64; 2]) -> Option<[f64; 2]> {
    let v = d.view(view)?;
    let s = d.view_scale(v);
    Some([(at[0] - v.at[0]) / s, (at[1] - v.at[1]) / s])
}

fn view_section(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "drawing.view.section";
    let parent = parent_of(s, p, cmd)?;
    let line = p.get("line").and_then(Value::as_array).ok_or_else(|| bad(cmd, "`line` must be two sheet points [[x, y], [x, y]]"))?;
    let (Some(a), Some(b)) = (point(line.first()), point(line.get(1))) else { return Err(bad(cmd, "`line` must be two sheet points")) };
    if (a[0] - b[0]).hypot(a[1] - b[1]) < 1e-3 {
        return Err(bad(cmd, "the cutting line has no length"));
    }
    let i = find(s, p, cmd)?;
    let d = s.doc.drawings.get(i).ok_or_else(|| bad(cmd, "no such drawing"))?;
    let (Some(va), Some(vb)) = (to_view(d, parent, a), to_view(d, parent, b)) else { return Err(bad(cmd, "no such parent view")) };
    let flip = bool_(p, "flip").unwrap_or(false);
    // Beside the parent, on the side the arrows point to: a vertical line, to the right.
    let side = if (a[0] - b[0]).abs() < (a[1] - b[1]).abs() { "right" } else { "above" };
    add_view(s, p, cmd, ViewKind::Section { parent, a: va, b: vb, flip }, |s, d| beside(s, d, parent, side), true)
}

fn view_detail(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "drawing.view.detail";
    let parent = parent_of(s, p, cmd)?;
    let c = point(p.get("center")).ok_or_else(|| bad(cmd, "`center` must be [x, y] on the sheet"))?;
    let r = num(p, "radius").filter(|r| *r > 0.1 && *r < 1e4).ok_or_else(|| bad(cmd, "`radius` must be a positive length (sheet mm)"))?;
    let i = find(s, p, cmd)?;
    let d = s.doc.drawings.get(i).ok_or_else(|| bad(cmd, "no such drawing"))?;
    let center = to_view(d, parent, c).ok_or_else(|| bad(cmd, "no such parent view"))?;
    let ps = d.view(parent).map(|v| d.view_scale(v)).unwrap_or(1.0);
    add_view(s, p, cmd, ViewKind::Detail { parent, center, radius: r / ps }, |s, d| beside(s, d, parent, "right"), true)
}

fn view_index(d: &Drawing, p: &Value, cmd: &str) -> Result<usize> {
    let id = id_of(p, "view", cmd)?;
    d.views.iter().position(|v| v.id == id).ok_or_else(|| bad(cmd, "no such view"))
}

fn view_edit(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "drawing.view.edit";
    let i = find(s, p, cmd)?;
    change(s, i, |d| {
        let vi = view_index(d, p, cmd)?;
        let v = d.views.get_mut(vi).ok_or_else(|| bad(cmd, "no such view"))?;
        common_view_params(v, p, cmd)?;
        if let Some(f) = bool_(p, "flip") {
            match &mut v.kind {
                ViewKind::Section { flip, .. } => *flip = f,
                _ => return Err(bad(cmd, "only a section view flips")),
            }
        }
        if let Some(o) = str_(p, "orientation") {
            let o = o.trim().to_ascii_lowercase();
            match &mut v.kind {
                ViewKind::Base { orientation } if dv::orientation(&o).is_some() => *orientation = o,
                ViewKind::Base { .. } => return Err(bad(cmd, "unknown orientation")),
                _ => return Err(bad(cmd, "only a base view has an orientation")),
            }
        }
        if let Some(l) = str_(p, "label") {
            if l.len() > 8 {
                return Err(bad(cmd, "`label` is at most 8 characters"));
            }
            v.label = l.trim().to_string();
        }
        Ok(json!({"view": v.id}))
    })
}

fn view_delete(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "drawing.view.delete";
    let i = find(s, p, cmd)?;
    change(s, i, |d| {
        let vi = view_index(d, p, cmd)?;
        let id = d.views.get(vi).map(|v| v.id).unwrap_or(0);
        let gone = d.descendants(id);
        d.views.retain(|v| !gone.contains(&v.id));
        d.dims.retain(|x| !gone.contains(&x.view));
        Ok(json!({"deleted": gone}))
    })
}

fn kind_of(p: &Value, cmd: &str) -> Result<DimKind> {
    Ok(match str_(p, "kind").unwrap_or("horizontal").trim().to_ascii_lowercase().as_str() {
        "horizontal" => DimKind::Horizontal,
        "vertical" => DimKind::Vertical,
        "aligned" | "linear" => DimKind::Aligned,
        "diameter" => DimKind::Diameter,
        "radius" => DimKind::Radius,
        "angle" | "angular" => DimKind::Angle,
        _ => return Err(bad(cmd, "`kind` must be horizontal, vertical, aligned, diameter, radius or angle")),
    })
}

fn text_param(p: &Value, k: &str, cmd: &str) -> Result<Option<String>> {
    match p.get(k) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(t)) if t.len() <= 200 => Ok(Some(t.clone())),
        Some(_) => Err(bad(cmd, format!("`{k}` must be text (at most 200 characters)"))),
    }
}

fn dimension(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "drawing.dimension";
    let i = find(s, p, cmd)?;
    let view = id_of(p, "view", cmd)?;
    let kind = kind_of(p, cmd)?;
    let pts: Vec<[f64; 2]> = p.get("points").and_then(Value::as_array).map(|a| a.iter().filter_map(|v| point(Some(v))).collect()).unwrap_or_default();
    let want = match kind {
        DimKind::Diameter | DimKind::Radius => 1,
        DimKind::Angle => 3,
        _ => 2,
    };
    if pts.len() != want {
        return Err(bad(cmd, format!("a {kind:?} dimension takes {want} point(s)").to_lowercase()));
    }
    let d = s.doc.drawings.get(i).cloned().ok_or_else(|| bad(cmd, "no such drawing"))?;
    if d.dims.len() >= MAX_ITEMS {
        return Err(bad(cmd, "too many dimensions"));
    }
    let drawn = dv::draw(s, &d);
    let pv = drawn.views.iter().find(|v| v.id == view).ok_or_else(|| bad(cmd, "no such view (or it couldn't be drawn)"))?;
    let arc_given = arc_param(p, cmd)?;
    let arc = arc_given.clone().unwrap_or_default();
    if arc_given.is_some() && !matches!(kind, DimKind::Horizontal | DimKind::Vertical | DimKind::Aligned) {
        return Err(bad(cmd, "`arc` is for horizontal, vertical and aligned dimensions"));
    }
    // A diameter or radius, and a dimension to the sides of arcs, take their points on circles;
    // one to virtual sharps, at the corners rounds would leave without them.
    let sharps = arc == "sharp";
    let on_arcs = arc_given.is_some() && !sharps;
    let circles = matches!(kind, DimKind::Diameter | DimKind::Radius) || on_arcs;
    let mut refs: Vec<DimRef> = Vec::new();
    for (k, at) in pts.iter().enumerate() {
        let mode = if sharps {
            dv::PickMode::Sharps
        } else if circles && (k == 0 || on_arcs) {
            dv::PickMode::Circles
        } else {
            dv::PickMode::Points
        };
        let snap = dv::pick(pv, Vec2::new(at[0], at[1]), 6.0, mode).ok_or_else(|| {
            bad(
                cmd,
                match mode {
                    dv::PickMode::Circles => "no circle edge there".to_string(),
                    dv::PickMode::Sharps => format!("no rounded corner near ({:.1}, {:.1})", at[0], at[1]),
                    dv::PickMode::Points => format!("no model point near ({:.1}, {:.1})", at[0], at[1]),
                },
            )
        })?;
        refs.push(snap.r);
    }
    if (!circles || on_arcs) && kind != DimKind::Angle && refs.first().map(|r| r.point) == refs.get(1).map(|r| r.point) {
        return Err(bad(cmd, "both points are the same"));
    }
    let offset = match p.get("offset") {
        None | Some(Value::Null) => 10.0,
        Some(_) => num(p, "offset").filter(|o| o.abs() < 1e4).ok_or_else(|| bad(cmd, "`offset` must be a length below 10 m"))?,
    };
    let text = text_param(p, "text", cmd)?.unwrap_or_default();
    let tolerance = text_param(p, "tolerance", cmd)?.unwrap_or_default();
    change(s, i, |d| {
        let id = d.take_id();
        d.dims.push(DrawDim { id, view, kind, refs, offset, text, tolerance, arc });
        Ok(json!({"dimension": id}))
    })
}

/// `arc`: center (none), min (the near sides of the arcs) or max (their far sides).
fn arc_param(p: &Value, cmd: &str) -> Result<Option<String>> {
    match p.get("arc") {
        None | Some(Value::Null) => Ok(None),
        Some(v) => match v.as_str().map(|s| s.trim().to_ascii_lowercase()) {
            Some(s) if s == "center" || s == "centre" || s.is_empty() => Ok(Some(String::new())),
            Some(s) if s == "min" || s == "max" || s == "sharp" => Ok(Some(s)),
            _ => Err(bad(cmd, "`arc` must be center, min, max or sharp")),
        },
    }
}

fn dim_index(d: &Drawing, p: &Value, cmd: &str) -> Result<usize> {
    let id = id_of(p, "dimension", cmd)?;
    d.dims.iter().position(|x| x.id == id).ok_or_else(|| bad(cmd, "no such dimension"))
}

/// `drawing.dimension` with its kind (and arc side) set.
fn dim_as(s: &mut Session, p: &Value, kind: &str, arc: Option<&str>) -> Result<Value> {
    let mut q = p.clone();
    if let Value::Object(m) = &mut q {
        if arc.is_none() || !m.contains_key("kind") {
            m.insert("kind".into(), json!(kind));
        }
        if let Some(a) = arc {
            m.entry("arc").or_insert_with(|| json!(a));
        }
    }
    dimension(s, &q)
}

fn dim_horizontal(s: &mut Session, p: &Value) -> Result<Value> {
    dim_as(s, p, "horizontal", None)
}
fn dim_vertical(s: &mut Session, p: &Value) -> Result<Value> {
    dim_as(s, p, "vertical", None)
}
fn dim_aligned(s: &mut Session, p: &Value) -> Result<Value> {
    dim_as(s, p, "aligned", None)
}
fn dim_radius(s: &mut Session, p: &Value) -> Result<Value> {
    dim_as(s, p, "radius", None)
}
fn dim_diameter(s: &mut Session, p: &Value) -> Result<Value> {
    dim_as(s, p, "diameter", None)
}
fn dim_angle(s: &mut Session, p: &Value) -> Result<Value> {
    dim_as(s, p, "angle", None)
}
fn dim_arcs(s: &mut Session, p: &Value) -> Result<Value> {
    dim_as(s, p, "horizontal", Some("max"))
}

fn dimension_edit(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "drawing.dimension.edit";
    let i = find(s, p, cmd)?;
    change(s, i, |d| {
        let k = dim_index(d, p, cmd)?;
        let x = d.dims.get_mut(k).ok_or_else(|| bad(cmd, "no such dimension"))?;
        if let Some(o) = num(p, "offset") {
            if o.abs() >= 1e4 {
                return Err(bad(cmd, "`offset` is too large"));
            }
            x.offset = o;
        }
        if let Some(t) = text_param(p, "text", cmd)? {
            x.text = t;
        }
        if let Some(t) = text_param(p, "tolerance", cmd)? {
            x.tolerance = t;
        }
        if let Some(a) = arc_param(p, cmd)? {
            x.arc = a;
        }
        Ok(json!({"dimension": x.id}))
    })
}

fn dimension_delete(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "drawing.dimension.delete";
    let i = find(s, p, cmd)?;
    change(s, i, |d| {
        let k = dim_index(d, p, cmd)?;
        let x = d.dims.remove(k);
        Ok(json!({"deleted": x.id}))
    })
}

fn note_height(p: &Value, cmd: &str) -> Result<Option<f64>> {
    match num(p, "height") {
        None if p.get("height").is_none_or(Value::is_null) => Ok(None),
        Some(h) if (1.0..=50.0).contains(&h) => Ok(Some(h)),
        _ => Err(bad(cmd, "`height` must be 1–50 mm")),
    }
}

fn note(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "drawing.note";
    let i = find(s, p, cmd)?;
    let at = point(p.get("at")).ok_or_else(|| bad(cmd, "`at` must be [x, y] on the sheet"))?;
    let text = str_(p, "text").filter(|t| !t.trim().is_empty() && t.len() <= 4000).ok_or_else(|| bad(cmd, "`text` is required"))?.to_string();
    let height = note_height(p, cmd)?.unwrap_or(3.5);
    change(s, i, |d| {
        if d.notes.len() >= MAX_ITEMS {
            return Err(bad(cmd, "too many notes"));
        }
        let id = d.take_id();
        d.notes.push(DrawNote { id, at, text, height });
        Ok(json!({"note": id}))
    })
}

fn note_edit(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "drawing.note.edit";
    let i = find(s, p, cmd)?;
    let id = id_of(p, "note", cmd)?;
    let h = note_height(p, cmd)?;
    change(s, i, |d| {
        let n = d.notes.iter_mut().find(|n| n.id == id).ok_or_else(|| bad(cmd, "no such note"))?;
        if let Some(at) = p.get("at") {
            n.at = point(Some(at)).ok_or_else(|| bad(cmd, "`at` must be [x, y]"))?;
        }
        if let Some(t) = str_(p, "text") {
            if t.trim().is_empty() || t.len() > 4000 {
                return Err(bad(cmd, "`text` must be 1–4000 characters"));
            }
            n.text = t.to_string();
        }
        if let Some(h) = h {
            n.height = h;
        }
        Ok(json!({"note": id}))
    })
}

fn note_delete(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "drawing.note.delete";
    let i = find(s, p, cmd)?;
    let id = id_of(p, "note", cmd)?;
    change(s, i, |d| {
        let before = d.notes.len();
        d.notes.retain(|n| n.id != id);
        if d.notes.len() == before {
            return Err(bad(cmd, "no such note"));
        }
        Ok(json!({"deleted": id}))
    })
}

fn render(s: &mut Session, p: &Value) -> Result<Value> {
    let i = find(s, p, "drawing.render")?;
    let d = s.doc.drawings.get(i).ok_or_else(|| bad("drawing.render", "no such drawing"))?;
    let drawn = dv::draw(s, d);
    Ok(serde_json::to_value(&drawn).unwrap_or(Value::Null))
}

fn dash(style: Style) -> Vec<f64> {
    match style {
        Style::Hidden => vec![3.0, 1.5],
        Style::Chain => vec![12.0, 1.5, 0.5, 1.5],
        _ => Vec::new(),
    }
}

/// The PDF of a drawn sheet.
pub fn pdf_of(drawn: &SheetDrawn, title: &str) -> Vec<u8> {
    use solidcraft_io::pdf::{PdfAlign, PdfItem};
    let items: Vec<PdfItem> = drawn
        .prims
        .iter()
        .map(|p| match p {
            Prim::Line { a, b, style } => PdfItem::Line { a: *a, b: *b, width: style.width(), dash: dash(*style) },
            Prim::Fill { points } => PdfItem::Fill { points: points.clone() },
            Prim::Text { at, height, angle_deg, text, align } => PdfItem::Text {
                at: *at,
                height: *height,
                angle_deg: *angle_deg,
                text: text.clone(),
                align: match align {
                    Align::Left => PdfAlign::Left,
                    Align::Center => PdfAlign::Center,
                    Align::Right => PdfAlign::Right,
                },
            },
        })
        .collect();
    solidcraft_io::pdf::write_pdf(drawn.width, drawn.height, &items, title)
}

/// The DXF of a drawn sheet (1:1 on the sheet, mm).
pub fn dxf_of(drawn: &SheetDrawn) -> String {
    use solidcraft_io::{DxfEntity, DxfLayer, DxfLinetype, DxfShape};
    let layers = vec![
        DxfLayer::new("VISIBLE", 7),
        DxfLayer { linetype: DxfLinetype::Hidden, ..DxfLayer::new("HIDDEN", 8) },
        DxfLayer::new("THIN", 3),
        DxfLayer { linetype: DxfLinetype::Center, ..DxfLayer::new("CENTER", 1) },
        DxfLayer::new("FRAME", 7),
        DxfLayer::new("TEXT", 2),
    ];
    let mut out = Vec::new();
    for p in &drawn.prims {
        match p {
            Prim::Line { a, b, style } => {
                let layer = match style {
                    Style::Visible => "VISIBLE",
                    Style::Hidden => "HIDDEN",
                    Style::Thin => "THIN",
                    Style::Chain => "CENTER",
                    Style::Wide => "FRAME",
                };
                out.push(DxfEntity { layer: layer.into(), shape: DxfShape::Line(*a, *b) });
            }
            Prim::Fill { points } => {
                let mut pts = points.clone();
                if let Some(f) = points.first() {
                    pts.push(*f);
                }
                out.push(DxfEntity { layer: "THIN".into(), shape: DxfShape::Polyline { points: pts } });
            }
            Prim::Text { at, height, angle_deg, text, align } => {
                // DXF TEXT is centred on its point: move the point to the text's middle.
                let w = solidcraft_io::pdf::text_width(text, *height);
                let shift = match align {
                    Align::Left => w / 2.0,
                    Align::Center => 0.0,
                    Align::Right => -w / 2.0,
                };
                let (s, c) = angle_deg.to_radians().sin_cos();
                let t = text.replace('Ø', "%%c").replace('±', "%%p").replace('°', "%%d");
                out.push(DxfEntity {
                    layer: "TEXT".into(),
                    shape: DxfShape::Text { at: *at + Vec2::new(c, s) * shift, height: *height, angle_deg: *angle_deg, text: t },
                });
            }
        }
    }
    solidcraft_io::write_dxf_drawing(&layers, &out)
}

fn path_param<'a>(p: &'a Value, cmd: &str) -> Result<Option<&'a str>> {
    match p.get("path") {
        None | Some(Value::Null) => Ok(None),
        Some(_) => str_(p, "path").map(str::trim).filter(|v| !v.is_empty() && v.len() < 4096).map(Some).ok_or_else(|| bad(cmd, "invalid path")),
    }
}

fn export_pdf(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "drawing.export_pdf";
    let i = find(s, p, cmd)?;
    let d = s.doc.drawings.get(i).ok_or_else(|| bad(cmd, "no such drawing"))?;
    let drawn = dv::draw(s, d);
    let title = if d.title.title.trim().is_empty() { d.name.clone() } else { d.title.title.clone() };
    let bytes = pdf_of(&drawn, &title);
    let errors: Vec<Value> = drawn.errors.iter().map(|(id, e)| json!({"id": id, "error": e})).collect();
    match path_param(p, cmd)? {
        Some(path) => {
            solidcraft_io::vfs::write(path, &bytes).map_err(|e| EngineError::Other(format!("{path}: {e}")))?;
            Ok(json!({"path": path, "bytes": bytes.len(), "errors": errors}))
        }
        None => Ok(json!({"pdf_base64": solidcraft_doc::canvas::base64_encode(&bytes), "bytes": bytes.len(), "errors": errors})),
    }
}

fn export_dxf(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "drawing.export_dxf";
    let i = find(s, p, cmd)?;
    let d = s.doc.drawings.get(i).ok_or_else(|| bad(cmd, "no such drawing"))?;
    let drawn = dv::draw(s, d);
    let text = dxf_of(&drawn);
    let errors: Vec<Value> = drawn.errors.iter().map(|(id, e)| json!({"id": id, "error": e})).collect();
    match path_param(p, cmd)? {
        Some(path) => {
            solidcraft_io::vfs::write(path, text.as_bytes()).map_err(|e| EngineError::Other(format!("{path}: {e}")))?;
            Ok(json!({"path": path, "errors": errors}))
        }
        None => Ok(json!({"dxf": text, "errors": errors})),
    }
}

#[cfg(test)]
#[path = "drawing_tests.rs"]
mod tests;
