//! The drawing editor: while it is open the viewport shows the drawing's sheet (paper white,
//! lines as they print) instead of the model, and the panel holds the sheet, the title block,
//! the views and the annotation tools. Everything it changes goes through `drawing.*` commands.
//!
//! On the sheet: a click selects a view, a drag moves it; the tools take their points by clicks
//! (a section: the cutting line's two ends; a detail: its centre, then a point on its circle;
//! a dimension: its points, snapped to the model; a note: where it goes). The camera is the
//! shared flat navigation (pan and zoom, no orbit).

use egui::{Align2, Color32, FontId, Pos2, Rect, RichText, Stroke, pos2, vec2};
use serde_json::{Value, json};
use solidcraft_engine::doc::drawing::{SCALES, SHEET_SIZES};

use crate::SolveApp;
use crate::dialogs::{Dialog, Kind, combo, row_label};
use crate::theme::Tokens;

const ORIENTATIONS: [&str; 7] = ["front", "back", "top", "bottom", "left", "right", "iso"];
const ORIENTATION_LABELS: [&str; 7] = ["Front", "Back", "Top", "Bottom", "Left", "Right", "Iso"];
const SIDES: [&str; 5] = ["right", "left", "above", "below", "iso"];
const SIDE_LABELS: [&str; 5] = ["Right", "Left", "Above", "Below", "Iso"];
const DIM_KINDS: [&str; 6] = ["horizontal", "vertical", "aligned", "diameter", "radius", "angle"];
const DIM_LABELS: [&str; 6] = ["Horizontal", "Vertical", "Aligned", "Diameter", "Radius", "Angle"];
/// Where a linear dimension meets the arcs it measures to.
/// What a linear dimension's points snap to: edge ends, or circles and arcs (their centres, near
/// or far sides), or virtual sharps (the corners rounds took away). Index 0 sends no `arc`.
const ARCS: [&str; 5] = ["", "center", "min", "max", "sharp"];
const ARC_LABELS: [&str; 5] = ["Edge ends", "Circle centres", "Near sides (min)", "Far sides (max)", "Virtual sharp (no round)"];
const TITLE_FIELDS: [(&str, &str); 9] = [
    ("title", "Title"),
    ("number", "Drawing no."),
    ("company", "Company"),
    ("drawn_by", "Drawn by"),
    ("checked_by", "Checked by"),
    ("date", "Date"),
    ("material", "Material"),
    ("revision", "Revision"),
    ("tolerances", "Tolerances"),
];

/// What clicks on the sheet do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Select,
    Section,
    Detail,
    Dimension,
    Note,
}

#[derive(Clone, Debug)]
pub struct DrawUi {
    /// The drawing open (its id).
    pub drawing: Option<u64>,
    pub tool: Tool,
    /// Sheet points the tool has taken so far.
    pub picks: Vec<[f64; 2]>,
    pub selected: Option<u64>,
    pub orientation: usize,
    pub side: usize,
    pub dim_kind: usize,
    /// Index into `ARCS`.
    pub arc: usize,
    pub tolerance: String,
    pub note: String,
    /// The title block's fields as edited (written when a field is left).
    pub title: Vec<String>,
    /// The drawn sheet: (revision, drawing) it was drawn for and the result.
    pub drawn: Option<((u64, u64), Value)>,
    /// A view being dragged: its id, where the drag started (sheet mm) and its place then.
    pub drag: Option<(u64, [f64; 2], [f64; 2])>,
    pub status: Option<String>,
}

impl DrawUi {
    fn new(tool: Tool) -> DrawUi {
        DrawUi {
            drawing: None,
            tool,
            picks: Vec::new(),
            selected: None,
            orientation: 0,
            side: 0,
            dim_kind: 0,
            arc: 0,
            tolerance: String::new(),
            note: String::new(),
            title: Vec::new(),
            drawn: None,
            drag: None,
            status: None,
        }
    }
}

/// The editor for a toolbar command of the DRAWING tab.
pub fn start(_app: &SolveApp, id: &str) -> Option<(Kind, Vec<crate::selection::SelInput>)> {
    let tool = match id {
        "drawing.create" | "drawing.edit" | "drawing.view.base" | "drawing.view.projected" | "drawing.export_pdf" | "drawing.export_dxf" => {
            Tool::Select
        }
        "drawing.view.section" => Tool::Section,
        "drawing.view.detail" => Tool::Detail,
        "drawing.note" => Tool::Note,
        _ if id.starts_with("drawing.dimension") => Tool::Dimension,
        _ => return None,
    };
    let mut ui = DrawUi::new(tool);
    // The ribbon's dimension kinds: the kind (and for Arc to Arc, the far sides of two arcs).
    let (kind, arc) = match id {
        "drawing.dimension.vertical" => (1, 0),
        "drawing.dimension.aligned" => (2, 0),
        "drawing.dimension.diameter" => (3, 0),
        "drawing.dimension.radius" => (4, 0),
        "drawing.dimension.angle" => (5, 0),
        "drawing.dimension.arcs" => (0, 3),
        _ => (0, 0),
    };
    ui.dim_kind = kind;
    ui.arc = arc;
    ui.status = Some(if id == "drawing.create" { "new".into() } else { String::new() });
    Some((Kind::Drawing(Box::new(ui)), vec![]))
}

/// The drawing editor is open: the viewport shows the sheet.
#[cfg(test)]
pub(crate) fn active(app: &SolveApp) -> bool {
    matches!(app.dialog.as_ref().map(|d| &d.kind), Some(Kind::Drawing(_)))
}

fn drawing<'a>(app: &'a SolveApp, k: &DrawUi) -> Option<&'a solidcraft_engine::doc::drawing::Drawing> {
    k.drawing.and_then(|id| app.session.doc.drawings.iter().find(|d| d.id == id))
}

fn run(app: &mut SolveApp, k: &mut DrawUi, cmd: &str, p: Value) -> Option<Value> {
    match app.run(cmd, p) {
        Ok(v) => {
            k.status = None;
            Some(v)
        }
        Err(e) => {
            k.status = Some(e);
            None
        }
    }
}

/// The panel's rows.
pub fn rows(app: &mut SolveApp, ui: &mut egui::Ui, k: &mut DrawUi) -> bool {
    let t = Tokens::get();
    // The drawing: a new one when the editor was opened for one (or there is none yet).
    let wants_new = k.status.as_deref() == Some("new");
    if wants_new {
        k.status = None;
    }
    if k.drawing.is_none_or(|id| !app.session.doc.drawings.iter().any(|d| d.id == id)) {
        k.drawing = if wants_new || app.session.doc.drawings.is_empty() {
            if app.session.world_state().bodies.is_empty() {
                k.status = Some("Model a part first: a drawing shows its bodies".into());
                None
            } else {
                run(app, k, "drawing.create", json!({})).and_then(|v| v["drawing"].as_u64())
            }
        } else {
            app.session.doc.drawings.last().map(|d| d.id)
        };
        k.title.clear();
    }
    let names: Vec<String> = app.session.doc.drawings.iter().map(|d| d.name.clone()).chain(std::iter::once("New drawing…".to_string())).collect();
    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
    let mut pick = app.session.doc.drawings.iter().position(|d| Some(d.id) == k.drawing).unwrap_or(0);
    let before = pick;
    row_label(ui, "Drawing");
    combo(ui, "drw_pick", &refs, &mut pick);
    ui.end_row();
    if pick != before {
        k.drawing = match app.session.doc.drawings.get(pick) {
            Some(d) => Some(d.id),
            None => run(app, k, "drawing.create", json!({})).and_then(|v| v["drawing"].as_u64()),
        };
        k.title.clear();
        k.selected = None;
    }
    let Some(d) = drawing(app, k).cloned() else {
        if let Some(s) = &k.status {
            row_label(ui, "");
            ui.label(RichText::new(s).color(t.error));
            ui.end_row();
        }
        return false;
    };
    let id = d.id;

    // ---- sheet ----
    section(ui, "SHEET");
    let sizes: Vec<&str> = SHEET_SIZES.iter().map(|(n, ..)| *n).collect();
    let mut size = sizes.iter().position(|s| s.eq_ignore_ascii_case(&d.sheet.size)).unwrap_or(1);
    let b = size;
    row_label(ui, "Size");
    combo(ui, "drw_size", &sizes, &mut size);
    ui.end_row();
    if size != b {
        run(app, k, "drawing.edit", json!({"drawing": id, "size": sizes.get(size)}));
    }
    let mut landscape = d.sheet.landscape;
    row_label(ui, "Landscape");
    if ui.checkbox(&mut landscape, "").changed() {
        run(app, k, "drawing.edit", json!({"drawing": id, "landscape": landscape}));
    }
    ui.end_row();
    let mut scale = SCALES.iter().position(|s| *s == d.scale).unwrap_or(3);
    let b = scale;
    row_label(ui, "Scale");
    combo(ui, "drw_scale", SCALES, &mut scale);
    ui.end_row();
    if scale != b {
        run(app, k, "drawing.edit", json!({"drawing": id, "scale": SCALES.get(scale)}));
    }
    let projections = ["First angle (ISO E)", "Third angle (ISO A)"];
    let mut proj = usize::from(d.projection == solidcraft_engine::doc::drawing::Projection::Third);
    let b = proj;
    row_label(ui, "Projection");
    combo(ui, "drw_proj", &projections, &mut proj);
    ui.end_row();
    if proj != b {
        run(app, k, "drawing.edit", json!({"drawing": id, "projection": if proj == 1 { "third" } else { "first" }}));
    }

    // ---- title block ----
    section(ui, "TITLE BLOCK");
    if k.title.len() != TITLE_FIELDS.len() {
        let mut tb = d.title.clone();
        k.title = tb.fields_mut().iter().map(|(_, v)| v.to_string()).collect();
    }
    let mut changed: Option<(&str, String)> = None;
    for ((key, label), value) in TITLE_FIELDS.iter().zip(k.title.iter_mut()) {
        row_label(ui, label);
        let r = ui.add(egui::TextEdit::singleline(value).hint_text(if *key == "material" { crate::i18n::tr_label("from the part") } else { "" }));
        crate::scenario::publish_handle(&format!("dialog:title:{key}"), r.rect.center());
        if r.lost_focus() {
            changed = Some((key, value.clone()));
        }
        ui.end_row();
    }
    if let Some((key, value)) = changed {
        let mut title = serde_json::Map::new();
        title.insert(key.into(), json!(value));
        run(app, k, "drawing.edit", json!({"drawing": id, "title": title}));
    }

    // ---- views ----
    section(ui, "VIEWS");
    row_label(ui, "Base view");
    ui.horizontal(|ui| {
        combo(ui, "drw_orient", &ORIENTATION_LABELS, &mut k.orientation);
        if ui.button(crate::i18n::tr_label("Add")).clicked() {
            let o = ORIENTATIONS.get(k.orientation).copied().unwrap_or("front");
            if let Some(v) = run(app, k, "drawing.view.base", json!({"drawing": id, "orientation": o})) {
                k.selected = v["view"].as_u64();
            }
        }
    });
    ui.end_row();
    let selected = k.selected.and_then(|v| d.view(v)).cloned();
    if let Some(v) = &selected {
        row_label(ui, "Projected from it");
        ui.horizontal(|ui| {
            combo(ui, "drw_side", &SIDE_LABELS, &mut k.side);
            if ui.button(crate::i18n::tr_label("Add")).clicked() {
                let side = SIDES.get(k.side).copied().unwrap_or("right");
                if let Some(r) = run(app, k, "drawing.view.projected", json!({"drawing": id, "parent": v.id, "side": side})) {
                    k.selected = r["view"].as_u64();
                }
            }
        });
        ui.end_row();
        let mut hidden = v.hidden;
        row_label(ui, "Hidden edges");
        if ui.checkbox(&mut hidden, "").changed() {
            run(app, k, "drawing.view.edit", json!({"drawing": id, "view": v.id, "hidden": hidden}));
        }
        ui.end_row();
        let own: Vec<&str> = std::iter::once("Sheet's").chain(SCALES.iter().copied()).collect();
        let mut sc = v.scale.as_deref().and_then(|s| SCALES.iter().position(|x| *x == s)).map_or(0, |i| i + 1);
        let b = sc;
        row_label(ui, "View scale");
        combo(ui, "drw_vscale", &own, &mut sc);
        ui.end_row();
        if sc != b {
            let text = sc.checked_sub(1).and_then(|i| SCALES.get(i)).copied().unwrap_or("");
            run(app, k, "drawing.view.edit", json!({"drawing": id, "view": v.id, "scale": text}));
        }
        row_label(ui, "");
        if ui.button(crate::i18n::tr_label("Delete view")).clicked() {
            run(app, k, "drawing.view.delete", json!({"drawing": id, "view": v.id}));
            k.selected = None;
        }
        ui.end_row();
    } else {
        row_label(ui, "");
        ui.label(RichText::new(crate::i18n::tr_label("Click a view to select it, drag it to move it")).weak());
        ui.end_row();
    }

    // ---- tools ----
    section(ui, "TOOLS");
    row_label(ui, "On the sheet");
    ui.horizontal_wrapped(|ui| {
        for (tool, label) in
            [(Tool::Select, "Select"), (Tool::Section, "Section"), (Tool::Detail, "Detail"), (Tool::Dimension, "Dimension"), (Tool::Note, "Note")]
        {
            let r = ui.selectable_label(k.tool == tool, crate::i18n::tr_label(label));
            crate::scenario::publish_handle(&format!("dialog:tool:{label}"), r.rect.center());
            if r.clicked() {
                k.tool = tool;
                k.picks.clear();
            }
        }
    });
    ui.end_row();
    match k.tool {
        Tool::Dimension => {
            row_label(ui, "Kind");
            combo(ui, "drw_dim", &DIM_LABELS, &mut k.dim_kind);
            ui.end_row();
            if k.dim_kind <= 2 {
                // To circles and arcs: their centres, or their near or far sides.
                row_label(ui, "Points on");
                combo(ui, "drw_arc", &ARC_LABELS, &mut k.arc);
                ui.end_row();
            } else {
                k.arc = 0;
            }
            row_label(ui, "Tolerance");
            ui.add(egui::TextEdit::singleline(&mut k.tolerance).hint_text("±0.1, +0.2/-0.1, H7"));
            ui.end_row();
        }
        Tool::Note => {
            row_label(ui, "Text");
            ui.add(egui::TextEdit::multiline(&mut k.note).desired_rows(2));
            ui.end_row();
        }
        _ => {}
    }
    row_label(ui, "");
    ui.label(RichText::new(crate::i18n::tr_label(hint(k))).weak());
    ui.end_row();

    // ---- export ----
    section(ui, "EXPORT");
    row_label(ui, "");
    ui.horizontal(|ui| {
        for (ext, cmd) in [("pdf", "drawing.export_pdf"), ("dxf", "drawing.export_dxf")] {
            if ui.button(format!("{}…", ext.to_ascii_uppercase())).clicked() {
                app.pick_save_command(&format!("{}.{ext}", d.name), &[ext], cmd, json!({"drawing": id}));
            }
        }
    });
    ui.end_row();
    if let Some(s) = &k.status {
        row_label(ui, "");
        ui.label(RichText::new(s).color(t.error));
        ui.end_row();
    }
    false
}

fn section(ui: &mut egui::Ui, title: &str) {
    let t = Tokens::get();
    row_label(ui, "");
    ui.label(RichText::new(crate::i18n::tr_label(title)).size(11.0).strong().color(t.text_dim));
    ui.end_row();
}

/// What the tool wants next.
fn hint(k: &DrawUi) -> &'static str {
    match (k.tool, k.picks.len()) {
        (Tool::Select, _) => "Click a view to select it, drag it to move it",
        (Tool::Section, 0) => "Click the cutting line's start in a view",
        (Tool::Section, _) => "Click the cutting line's end",
        (Tool::Detail, 0) => "Click the centre of the detail in a view",
        (Tool::Detail, _) => "Click a point on the detail's circle",
        (Tool::Dimension, _) if k.arc == 4 && k.dim_kind <= 2 => "Click where the rounded corner would be without its round",
        (Tool::Dimension, _) if k.arc != 0 && k.dim_kind <= 2 => "Click the edge of a circle or arc",
        (Tool::Dimension, 0) => match k.dim_kind {
            3 | 4 => "Click a circle's edge",
            5 => "Click the angle's corner",
            _ => "Click the first point (an edge's end)",
        },
        (Tool::Dimension, _) => "Click the next point",
        (Tool::Note, _) => "Type the text, then click where it goes",
    }
}

/// The drawn sheet (drawn again when the design or the drawing changed).
fn drawn(app: &mut SolveApp, k: &mut DrawUi) -> Option<Value> {
    let id = k.drawing?;
    let key = (app.session.revision, id);
    if k.drawn.as_ref().map(|(kk, _)| *kk) != Some(key) {
        let v = app.session.execute("drawing.render", &json!({"drawing": id})).ok()?;
        k.drawn = Some(((app.session.revision, id), v));
    }
    k.drawn.as_ref().map(|(_, v)| v.clone())
}

fn read2(v: &Value) -> Option<[f64; 2]> {
    Some([v.get(0)?.as_f64()?, v.get(1)?.as_f64()?])
}

/// The view whose frame (10 mm round it) holds a sheet point: the smallest such.
fn view_at(v: &Value, p: [f64; 2]) -> Option<u64> {
    v["views"]
        .as_array()?
        .iter()
        .filter_map(|x| Some((x["id"].as_u64()?, read2(&x["min"])?, read2(&x["max"])?)))
        .filter(|(_, lo, hi)| p[0] >= lo[0] - 10.0 && p[0] <= hi[0] + 10.0 && p[1] >= lo[1] - 10.0 && p[1] <= hi[1] + 10.0)
        .min_by(|a, b| ((a.2[0] - a.1[0]) * (a.2[1] - a.1[1])).total_cmp(&((b.2[0] - b.1[0]) * (b.2[1] - b.1[1]))))
        .map(|(id, ..)| id)
}

/// Paint the sheet over the viewport and handle clicks and drags on it. True when the dialog is
/// the drawing editor.
pub fn overlay(app: &mut SolveApp, painter: &egui::Painter, rect: Rect, d: &mut Dialog) -> bool {
    let Kind::Drawing(k) = &mut d.kind else { return false };
    let t = Tokens::get();
    painter.rect_filled(rect, 0.0, t.panel);
    let Some(v) = drawn(app, k) else {
        painter.text(rect.center(), Align2::CENTER_CENTER, crate::i18n::tr_label("No drawing"), FontId::proportional(14.0), t.text_dim);
        return true;
    };
    let (w, h) = (v["width"].as_f64().unwrap_or(420.0), v["height"].as_f64().unwrap_or(297.0));
    let panel_left = painter
        .ctx()
        .memory(|m| m.area_rect(egui::Id::new(("sc_dialog", "DRAWING"))))
        .map(|panel| panel.left())
        .unwrap_or(rect.right() - crate::frame::DIALOG_MAX_W);
    let area = Rect::from_min_max(rect.min + vec2(20.0, 20.0), pos2(panel_left.min(rect.right()) - 20.0, rect.bottom() - 20.0));
    if !app.flat_view.fitted && area.width() > 10.0 && area.height() > 10.0 {
        app.flat_view.fit(rect, area, (0.0, 0.0), (w, h));
    }
    let cam = app.flat_view.cam;
    let scale = f64::from(rect.height()) / (2.0 * cam.half_height());
    let ox = f64::from(rect.center().x) - cam.target.x * scale;
    let oy = f64::from(rect.center().y) + cam.target.y * scale;
    let to = |p: [f64; 2]| -> Pos2 { pos2((ox + p[0] * scale) as f32, (oy - p[1] * scale) as f32) };
    let from = |p: Pos2| -> [f64; 2] { [(f64::from(p.x) - ox) / scale, (oy - f64::from(p.y)) / scale] };
    // The paper: white, whatever the theme; ink black.
    let paper = Rect::from_two_pos(to([0.0, 0.0]), to([w, h]));
    painter.rect_filled(paper.translate(vec2(3.0, 3.0)), 0.0, Color32::from_black_alpha(60));
    painter.rect_filled(paper, 0.0, Color32::WHITE);
    let ink = Color32::from_gray(20);
    let px = |mm: f64| ((mm * scale) as f32).max(0.6);
    // Views moved by a drag are drawn where they are going.
    let shift = |view: Option<u64>| -> [f64; 2] {
        match (k.drag, view) {
            (Some((id, start, _)), Some(v)) if id == v => {
                let now = painter.ctx().input(|i| i.pointer.latest_pos()).map(&from).unwrap_or(start);
                [now[0] - start[0], now[1] - start[1]]
            }
            _ => [0.0, 0.0],
        }
    };
    let none = [0.0, 0.0];
    for p in v["prims"].as_array().into_iter().flatten() {
        match p["type"].as_str() {
            Some("line") => {
                let (Some(a), Some(b)) = (read2(&p["a"]), read2(&p["b"])) else { continue };
                let (a, b) = (to(a), to(b));
                if !(paper.expand(2000.0).contains(a) || paper.expand(2000.0).contains(b)) {
                    continue;
                }
                let style = p["style"].as_str().unwrap_or("thin");
                let width = match style {
                    "visible" => 0.5,
                    "wide" => 0.7,
                    _ => 0.25,
                };
                let stroke = Stroke::new(px(width), ink);
                match style {
                    "hidden" => painter.extend(egui::Shape::dashed_line(&[a, b], stroke, px(3.0), px(1.5))),
                    "chain" => painter.extend(egui::Shape::dashed_line(&[a, b], stroke, px(12.0), px(2.0))),
                    _ => {
                        painter.line_segment([a, b], stroke);
                    }
                }
            }
            Some("fill") => {
                let pts: Vec<Pos2> = p["points"].as_array().into_iter().flatten().filter_map(read2).map(to).collect();
                if pts.len() >= 3 {
                    painter.add(egui::Shape::convex_polygon(pts, ink, Stroke::NONE));
                }
            }
            Some("text") => {
                let (Some(at), Some(text)) = (read2(&p["at"]), p["text"].as_str()) else { continue };
                let size = (p["height"].as_f64().unwrap_or(3.5) * scale / 0.72) as f32;
                if !(2.0..=400.0).contains(&size) {
                    continue;
                }
                let angle = p["angle_deg"].as_f64().unwrap_or(0.0).to_radians() as f32;
                let galley = painter.layout_no_wrap(text.to_string(), FontId::proportional(size), ink);
                let wpx = galley.size().x;
                let shift = match p["align"].as_str() {
                    Some("center") => -wpx / 2.0,
                    Some("right") => -wpx,
                    _ => 0.0,
                };
                let dir = vec2(angle.cos(), -angle.sin());
                let up = vec2(dir.y, -dir.x);
                let pos = to(at) + dir * shift + up * (size * 0.8);
                painter.add(egui::epaint::TextShape::new(pos, galley, ink).with_angle(-angle));
            }
            _ => {}
        }
    }
    // The selected view's frame, and a dragged one where it will go.
    for x in v["views"].as_array().into_iter().flatten() {
        let (Some(id), Some(lo), Some(hi)) = (x["id"].as_u64(), read2(&x["min"]), read2(&x["max"])) else { continue };
        let s = shift(Some(id));
        if Some(id) == k.selected || s != none {
            let r = Rect::from_two_pos(to([lo[0] - 3.0 + s[0], lo[1] - 3.0 + s[1]]), to([hi[0] + 3.0 + s[0], hi[1] + 3.0 + s[1]]));
            painter.rect_stroke(r, 2.0, Stroke::new(1.5, t.accent), egui::StrokeKind::Outside);
        }
        crate::scenario::publish_handle(&format!("drawing:view:{id}"), to([(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0]));
    }
    for e in v["errors"].as_array().into_iter().flatten() {
        if let Some(msg) = e.get(1).and_then(Value::as_str) {
            painter.text(paper.left_top() + vec2(8.0, -18.0), Align2::LEFT_BOTTOM, msg, FontId::proportional(12.0), t.error);
        }
    }
    // The tool's points so far.
    for p in &k.picks {
        painter.circle_filled(to(*p), 4.0, t.accent);
    }
    crate::scenario::publish_rect("drawing:paper", paper);
    for (name, p) in [("drawing:origin", [0.0, 0.0]), ("drawing:corner", [w, h])] {
        crate::scenario::publish_handle(name, to(p));
    }
    // Input: only where the viewport itself is under the pointer (not the panel or a menu).
    let ctx = painter.ctx().clone();
    let (pos, pressed, released, down) =
        ctx.input(|i| (i.pointer.interact_pos(), i.pointer.primary_pressed(), i.pointer.primary_released(), i.pointer.primary_down()));
    let over = pos.is_some_and(|p| rect.contains(p) && ctx.layer_id_at(p).is_none_or(|l| l.order == egui::Order::Background));
    if let (Some(pos), true) = (pos, over) {
        let at = from(pos);
        if pressed {
            press(app, k, &v, at);
        }
    }
    if released
        && let Some((id, start, at0)) = k.drag.take()
        && let Some(pos) = pos
    {
        let now = from(pos);
        let (dx, dy) = (now[0] - start[0], now[1] - start[1]);
        if dx.hypot(dy) > 0.5
            && let Some(did) = k.drawing
        {
            run(app, k, "drawing.view.edit", json!({"drawing": did, "view": id, "at": [at0[0] + dx, at0[1] + dy]}));
        }
    }
    if down || k.drag.is_some() {
        ctx.request_repaint();
    }
    true
}

/// A press on the sheet at `at` (sheet mm).
fn press(app: &mut SolveApp, k: &mut DrawUi, v: &Value, at: [f64; 2]) {
    let Some(id) = k.drawing else { return };
    match k.tool {
        Tool::Select => {
            k.selected = view_at(v, at);
            if let Some(view) = k.selected
                && let Some(place) = drawing(app, k).and_then(|d| d.view(view)).map(|x| x.at)
            {
                k.drag = Some((view, at, place));
            }
        }
        Tool::Note => {
            if k.note.trim().is_empty() {
                k.status = Some("Type the note's text first".into());
                return;
            }
            let text = k.note.clone();
            if run(app, k, "drawing.note", json!({"drawing": id, "at": at, "text": text})).is_some() {
                k.note.clear();
            }
        }
        Tool::Section | Tool::Detail | Tool::Dimension => {
            k.picks.push(at);
            let first = k.picks.first().copied().unwrap_or(at);
            let Some(view) = view_at(v, first) else {
                k.status = Some("Start inside a view".into());
                k.picks.clear();
                return;
            };
            match k.tool {
                Tool::Section if k.picks.len() == 2 => {
                    let line = [k.picks[0], k.picks[1]];
                    k.picks.clear();
                    if let Some(r) = run(app, k, "drawing.view.section", json!({"drawing": id, "parent": view, "line": line})) {
                        k.selected = r["view"].as_u64();
                        k.tool = Tool::Select;
                    }
                }
                Tool::Detail if k.picks.len() == 2 => {
                    let (c, e) = (k.picks[0], k.picks[1]);
                    k.picks.clear();
                    let radius = (e[0] - c[0]).hypot(e[1] - c[1]);
                    if let Some(r) = run(app, k, "drawing.view.detail", json!({"drawing": id, "parent": view, "center": c, "radius": radius})) {
                        k.selected = r["view"].as_u64();
                        k.tool = Tool::Select;
                    }
                }
                Tool::Dimension => {
                    let kind = DIM_KINDS.get(k.dim_kind).copied().unwrap_or("horizontal");
                    let need = match kind {
                        "diameter" | "radius" => 1,
                        "angle" => 3,
                        _ => 2,
                    };
                    if k.picks.len() >= need {
                        let points = std::mem::take(&mut k.picks);
                        let mut p = json!({"drawing": id, "view": view, "kind": kind, "points": points});
                        if !k.tolerance.trim().is_empty() {
                            p["tolerance"] = json!(k.tolerance.trim());
                        }
                        if k.arc != 0 && need == 2 {
                            p["arc"] = json!(ARCS.get(k.arc).copied().unwrap_or(""));
                        }
                        run(app, k, "drawing.dimension", p);
                    }
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// A 100 × 50 × 20 plate with a 20 mm hole, and the drawing editor opened on it.
    fn editor() -> crate::scenario::Harness {
        let mut h = crate::scenario::Harness::new();
        h.app.run("solid.box", json!({"length": 100, "width": 50, "height": 20})).unwrap();
        h.app.run("solid.cylinder", json!({"diameter": 20, "height": 30, "base": [50, 25, -5], "operation": "cut"})).unwrap();
        h.step(&json!({"start": "drawing.create"})).unwrap();
        h.frames(4);
        h
    }

    /// Screen point of a sheet point, from the paper's published corners.
    fn screen(p: [f64; 2]) -> [f64; 2] {
        let o = crate::scenario::handle_at("drawing:origin").unwrap();
        let c = crate::scenario::handle_at("drawing:corner").unwrap();
        // The sheet is 420 mm wide on A3: mm → px by the corners.
        let sx = f64::from(c.x - o.x) / 420.0;
        let sy = f64::from(o.y - c.y) / 297.0;
        [f64::from(o.x) + p[0] * sx, f64::from(o.y) - p[1] * sy]
    }

    fn ui(h: &crate::scenario::Harness) -> DrawUi {
        match h.app.dialog.as_ref().map(|d| &d.kind) {
            Some(Kind::Drawing(k)) => (**k).clone(),
            other => panic!("not the drawing editor: {other:?}"),
        }
    }

    #[test]
    fn the_editor_makes_a_drawing_and_shows_its_sheet() {
        let h = editor();
        assert_eq!(h.app.session.doc.drawings.len(), 1);
        let d = h.app.session.doc.drawings.first().cloned().unwrap();
        assert_eq!(d.views.len(), 4);
        assert!(active(&h.app) && crate::dialogs_sheet::flat_active(&h.app), "the sheet takes the viewport");
        assert!(crate::scenario::rect_of("drawing:paper").is_some_and(|r| r.width() > 100.0));
        for v in &d.views {
            assert!(crate::scenario::handle_at(&format!("drawing:view:{}", v.id)).is_some(), "view {}", v.id);
        }
        assert_eq!(ui(&h).drawing, Some(d.id));
    }

    #[test]
    fn a_view_is_selected_and_dragged_and_notes_and_sections_are_placed_by_clicks() {
        let mut h = editor();
        let d = h.app.session.doc.drawings.first().cloned().unwrap();
        let front = d.views.first().map(|v| v.id).unwrap();
        let at0 = d.views.first().map(|v| v.at).unwrap();
        let p = crate::scenario::handle_at(&format!("drawing:view:{front}")).unwrap();
        h.step(&json!({"click": [p.x, p.y]})).unwrap();
        h.frames(2);
        assert_eq!(ui(&h).selected, Some(front));
        // A drag 10 mm to the right moves it 10 mm.
        let a = [f64::from(p.x), f64::from(p.y)];
        let b = screen([at0[0] + 10.0, at0[1]]);
        let dx = b[0] - screen(at0)[0];
        h.step(&json!({"drag": [a, [a[0] + dx, a[1]]], "steps": 6})).unwrap();
        h.frames(3);
        let moved = h.app.session.doc.drawings.first().and_then(|d| d.view(front)).map(|v| v.at).unwrap();
        assert!((moved[0] - at0[0] - 10.0).abs() < 1.0 && (moved[1] - at0[1]).abs() < 1.0, "{at0:?} → {moved:?}");
        // A note, typed and placed.
        if let Some(Kind::Drawing(k)) = h.app.dialog.as_mut().map(|d| &mut d.kind) {
            k.tool = Tool::Note;
            k.note = "Sorjamentes".into();
        }
        let n = screen([40.0, 60.0]);
        h.step(&json!({"click": n})).unwrap();
        h.frames(2);
        let notes = h.app.session.doc.drawings.first().map(|d| d.notes.clone()).unwrap();
        assert!(notes.len() == 1 && notes[0].text == "Sorjamentes" && (notes[0].at[0] - 40.0).abs() < 1.0, "{notes:?}");
        // A section across the top view: two clicks.
        let top = d.views.get(1).cloned().unwrap();
        if let Some(Kind::Drawing(k)) = h.app.dialog.as_mut().map(|d| &mut d.kind) {
            k.tool = Tool::Section;
        }
        for x in [-60.0, 60.0] {
            h.step(&json!({"click": screen([top.at[0] + x, top.at[1]])})).unwrap();
            h.frames(2);
        }
        let d = h.app.session.doc.drawings.first().cloned().unwrap();
        assert_eq!(d.views.len(), 5, "{:?}", ui(&h).status);
        assert!(matches!(d.views.last().map(|v| &v.kind), Some(solidcraft_engine::doc::drawing::ViewKind::Section { .. })));
        assert_eq!(ui(&h).tool, Tool::Select);
    }

    #[test]
    fn the_ribbon_dimension_kinds_open_the_tool_with_their_kind() {
        let mut h = editor();
        for (id, kind, arc) in [("drawing.dimension.vertical", 1, 0), ("drawing.dimension.diameter", 3, 0), ("drawing.dimension.arcs", 0, 3)] {
            h.app.dialog = None;
            h.step(&json!({"start": id})).unwrap();
            h.frames(2);
            let k = ui(&h);
            assert_eq!((k.tool, k.dim_kind, k.arc), (Tool::Dimension, kind, arc), "{id}");
            assert_eq!(h.app.session.doc.drawings.len(), 1, "{id}: no new drawing");
        }
    }
}
