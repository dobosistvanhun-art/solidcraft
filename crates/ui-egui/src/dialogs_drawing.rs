//! The drawing editor: while it is open the viewport shows the drawing's sheet (paper white,
//! lines as they print) instead of the model, and the panel holds the sheet, the title block,
//! the views and the annotation tools. Everything it changes goes through `drawing.*` commands.
//!
//! A new drawing is an empty sheet: the views are put on it by hand, as on a drawing board.
//! - Base view: choose the orientation (and scale), the view follows the pointer, a click puts it.
//!   The tool then goes on to projected views of it.
//! - Projected view: move the pointer to a side of its parent (right, left, above, below, or off a
//!   corner for an iso view), the view follows lined up with its parent, a click puts it; again
//!   for more, right click or Esc to stop.
//! - Section: two clicks for the cutting line (snapping to the model's points, and square when
//!   nearly so), then the view follows the pointer in line with its parent, a click puts it.
//! - Detail: its centre, a point on its circle, then where the view goes.
//! - Dimension: the pointer shows what it would pick (a point, an edge, a circle) — nothing is
//!   picked in empty paper. An edge measures its length, a circle its diameter (an arc its
//!   radius), two points the distance between them; then the dimension follows the pointer
//!   (horizontal, vertical or aligned by where it is pulled) and a click puts it.
//! - Select: click a view or a dimension; drag it to move it (a projected view keeps in line with
//!   its parent, which takes its views along); Delete removes it.
//!
//! The camera is the shared flat navigation (pan and zoom, no orbit).

use egui::{Align2, Color32, FontId, Pos2, Rect, Stroke, pos2, vec2};
use serde_json::{Value, json};
use solidcraft_engine::doc::drawing::{SCALES, SHEET_SIZES, ViewKind};

use crate::SolveApp;
use crate::dialogs::{Dialog, Kind};
use crate::theme::Tokens;
use crate::ui_kit::{self, choice_row, text_row};

const ORIENTATIONS: [&str; 7] = ["front", "back", "top", "bottom", "left", "right", "iso"];
const ORIENTATION_LABELS: [&str; 7] = ["Front", "Back", "Top", "Bottom", "Left", "Right", "Iso"];
const DIM_KINDS: [&str; 7] = ["auto", "horizontal", "vertical", "aligned", "diameter", "radius", "angle"];
const DIM_LABELS: [&str; 7] = ["Automatic", "Horizontal", "Vertical", "Aligned", "Diameter", "Radius", "Angle"];
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
/// How near the pointer (screen points) a point or an edge must be to be picked.
const REACH_PX: f64 = 9.0;

/// What clicks on the sheet do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Select,
    Base,
    Projected,
    Section,
    Detail,
    Dimension,
    Note,
}

const TOOLS: [(Tool, &str); 7] = [
    (Tool::Select, "Select"),
    (Tool::Base, "Base view"),
    (Tool::Projected, "Projected view"),
    (Tool::Section, "Section"),
    (Tool::Detail, "Detail"),
    (Tool::Dimension, "Dimension"),
    (Tool::Note, "Note"),
];

/// Something under the pointer a tool may pick (sheet mm).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Pick {
    /// An edge's end, a circle's centre or a virtual sharp.
    Point([f64; 2]),
    /// A straight edge.
    Line([f64; 2], [f64; 2]),
    /// A circle (`true`) or an arc seen face on: centre, radius (sheet mm).
    Circle([f64; 2], f64, bool),
}

impl Pick {
    /// The point the engine is sent for it: the point, a circle's point on its edge.
    fn sent(self) -> Vec<[f64; 2]> {
        match self {
            Pick::Point(p) => vec![p],
            Pick::Line(a, b) => vec![a, b],
            Pick::Circle(c, r, _) => vec![[c[0] + r, c[1]]],
        }
    }
    /// Where it counts from in a preview: the point, a line's ends, a circle's centre.
    fn anchors(self) -> Vec<[f64; 2]> {
        match self {
            Pick::Point(p) => vec![p],
            Pick::Line(a, b) => vec![a, b],
            Pick::Circle(c, ..) => vec![c],
        }
    }
}

/// What a tool's next pick may be.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Want {
    Points,
    /// Points first, else straight edges and (`circles`) circles.
    PointsOrEdges {
        circles: bool,
    },
    Circles,
    Sharps,
}

#[derive(Clone, Debug)]
pub struct DrawUi {
    /// The drawing open (its id).
    pub drawing: Option<u64>,
    pub tool: Tool,
    /// Sheet points the tool has taken so far (section, detail).
    pub picks: Vec<[f64; 2]>,
    /// What the dimension tool has picked so far (view, what).
    pub dim_picks: Vec<(u64, Pick)>,
    /// The view the section or detail is taken from.
    pub pick_view: Option<u64>,
    pub selected: Option<u64>,
    /// The dimension selected (by its text), and its text and tolerance as edited.
    pub dim: Option<(u64, String, String)>,
    /// The base view to put: its orientation (index), scale (0: the sheet's) and hidden edges.
    pub orientation: usize,
    pub base_scale: usize,
    pub base_hidden: bool,
    /// The parent of the projected views being put.
    pub parent: Option<u64>,
    pub dim_kind: usize,
    /// Index into `ARCS`.
    pub arc: usize,
    pub tolerance: String,
    pub note: String,
    /// The title block's fields as edited (written when a field is left).
    pub title: Vec<String>,
    /// The drawn sheet: (revision, drawing) it was drawn for and the result.
    pub drawn: Option<((u64, u64), std::sync::Arc<Value>)>,
    /// Frames of views not yet put (by what they would be): about their centre, sheet mm.
    pub previews: Vec<(String, Option<([f64; 2], [f64; 2])>)>,
    /// A view being dragged: its id, where the drag started (sheet mm) and its place then.
    pub drag: Option<(u64, [f64; 2], [f64; 2])>,
    /// A dimension being dragged: its id and where the drag started.
    pub dim_drag: Option<(u64, [f64; 2])>,
    /// What is under the pointer for the tool (view, what), its kind, and how many there are
    /// to choose from there (Space goes to the next).
    pub hover: Option<(u64, Pick)>,
    pub hover_kind: &'static str,
    pub choices: usize,
    /// Which of the candidates under the pointer is taken, and the place it was counted for.
    pub cycle: usize,
    pub cycle_at: Option<[f64; 2]>,
    pub status: Option<String>,
}

impl DrawUi {
    fn new(tool: Tool) -> DrawUi {
        DrawUi {
            drawing: None,
            tool,
            picks: Vec::new(),
            dim_picks: Vec::new(),
            pick_view: None,
            selected: None,
            dim: None,
            orientation: 0,
            base_scale: 0,
            base_hidden: false,
            parent: None,
            dim_kind: 0,
            arc: 0,
            tolerance: String::new(),
            note: String::new(),
            title: Vec::new(),
            drawn: None,
            previews: Vec::new(),
            drag: None,
            dim_drag: None,
            hover: None,
            hover_kind: "",
            choices: 0,
            cycle: 0,
            cycle_at: None,
            status: None,
        }
    }

    /// Switch tools, dropping what the last one had picked.
    fn set_tool(&mut self, tool: Tool) {
        self.tool = tool;
        self.picks.clear();
        self.dim_picks.clear();
        self.pick_view = None;
        self.hover = None;
        if tool == Tool::Projected {
            self.parent = self.parent.or(self.selected);
        }
    }

    /// The dimension tool has all it needs: the next click puts it.
    fn dim_ready(&self) -> bool {
        let n = self.dim_picks.len();
        match self.dim_kind {
            4 | 5 => n >= 1,
            6 => n >= 3,
            _ => {
                let first = self.dim_picks.first().map(|(_, p)| *p);
                matches!(first, Some(Pick::Line(..))) || (self.dim_kind == 0 && self.arc == 0 && matches!(first, Some(Pick::Circle(..)))) || n >= 2
            }
        }
    }

    fn want(&self) -> Want {
        match self.dim_kind {
            4 | 5 => Want::Circles,
            6 => Want::Points,
            _ => match self.arc {
                1..=3 => Want::Circles,
                4 => Want::Sharps,
                _ if self.dim_picks.is_empty() => Want::PointsOrEdges { circles: self.dim_kind == 0 },
                _ => Want::Points,
            },
        }
    }
}

/// The editor for a toolbar command of the DRAWING tab.
pub fn start(_app: &SolveApp, id: &str) -> Option<(Kind, Vec<crate::selection::SelInput>)> {
    let tool = match id {
        "drawing.create" | "drawing.edit" | "drawing.view.flat" | "drawing.export_pdf" | "drawing.export_dxf" => Tool::Select,
        "drawing.view.base" => Tool::Base,
        "drawing.view.projected" => Tool::Projected,
        "drawing.view.section" => Tool::Section,
        "drawing.view.detail" => Tool::Detail,
        "drawing.note" => Tool::Note,
        _ if id.starts_with("drawing.dimension") => Tool::Dimension,
        _ => return None,
    };
    let mut ui = DrawUi::new(tool);
    // The ribbon's dimension kinds: the kind (and for Arc to Arc, the far sides of two arcs).
    let (kind, arc) = match id {
        "drawing.dimension.horizontal" => (1, 0),
        "drawing.dimension.vertical" => (2, 0),
        "drawing.dimension.aligned" => (3, 0),
        "drawing.dimension.diameter" => (4, 0),
        "drawing.dimension.radius" => (5, 0),
        "drawing.dimension.angle" => (6, 0),
        "drawing.dimension.arcs" => (1, 3),
        _ => (0, 0),
    };
    ui.dim_kind = kind;
    ui.arc = arc;
    ui.status = Some(match id {
        "drawing.create" => "new".into(),
        // The ribbon's flat pattern view: added once the drawing is known.
        "drawing.view.flat" => "flat".into(),
        _ => String::new(),
    });
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

/// A new, empty drawing (its views are put by hand); the base view tool takes over.
fn new_drawing(app: &mut SolveApp, k: &mut DrawUi) -> Option<u64> {
    let id = run(app, k, "drawing.create", json!({"views": "none"})).and_then(|v| v["drawing"].as_u64());
    if id.is_some() && matches!(k.tool, Tool::Select | Tool::Projected | Tool::Section | Tool::Detail | Tool::Dimension) {
        k.set_tool(Tool::Base);
    }
    id
}

/// The panel's rows.
pub fn rows(app: &mut SolveApp, ui: &mut egui::Ui, k: &mut DrawUi) -> bool {
    // The drawing: a new one when the editor was opened for one (or there is none yet).
    let wants_new = k.status.as_deref() == Some("new");
    let wants_flat = k.status.as_deref() == Some("flat");
    if wants_new || wants_flat {
        k.status = None;
    }
    if k.drawing.is_none_or(|id| !app.session.doc.drawings.iter().any(|d| d.id == id)) {
        k.drawing = if wants_new || app.session.doc.drawings.is_empty() {
            if app.session.world_state().bodies.is_empty() {
                k.status = Some(crate::i18n::tr_label("Model a part first: a drawing shows its bodies").to_string());
                None
            } else {
                new_drawing(app, k)
            }
        } else {
            app.session.doc.drawings.last().map(|d| d.id)
        };
        k.title.clear();
        // An empty sheet starts with putting its first view.
        if k.tool == Tool::Select && drawing(app, k).is_some_and(|d| d.views.is_empty()) && !wants_flat {
            k.set_tool(Tool::Base);
        }
    }
    let names: Vec<String> =
        app.session.doc.drawings.iter().map(|d| d.name.clone()).chain(std::iter::once(crate::i18n::tr_label("New drawing…").to_string())).collect();
    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
    let mut pick = app.session.doc.drawings.iter().position(|d| Some(d.id) == k.drawing).unwrap_or(0);
    let before = pick;
    choice_row(ui, "Drawing", "drw_pick", &mut pick, &refs);
    if pick != before {
        k.drawing = match app.session.doc.drawings.get(pick) {
            Some(d) => Some(d.id),
            None => new_drawing(app, k),
        };
        k.title.clear();
        k.selected = None;
        k.parent = None;
    }
    let Some(d) = drawing(app, k).cloned() else {
        if let Some(s) = &k.status {
            ui_kit::text(ui, s, ui_kit::Text::Report("drawing_status", true));
        }
        return false;
    };
    let id = d.id;
    if wants_flat && let Some(v) = run(app, k, "drawing.view.flat", json!({"drawing": id})) {
        k.selected = v["view"].as_u64();
        k.set_tool(Tool::Select);
    }

    // ---- what clicks on the sheet do ----
    ui_kit::section(ui, "drw_tools", "On the sheet", true, |ui| {
        let labels: Vec<&str> = TOOLS.iter().map(|(_, l)| *l).collect();
        let mut at = TOOLS.iter().position(|(t, _)| *t == k.tool).unwrap_or(0);
        let before = at;
        ui_kit::row(ui, "Tool", |ui| ui_kit::dropdown(ui, "drw_tool", &mut at, &labels));
        // For scenarios and the control channel: `dialog:tool:Section` picks that tool.
        if let Some(at) = crate::scenario::handle_at("dialog:Tool") {
            for (_, label) in TOOLS {
                crate::scenario::publish_handle(&format!("dialog:tool:{label}"), at);
            }
        }
        if at != before
            && let Some((tool, _)) = TOOLS.get(at)
        {
            k.set_tool(*tool);
        }
        match k.tool {
            Tool::Base => {
                choice_row(ui, "Orientation", "drw_orient", &mut k.orientation, &ORIENTATION_LABELS);
                let own: Vec<&str> = std::iter::once("Sheet's").chain(SCALES.iter().copied()).collect();
                choice_row(ui, "View scale", "drw_bscale", &mut k.base_scale, &own);
                ui_kit::checkbox(ui, "Hidden edges", &mut k.base_hidden);
            }
            Tool::Projected => {
                let parent = k.parent.and_then(|p| d.view(p));
                let name = parent.map(|v| view_name(&d, v)).unwrap_or_else(|| crate::i18n::tr_label("— click a view —").to_string());
                ui_kit::row(ui, "From view", |ui| ui.label(name));
            }
            Tool::Dimension => {
                let b = k.dim_kind;
                choice_row(ui, "Kind", "drw_dim", &mut k.dim_kind, &DIM_LABELS);
                if k.dim_kind <= 3 {
                    // To circles and arcs: their centres, or their near or far sides.
                    choice_row(ui, "Points on", "drw_arc", &mut k.arc, &ARC_LABELS);
                } else {
                    k.arc = 0;
                }
                if k.dim_kind != b {
                    k.dim_picks.clear();
                }
                text_row(ui, "Tolerance", &mut k.tolerance, "±0.1, +0.2/-0.1, H7");
            }
            Tool::Note => {
                text_row(ui, "Text", &mut k.note, "");
            }
            _ => {}
        }
        ui_kit::text(ui, crate::i18n::tr_label(hint(k)), ui_kit::Text::Hint("drw_tool_hint"));
        let keys = match k.tool {
            Tool::Dimension | Tool::Section => "Space: the next thing under the pointer · Backspace: back one pick · Esc: cancel",
            Tool::Projected => "Shift: put it anywhere (not lined up) · Esc: done",
            Tool::Select => "Shift while dragging: off its line (free) · Delete: remove",
            _ => "Esc: cancel",
        };
        ui_kit::text(ui, crate::i18n::tr_label(keys), ui_kit::Text::Hint("drw_tool_keys"));
    });

    // ---- the selected view ----
    let selected = k.selected.and_then(|v| d.view(v)).cloned();
    ui_kit::section(ui, "drw_views", "Views", true, |ui| {
        ui.horizontal_wrapped(|ui| {
            for (tool, label) in
                [(Tool::Base, "Base view"), (Tool::Projected, "Projected view"), (Tool::Section, "Section"), (Tool::Detail, "Detail")]
            {
                if ui_kit::button(ui, crate::i18n::tr_label(label), ui_kit::Button::Small).clicked() {
                    k.set_tool(tool);
                }
            }
            // A sheet metal part's flat pattern, as cut, with its bend lines.
            if !app.session.model.state().sheets.is_empty()
                && ui_kit::button(ui, crate::i18n::tr_label("Flat Pattern"), ui_kit::Button::Small).clicked()
                && let Some(v) = run(app, k, "drawing.view.flat", json!({"drawing": id}))
            {
                k.selected = v["view"].as_u64();
            }
        });
        let Some(v) = &selected else {
            ui_kit::text(ui, crate::i18n::tr_label("Click a view to select it, drag it to move it"), ui_kit::Text::Hint("drw_views"));
            return;
        };
        ui_kit::row(ui, "Selected", |ui| ui.label(view_name(&d, v)));
        if let ViewKind::Base { orientation } = &v.kind {
            let mut o = ORIENTATIONS.iter().position(|x| x == orientation).unwrap_or(0);
            let b = o;
            choice_row(ui, "Orientation", "drw_vorient", &mut o, &ORIENTATION_LABELS);
            if o != b {
                run(app, k, "drawing.view.edit", json!({"drawing": id, "view": v.id, "orientation": ORIENTATIONS.get(o)}));
            }
        }
        let mut hidden = v.hidden;
        if ui_kit::checkbox(ui, "Hidden edges", &mut hidden).changed() {
            run(app, k, "drawing.view.edit", json!({"drawing": id, "view": v.id, "hidden": hidden}));
        }
        if matches!(&v.kind, ViewKind::Projected { side, .. } if side != "iso") || matches!(v.kind, ViewKind::Section { .. }) {
            let mut lined = !v.free;
            if ui_kit::checkbox(ui, "Lined up with its parent", &mut lined).changed() {
                // Lined up again: back on its parent's line where it is.
                run(app, k, "drawing.view.edit", json!({"drawing": id, "view": v.id, "free": !lined, "at": v.at}));
            }
        }
        if let ViewKind::Section { flip, .. } = &v.kind {
            let mut f = *flip;
            if ui_kit::checkbox(ui, "Look the other way", &mut f).changed() {
                run(app, k, "drawing.view.edit", json!({"drawing": id, "view": v.id, "flip": f}));
            }
        }
        let own: Vec<&str> = std::iter::once("Sheet's").chain(SCALES.iter().copied()).collect();
        let mut sc = v.scale.as_deref().and_then(|s| SCALES.iter().position(|x| *x == s)).map_or(0, |i| i + 1);
        let b = sc;
        choice_row(ui, "View scale", "drw_vscale", &mut sc, &own);
        if sc != b {
            let text = sc.checked_sub(1).and_then(|i| SCALES.get(i)).copied().unwrap_or("");
            run(app, k, "drawing.view.edit", json!({"drawing": id, "view": v.id, "scale": text}));
        }
        ui.horizontal(|ui| {
            if !matches!(v.kind, ViewKind::Flat { .. })
                && ui_kit::button(ui, crate::i18n::tr_label("Project from it"), ui_kit::Button::Small).clicked()
            {
                k.parent = Some(v.id);
                k.set_tool(Tool::Projected);
            }
            if ui_kit::button(ui, crate::i18n::tr_label("Delete view"), ui_kit::Button::Small).clicked() {
                run(app, k, "drawing.view.delete", json!({"drawing": id, "view": v.id}));
                k.selected = None;
            }
        });
    });

    // ---- the selected dimension ----
    if let Some((dim_id, text, tol)) = k.dim.clone()
        && let Some(x) = d.dims.iter().find(|x| x.id == dim_id).cloned()
    {
        let (mut text, mut tol) = (text, tol);
        let mut edit: Option<Value> = None;
        let mut gone = false;
        ui_kit::section(ui, "drw_dimension", "Dimension", true, |ui| {
            let (enter, _) = text_row(ui, "Text", &mut text, crate::i18n::tr_label("<> is the value"));
            let idle = !ui.ctx().memory(|m| m.focused().is_some());
            if (enter || idle) && text != x.text {
                edit = Some(json!({"text": text.clone()}));
            }
            let (enter, _) = text_row(ui, "Tolerance", &mut tol, "±0.1, +0.2/-0.1, H7");
            if (enter || idle) && tol != x.tolerance {
                edit = Some(json!({"tolerance": tol.clone()}));
            }
            use solidcraft_engine::doc::drawing::DimKind;
            if matches!(x.kind, DimKind::Horizontal | DimKind::Vertical | DimKind::Aligned) && !x.arc.is_empty() && x.arc != "sharp" {
                // Between the arcs' centres, near or far sides (a virtual sharp stays one).
                let mut side = ["center", "min", "max"].iter().position(|a| *a == x.arc.as_str()).unwrap_or(0);
                let b = side;
                choice_row(ui, "Points on", "drw_dim_arc", &mut side, &ARC_LABELS[1..4]);
                if side != b {
                    let arc = ["center", "min", "max"].get(side).copied().unwrap_or("center");
                    edit = Some(json!({ "arc": arc }));
                }
            }
            ui_kit::text(ui, crate::i18n::tr_label("Drag its text to move it"), ui_kit::Text::Hint("drw_dim_hint"));
            if ui_kit::button(ui, crate::i18n::tr_label("Delete dimension"), ui_kit::Button::Normal).clicked() {
                gone = true;
            }
        });
        if gone {
            run(app, k, "drawing.dimension.delete", json!({"drawing": id, "dimension": dim_id}));
            k.dim = None;
        } else {
            k.dim = Some((dim_id, text, tol));
            if let Some(Value::Object(mut p)) = edit {
                p.insert("drawing".into(), json!(id));
                p.insert("dimension".into(), json!(dim_id));
                run(app, k, "drawing.dimension.edit", Value::Object(p));
            }
        }
    }

    // ---- sheet ----
    ui_kit::section(ui, "drw_sheet", "Sheet", false, |ui| {
        let sizes: Vec<&str> = SHEET_SIZES.iter().map(|(n, ..)| *n).collect();
        let mut size = sizes.iter().position(|s| s.eq_ignore_ascii_case(&d.sheet.size)).unwrap_or(1);
        let b = size;
        choice_row(ui, "Size", "drw_size", &mut size, &sizes);
        if size != b {
            run(app, k, "drawing.edit", json!({"drawing": id, "size": sizes.get(size)}));
        }
        let mut landscape = d.sheet.landscape;
        if ui_kit::checkbox(ui, "Landscape", &mut landscape).changed() {
            run(app, k, "drawing.edit", json!({"drawing": id, "landscape": landscape}));
        }
        let mut scale = SCALES.iter().position(|s| *s == d.scale).unwrap_or(3);
        let b = scale;
        choice_row(ui, "Scale", "drw_scale", &mut scale, SCALES);
        if scale != b {
            run(app, k, "drawing.edit", json!({"drawing": id, "scale": SCALES.get(scale)}));
        }
        // The dimensions' text (and arrowheads in proportion).
        const HEIGHTS: [&str; 5] = ["2.5", "3.5", "5", "7", "10"];
        let now = d.dim_text();
        let mut hi = HEIGHTS.iter().position(|x| x.parse::<f64>().is_ok_and(|v| (v - now).abs() < 1e-6)).unwrap_or(1);
        let b = hi;
        choice_row(ui, "Dimension text (mm)", "drw_text_h", &mut hi, &HEIGHTS);
        if hi != b {
            let h: f64 = HEIGHTS.get(hi).and_then(|x| x.parse().ok()).unwrap_or(3.5);
            run(app, k, "drawing.edit", json!({"drawing": id, "text_height": h}));
        }
        let projections = ["First angle (ISO E)", "Third angle (ISO A)"];
        let mut proj = usize::from(d.projection == solidcraft_engine::doc::drawing::Projection::Third);
        let b = proj;
        choice_row(ui, "Projection", "drw_proj", &mut proj, &projections);
        if proj != b {
            run(app, k, "drawing.edit", json!({"drawing": id, "projection": if proj == 1 { "third" } else { "first" }}));
        }
    });

    // ---- title block ----
    ui_kit::section(ui, "drw_title", "Title Block", false, |ui| {
        if k.title.len() != TITLE_FIELDS.len() {
            let mut tb = d.title.clone();
            k.title = tb.fields_mut().iter().map(|(_, v)| v.to_string()).collect();
        }
        let mut tb = d.title.clone();
        let stored: Vec<String> = tb.fields_mut().iter().map(|(_, v)| v.to_string()).collect();
        let mut changed: Option<(&str, String)> = None;
        for (((key, label), value), stored) in TITLE_FIELDS.iter().zip(k.title.iter_mut()).zip(&stored) {
            let (enter, _) = text_row(ui, label, value, if *key == "material" { crate::i18n::tr_label("from the part") } else { "" });
            if let Some(at) = crate::scenario::handle_at(&format!("dialog:{label}")) {
                crate::scenario::publish_handle(&format!("dialog:title:{key}"), at);
            }
            // Saved on Enter, or once the field is left.
            let idle = !ui.ctx().memory(|m| m.focused().is_some());
            if (enter || idle) && value != stored {
                changed = Some((key, value.clone()));
            }
        }
        if let Some((key, value)) = changed {
            let mut title = serde_json::Map::new();
            title.insert(key.into(), json!(value));
            run(app, k, "drawing.edit", json!({"drawing": id, "title": title}));
        }
    });

    // ---- export ----
    ui_kit::section(ui, "drw_export", "Export", true, |ui| {
        ui.horizontal(|ui| {
            for (ext, cmd) in [("pdf", "drawing.export_pdf"), ("dxf", "drawing.export_dxf")] {
                if ui_kit::button(ui, format!("{}…", ext.to_ascii_uppercase()), ui_kit::Button::Normal).clicked() {
                    app.pick_save_command(&format!("{}.{ext}", d.name), &[ext], cmd, json!({"drawing": id}));
                }
            }
        });
    });
    if let Some(s) = &k.status {
        ui_kit::text(ui, s, ui_kit::Text::Report("drawing_status", true));
    }
    false
}

/// A view's name for the panel: its kind and orientation or label.
fn view_name(d: &solidcraft_engine::doc::drawing::Drawing, v: &solidcraft_engine::doc::drawing::DrawView) -> String {
    let tr = crate::i18n::tr_label;
    match &v.kind {
        ViewKind::Base { orientation } => {
            let o = ORIENTATIONS.iter().position(|x| x == orientation).and_then(|i| ORIENTATION_LABELS.get(i)).copied().unwrap_or("Front");
            format!("{} ({})", tr("Base view"), tr(o))
        }
        ViewKind::Projected { parent, side } => {
            let from = d.view(*parent).map(|p| match &p.kind {
                ViewKind::Base { orientation } => orientation.clone(),
                _ => String::new(),
            });
            format!("{} ({side}{})", tr("Projected view"), from.filter(|f| !f.is_empty()).map(|f| format!(", {f}")).unwrap_or_default())
        }
        ViewKind::Section { .. } => format!("{} {1}–{1}", tr("Section"), v.label),
        ViewKind::Detail { .. } => format!("{} {}", tr("Detail"), v.label),
        ViewKind::Flat { .. } => tr("Flat Pattern").to_string(),
    }
}

/// What the tool wants next.
fn hint(k: &DrawUi) -> &'static str {
    match (k.tool, k.picks.len()) {
        (Tool::Select, _) => "Click a view or a dimension to select it, drag it to move it; Delete removes it",
        (Tool::Base, _) => "Click where the view goes",
        (Tool::Projected, _) if k.parent.is_none() => "Click the view to project from",
        (Tool::Projected, _) => "Move to a side of the view (a corner: iso) and click; right click or Esc to stop",
        (Tool::Section, 0) => "Click the cutting line's start in a view",
        (Tool::Section, 1) => "Click the cutting line's end",
        (Tool::Section, _) => "Click where the section view goes",
        (Tool::Detail, 0) => "Click the centre of the detail in a view",
        (Tool::Detail, 1) => "Click a point on the detail's circle",
        (Tool::Detail, _) => "Click where the detail view goes",
        (Tool::Dimension, _) if k.dim_ready() => "Click where the dimension goes; right click or Esc to start again",
        (Tool::Dimension, _) => match k.want() {
            Want::Circles => "Click a circle or an arc",
            Want::Sharps => "Click where the rounded corner would be without its round",
            Want::PointsOrEdges { .. } => "Click an edge (its length), a circle (its diameter) or a point",
            Want::Points if k.dim_kind == 6 && k.dim_picks.is_empty() => "Click the angle's corner",
            Want::Points => "Click the next point",
        },
        (Tool::Note, _) => "Type the text, then click where it goes",
    }
}

/// The drawn sheet (drawn again when the design or the drawing changed).
fn drawn(app: &mut SolveApp, k: &mut DrawUi) -> Option<std::sync::Arc<Value>> {
    let id = k.drawing?;
    let key = (app.session.revision, id);
    if k.drawn.as_ref().map(|(kk, _)| *kk) != Some(key) {
        let v = app.session.execute("drawing.render", &json!({"drawing": id})).ok()?;
        k.drawn = Some(((app.session.revision, id), std::sync::Arc::new(v)));
        k.previews.clear();
    }
    // Shared, not copied: the sheet is painted every frame.
    k.drawn.as_ref().map(|(_, v)| v.clone())
}

/// The frame a view not yet put would have (sheet mm about its centre), kept by what it is.
fn preview(app: &mut SolveApp, k: &mut DrawUi, p: Value) -> Option<([f64; 2], [f64; 2])> {
    let key = p.to_string();
    if let Some((_, f)) = k.previews.iter().find(|(kk, _)| *kk == key) {
        return *f;
    }
    let f = app.session.execute("drawing.view.preview", &p).ok().and_then(|v| Some((read2(&v["min"])?, read2(&v["max"])?)));
    if k.previews.len() > 32 {
        k.previews.remove(0);
    }
    k.previews.push((key, f));
    f
}

fn read2(v: &Value) -> Option<[f64; 2]> {
    Some([v.get(0)?.as_f64()?, v.get(1)?.as_f64()?])
}

fn dist(a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

fn seg_dist(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let l2 = dx * dx + dy * dy;
    let t = if l2 > 0.0 { (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / l2).clamp(0.0, 1.0) } else { 0.0 };
    dist(p, [a[0] + dx * t, a[1] + dy * t])
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

/// A view's frame and scale as drawn.
fn view_frame(v: &Value, id: u64) -> Option<([f64; 2], [f64; 2], f64)> {
    let x = v["views"].as_array()?.iter().find(|x| x["id"].as_u64() == Some(id))?;
    Some((read2(&x["min"])?, read2(&x["max"])?, x["scale"].as_f64().unwrap_or(1.0)))
}

/// Something the pointer may pick: in which view, what, what kind of reference (`end`, `mid`,
/// `center`, `quad`, `cross`, `sharp`, `line`, `circle`, `arc`) and how far from the pointer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Candidate {
    pub view: u64,
    pub pick: Pick,
    pub kind: &'static str,
    pub dist: f64,
}

/// A reference kind as `'static` text (the render's are a known few).
fn kind_of(k: &str) -> &'static str {
    match k {
        "mid" => "mid",
        "center" => "center",
        "quad" => "quad",
        "cross" => "cross",
        "sharp" => "sharp",
        _ => "end",
    }
}

/// Everything under the pointer (within `reach` sheet mm) that the tool may pick, the points
/// first and then the edges, each nearest first; only in `only` when given.
fn candidates(v: &Value, at: [f64; 2], reach: f64, want: Want, only: Option<u64>) -> Vec<Candidate> {
    let mut points: Vec<Candidate> = Vec::new();
    let mut edges: Vec<Candidate> = Vec::new();
    for x in v["views"].as_array().into_iter().flatten() {
        let Some(id) = x["id"].as_u64() else { continue };
        if only.is_some_and(|o| o != id) {
            continue;
        }
        if !matches!(want, Want::Circles) {
            for p in x["points"].as_array().into_iter().flatten() {
                let (Some(q), kind) = (read2(&p["at"]), kind_of(p["kind"].as_str().unwrap_or("end"))) else { continue };
                if (want == Want::Sharps) != (kind == "sharp") {
                    continue;
                }
                let dd = dist(q, at);
                // One candidate per place (a corner of several edges is one point).
                if dd <= reach && !points.iter().any(|c| c.pick == Pick::Point(q)) {
                    points.push(Candidate { view: id, pick: Pick::Point(q), kind, dist: dd });
                }
            }
        }
        let (lines, circles) = match want {
            Want::PointsOrEdges { circles } => (true, circles),
            Want::Circles => (false, true),
            _ => (false, false),
        };
        for e in x["edges"].as_array().into_iter().flatten() {
            let Some(a) = read2(&e["a"]) else { continue };
            let found = match e["kind"].as_str() {
                Some("line") if lines => read2(&e["b"]).map(|b| (seg_dist(at, a, b), Pick::Line(a, b), "line")),
                Some("circle") if circles => e["r"].as_f64().map(|r| {
                    let full = e["full"].as_bool().unwrap_or(false);
                    ((dist(at, a) - r).abs(), Pick::Circle(a, r, full), if full { "circle" } else { "arc" })
                }),
                _ => None,
            };
            if let Some((dd, pick, kind)) = found
                && dd <= reach
            {
                edges.push(Candidate { view: id, pick, kind, dist: dd });
            }
        }
    }
    points.sort_by(|a, b| a.dist.total_cmp(&b.dist));
    edges.sort_by(|a, b| a.dist.total_cmp(&b.dist));
    points.extend(edges);
    points.truncate(24);
    points
}

/// What a reference kind is called on screen.
fn kind_label(kind: &str) -> &'static str {
    match kind {
        "end" => "Endpoint",
        "mid" => "Midpoint",
        "center" => "Centre",
        "quad" => "Quadrant point",
        "cross" => "Intersection",
        "sharp" => "Virtual sharp",
        "line" => "Edge",
        "circle" => "Circle",
        _ => "Arc",
    }
}

/// A measured value as the drawing shows it (up to two decimals, a decimal comma).
fn number(v: f64) -> String {
    let t = format!("{:.2}", (v * 100.0).round() / 100.0);
    let t = t.trim_end_matches('0').trim_end_matches('.');
    if t == "-0" { "0".into() } else { t.replace('.', ",") }
}

/// A linear dimension's kind from where it is put (as the engine's `auto`).
fn auto_kind(p1: [f64; 2], p2: [f64; 2], q: [f64; 2]) -> usize {
    let (dx, dy) = ((p2[0] - p1[0]).abs(), (p2[1] - p1[1]).abs());
    if dy < 1e-6 {
        return 1;
    }
    if dx < 1e-6 {
        return 2;
    }
    let within = |v: f64, a: f64, b: f64| v >= a.min(b) && v <= a.max(b);
    match (within(q[0], p1[0], p2[0]), within(q[1], p1[1], p2[1])) {
        (true, false) => 1,
        (false, true) => 2,
        _ => 3,
    }
}

/// Which side of `parent`'s frame the pointer is on: a projected view's `side`.
fn side_of(lo: [f64; 2], hi: [f64; 2], at: [f64; 2], corner_iso: bool) -> &'static str {
    let c = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0];
    let (hw, hh) = (((hi[0] - lo[0]) / 2.0).max(1.0), ((hi[1] - lo[1]) / 2.0).max(1.0));
    let (rx, ry) = ((at[0] - c[0]) / hw, (at[1] - c[1]) / hh);
    if corner_iso && rx.abs() > 1.0 && ry.abs() > 1.0 {
        "iso"
    } else if rx.abs() >= ry.abs() {
        if rx >= 0.0 { "right" } else { "left" }
    } else if ry >= 0.0 {
        "above"
    } else {
        "below"
    }
}

/// A section's cutting line end: square to the first point when nearly so.
fn squared(first: [f64; 2], p: [f64; 2]) -> [f64; 2] {
    let (dx, dy) = (p[0] - first[0], p[1] - first[1]);
    let a = dy.atan2(dx).to_degrees().rem_euclid(90.0);
    if !(4.0..=86.0).contains(&a) { if dx.abs() >= dy.abs() { [p[0], first[1]] } else { [first[0], p[1]] } } else { p }
}

/// Where a view not yet put goes for the pointer at `at`: lined up with `parent`'s centre when
/// `line` (`h` across: same height; `v` up and down: same x).
fn lined(at: [f64; 2], parent_at: Option<[f64; 2]>, line: char) -> [f64; 2] {
    match (parent_at, line) {
        (Some(p), 'h') => [at[0], p[1]],
        (Some(p), 'v') => [p[0], at[1]],
        _ => at,
    }
}

/// Esc in the drawing editor: the drag, the picks so far, then the tool step back (to Select).
/// False when there is nothing to step back from (Esc then closes the editor).
pub fn escape(app: &mut SolveApp) -> bool {
    let Some(Kind::Drawing(k)) = app.dialog.as_mut().map(|d| &mut d.kind) else { return false };
    step_back(k)
}

fn step_back(k: &mut DrawUi) -> bool {
    if k.drag.take().is_some() | k.dim_drag.take().is_some() {
        return true;
    }
    if !k.picks.is_empty() || !k.dim_picks.is_empty() {
        k.picks.clear();
        k.dim_picks.clear();
        k.pick_view = None;
        return true;
    }
    if k.tool != Tool::Select {
        k.set_tool(Tool::Select);
        return true;
    }
    false
}

/// The frames of the views as drawn: id, lower left, upper right (sheet mm).
fn frames(v: &Value) -> Vec<(u64, [f64; 2], [f64; 2])> {
    v["views"].as_array().into_iter().flatten().filter_map(|x| Some((x["id"].as_u64()?, read2(&x["min"])?, read2(&x["max"])?))).collect()
}

/// What is wrong with a view's frame `lo`–`hi` where it would go: over another view (not one of
/// `except`), over the title block, or out of the drawing frame.
fn clash(v: &Value, lo: [f64; 2], hi: [f64; 2], except: &[u64]) -> Option<&'static str> {
    let (w, h) = (v["width"].as_f64().unwrap_or(420.0), v["height"].as_f64().unwrap_or(297.0));
    if lo[0] < 20.0 || lo[1] < 10.0 || hi[0] > w - 10.0 || hi[1] > h - 10.0 {
        return Some("Outside the drawing frame");
    }
    let overlaps = |a0: [f64; 2], a1: [f64; 2]| lo[0] < a1[0] && hi[0] > a0[0] && lo[1] < a1[1] && hi[1] > a0[1];
    if frames(v).into_iter().any(|(id, a0, a1)| !except.contains(&id) && overlaps(a0, a1)) {
        return Some("Overlaps another view");
    }
    let tb = [(w - 10.0 - 180.0f64.min(w - 30.0)), 10.0];
    if overlaps(tb, [w - 10.0, 46.0]) {
        return Some("Over the title block");
    }
    None
}

/// A point lined up with the centres of other views when it is near one of their lines (across
/// or up and down): the point, and the centres it lines up with (for guide lines).
fn aligned_to(at: [f64; 2], centres: &[[f64; 2]], reach: f64) -> ([f64; 2], Option<[f64; 2]>, Option<[f64; 2]>) {
    let near = |axis: usize| {
        centres
            .iter()
            .filter(|c| (c[axis] - at[axis]).abs() <= reach)
            .min_by(|a, b| (a[axis] - at[axis]).abs().total_cmp(&(b[axis] - at[axis]).abs()))
            .copied()
    };
    let (by_x, by_y) = (near(0), near(1));
    let p = [by_x.map_or(at[0], |c| c[0]), by_y.map_or(at[1], |c| c[1])];
    (p, by_x, by_y)
}

/// Where a dragged view goes for the pointer at `now`: a projected view lined up with its parent
/// slides along its line (with Shift held it goes anywhere, and is then free); others line up
/// with the centres of the views not moving with them. With the guides' centres.
fn drag_to(
    d: &solidcraft_engine::doc::drawing::Drawing,
    id: u64,
    start: [f64; 2],
    at0: [f64; 2],
    now: [f64; 2],
    shift: bool,
    carried: &[u64],
    reach: f64,
) -> ([f64; 2], Option<[f64; 2]>, Option<[f64; 2]>) {
    let at = [at0[0] + now[0] - start[0], at0[1] + now[1] - start[1]];
    let Some(v) = d.view(id) else { return (at, None, None) };
    if let ViewKind::Projected { parent, side } = &v.kind
        && !v.free
        && !shift
        && side != "iso"
        && let Some(pa) = d.view(*parent).map(|p| p.at)
    {
        return (lined(at, Some(pa), if side == "left" || side == "right" { 'h' } else { 'v' }), None, None);
    }
    let centres: Vec<[f64; 2]> = d.views.iter().filter(|w| !carried.contains(&w.id)).map(|w| w.at).collect();
    aligned_to(at, &centres, reach)
}

/// Paint the sheet over the viewport and handle clicks and drags on it. True when the dialog is
/// the drawing editor.
pub fn overlay(app: &mut SolveApp, painter: &egui::Painter, rect: Rect, d: &mut Dialog) -> bool {
    let Kind::Drawing(k) = &mut d.kind else { return false };
    let t = Tokens::get();
    painter.rect_filled(rect, 0.0, t.panel);
    let Some(v) = drawn(app, k) else {
        painter.text(rect.center(), Align2::CENTER_CENTER, crate::i18n::tr_label("No drawing"), FontId::proportional(t.font_body), t.text_dim);
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
    let reach = REACH_PX / scale.max(1e-6);
    // The paper: white, whatever the theme; ink black.
    let paper = Rect::from_two_pos(to([0.0, 0.0]), to([w, h]));
    painter.rect_filled(paper.translate(vec2(3.0, 3.0)), 0.0, Color32::from_black_alpha(60));
    painter.rect_filled(paper, 0.0, Color32::WHITE);
    let ink = Color32::from_gray(20);
    let px = |mm: f64| ((mm * scale) as f32).max(0.6);
    let ctx = painter.ctx().clone();
    let pointer = ctx.input(|i| i.pointer.latest_pos());
    let over = pointer.is_some_and(|p| rect.contains(p) && ctx.layer_id_at(p).is_none_or(|l| l.order == egui::Order::Background));
    let cursor = pointer.filter(|_| over).map(from);
    let shift_held = ctx.input(|i| i.modifiers.shift);
    // The views a dragged view takes along (its lined-up projected and section views).
    let carried: Vec<u64> = match k.drag {
        Some((id, ..)) => drawing(app, k)
            .map(|dd| {
                dd.descendants(id)
                    .into_iter()
                    .filter(|x| {
                        *x == id || dd.view(*x).is_some_and(|w| !w.free && matches!(w.kind, ViewKind::Projected { .. } | ViewKind::Section { .. }))
                    })
                    .collect()
            })
            .unwrap_or_default(),
        None => Vec::new(),
    };
    // Views moved by a drag are drawn where they are going.
    let mut drag_guides: (Option<[f64; 2]>, Option<[f64; 2]>) = (None, None);
    let mut drag_shift = [0.0, 0.0];
    if let (Some((id, start, at0)), Some(now)) = (k.drag, cursor)
        && let Some(dd) = drawing(app, k)
    {
        let (to_at, gx, gy) = drag_to(dd, id, start, at0, now, shift_held, &carried, reach * 1.5);
        drag_shift = [to_at[0] - at0[0], to_at[1] - at0[1]];
        drag_guides = (gx, gy);
    }
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
    // The selected view's frame, a dragged one where it will go (with the views it takes along):
    // red where it would be over another view or out of the frame.
    let mut warning: Option<&'static str> = None;
    for x in v["views"].as_array().into_iter().flatten() {
        let (Some(id), Some(lo), Some(hi)) = (x["id"].as_u64(), read2(&x["min"]), read2(&x["max"])) else { continue };
        let s = if carried.contains(&id) { drag_shift } else { [0.0, 0.0] };
        let marked = Some(id) == k.selected || carried.contains(&id) || (k.tool == Tool::Projected && Some(id) == k.parent);
        if marked {
            let (lo, hi) = ([lo[0] + s[0], lo[1] + s[1]], [hi[0] + s[0], hi[1] + s[1]]);
            let bad = if carried.contains(&id) && drag_shift != [0.0, 0.0] { clash(&v, lo, hi, &carried) } else { None };
            warning = warning.or(bad);
            let r = Rect::from_two_pos(to([lo[0] - 3.0, lo[1] - 3.0]), to([hi[0] + 3.0, hi[1] + 3.0]));
            painter.rect_stroke(r, 2.0, Stroke::new(1.5, if bad.is_some() { t.error } else { t.accent }), egui::StrokeKind::Outside);
        }
        crate::scenario::publish_handle(&format!("drawing:view:{id}"), to([(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0]));
    }
    let guide_line = |a: [f64; 2], b: [f64; 2]| painter.extend(egui::Shape::dashed_line(&[to(a), to(b)], Stroke::new(1.0, t.accent), 6.0, 4.0));
    if let (Some((id, ..)), true) = (k.drag, drag_shift != [0.0, 0.0])
        && let Some(at0) = drawing(app, k).and_then(|dd| dd.view(id)).map(|x| x.at)
    {
        let now = [at0[0] + drag_shift[0], at0[1] + drag_shift[1]];
        if let Some(c) = drag_guides.0 {
            guide_line(c, now);
        }
        if let Some(c) = drag_guides.1 {
            guide_line(c, now);
        }
    }
    for e in v["errors"].as_array().into_iter().flatten() {
        if let Some(msg) = e.get(1).and_then(Value::as_str) {
            painter.text(paper.left_top() + vec2(8.0, -18.0), Align2::LEFT_BOTTOM, msg, FontId::proportional(t.font_label), t.error);
        }
    }
    // The selected dimension: a ring round its text (where it is going while dragged).
    for x in v["dims"].as_array().into_iter().flatten() {
        let (Some(dim), Some(p)) = (x.get(0).and_then(Value::as_u64), x.get(1).and_then(read2)) else { continue };
        crate::scenario::publish_handle(&format!("drawing:dim:{dim}"), to(p));
        if k.dim.as_ref().is_some_and(|(s, ..)| *s == dim) {
            let p = match (k.dim_drag, cursor) {
                (Some((id, start)), Some(now)) if id == dim => [p[0] + now[0] - start[0], p[1] + now[1] - start[1]],
                _ => p,
            };
            painter.circle_stroke(to(p), px(5.0), Stroke::new(1.5, t.accent));
        }
    }
    crate::scenario::publish_rect("drawing:paper", paper);
    for (name, p) in [("drawing:origin", [0.0, 0.0]), ("drawing:corner", [w, h])] {
        crate::scenario::publish_handle(name, to(p));
    }

    // What the tool would pick, and what it would put, under the pointer.
    let guide = Stroke::new(1.5, t.accent);
    let ghost = Stroke::new(1.0, t.accent.gamma_multiply(0.8));
    k.hover = None;
    k.choices = 0;
    let only = |k: &DrawUi| k.dim_picks.first().map(|(view, _)| *view).or(k.pick_view);
    let mut near: Vec<Candidate> = Vec::new();
    if let Some(at) = cursor {
        let want = match k.tool {
            Tool::Dimension if !k.dim_ready() => Some(k.want()),
            Tool::Section if k.picks.len() < 2 => Some(Want::Points),
            _ => None,
        };
        if let Some(want) = want {
            let found = candidates(&v, at, reach, want, only(k));
            // Space goes to the next candidate; moving the pointer away starts again.
            if k.cycle_at.is_none_or(|p| dist(p, at) > reach * 0.5) {
                k.cycle = 0;
                k.cycle_at = Some(at);
            }
            if found.len() > 1 && ctx.memory(|m| m.focused().is_none()) && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Space)) {
                k.cycle += 1;
            }
            if let Some(c) = found.get(k.cycle % found.len().max(1)) {
                k.hover = Some((c.view, c.pick));
                k.hover_kind = c.kind;
                k.choices = found.len();
            }
            // The points about the pointer, shown faintly so it is seen what is there to pick.
            near = candidates(&v, at, reach * 3.0, want, only(k)).into_iter().filter(|c| matches!(c.pick, Pick::Point(_))).collect();
        }
    }
    let mark = |p: Pick, stroke: Stroke| match p {
        Pick::Point(q) => {
            painter.circle_stroke(to(q), 5.0, stroke);
            painter.circle_filled(to(q), 2.5, stroke.color);
        }
        Pick::Line(a, b) => {
            painter.line_segment([to(a), to(b)], Stroke::new(stroke.width + 1.5, stroke.color.gamma_multiply(0.7)));
        }
        Pick::Circle(c, r, _) => {
            painter.circle_stroke(to(c), (r * scale) as f32, Stroke::new(stroke.width + 1.5, stroke.color.gamma_multiply(0.7)));
        }
    };
    // Reference points near the pointer, each drawn by its kind; the one a click takes in full.
    let symbol = |q: [f64; 2], kind: &str, stroke: Stroke| {
        let c = to(q);
        let r = 4.5;
        match kind {
            "mid" => {
                let pts = vec![c + vec2(0.0, -r), c + vec2(r, r * 0.8), c + vec2(-r, r * 0.8)];
                painter.add(egui::Shape::closed_line(pts, stroke));
            }
            "center" => {
                painter.circle_stroke(c, r, stroke);
                painter.line_segment([c - vec2(r * 1.6, 0.0), c + vec2(r * 1.6, 0.0)], stroke);
                painter.line_segment([c - vec2(0.0, r * 1.6), c + vec2(0.0, r * 1.6)], stroke);
            }
            "quad" => {
                let pts = vec![c + vec2(0.0, -r), c + vec2(r, 0.0), c + vec2(0.0, r), c + vec2(-r, 0.0)];
                painter.add(egui::Shape::closed_line(pts, stroke));
            }
            "cross" | "sharp" => {
                painter.line_segment([c - vec2(r, r), c + vec2(r, r)], stroke);
                painter.line_segment([c - vec2(r, -r), c + vec2(r, -r)], stroke);
            }
            _ => {
                painter.rect_stroke(Rect::from_center_size(c, vec2(2.0 * r, 2.0 * r)), 0.0, stroke, egui::StrokeKind::Middle);
            }
        }
    };
    for c in &near {
        if let Pick::Point(q) = c.pick
            && Some((c.view, c.pick)) != k.hover
        {
            symbol(q, c.kind, Stroke::new(1.0, t.accent.gamma_multiply(0.45)));
        }
    }
    if let Some((_, p)) = k.hover {
        match p {
            Pick::Point(q) => {
                symbol(q, k.hover_kind, Stroke::new(2.0, t.accent));
                painter.circle_filled(to(q), 2.0, t.accent);
            }
            _ => mark(p, guide),
        }
        // What it is, and that Space goes to the next when there are more there.
        if let Some(at) = cursor {
            let mut text = crate::i18n::tr_label(kind_label(k.hover_kind)).to_string();
            if k.choices > 1 {
                text = format!("{text}  {}/{} · {}", k.cycle % k.choices + 1, k.choices, crate::i18n::tr_label("Space: next"));
            }
            painter.text(to(at) + vec2(14.0, 14.0), Align2::LEFT_TOP, text, FontId::proportional(t.font_label), t.accent);
        }
    }
    for (_, p) in &k.dim_picks {
        mark(*p, Stroke::new(2.0, t.accent));
    }
    for p in &k.picks {
        painter.circle_filled(to(*p), 4.0, t.accent);
    }
    let did = k.drawing;
    // A view not yet put: its frame where it would go (red where it would clash), and why.
    let frame_at = |f: Option<([f64; 2], [f64; 2])>, at: [f64; 2]| -> Option<&'static str> {
        let mut bad = None;
        if let Some((lo, hi)) = f {
            let (lo, hi) = ([at[0] + lo[0], at[1] + lo[1]], [at[0] + hi[0], at[1] + hi[1]]);
            bad = clash(&v, lo, hi, &[]);
            let r = Rect::from_two_pos(to(lo), to(hi));
            painter.rect_stroke(r, 0.0, if bad.is_some() { Stroke::new(1.5, t.error) } else { ghost }, egui::StrokeKind::Middle);
        }
        painter.line_segment([to(at) - vec2(6.0, 0.0), to(at) + vec2(6.0, 0.0)], ghost);
        painter.line_segment([to(at) - vec2(0.0, 6.0), to(at) + vec2(0.0, 6.0)], ghost);
        bad
    };
    let centres: Vec<[f64; 2]> = drawing(app, k).map(|dd| dd.views.iter().map(|w| w.at).collect()).unwrap_or_default();
    // The view or dimension the next click puts, where it would go.
    let mut put: Option<(&'static str, Value)> = None;
    if let (Some(at), Some(did)) = (cursor, did) {
        match k.tool {
            Tool::Base => {
                let o = ORIENTATIONS.get(k.orientation).copied().unwrap_or("front");
                let mut p = json!({"drawing": did, "kind": "base", "orientation": o});
                if let Some(s) = k.base_scale.checked_sub(1).and_then(|i| SCALES.get(i)) {
                    p["scale"] = json!(s);
                }
                let f = preview(app, k, p.clone());
                // Lined up with the other views' centres when near their lines.
                let (at, gx, gy) = aligned_to(at, &centres, reach * 1.5);
                for c in [gx, gy].into_iter().flatten() {
                    guide_line(c, at);
                }
                warning = warning.or(frame_at(f, at));
                let mut q = json!({"drawing": did, "orientation": o, "at": at, "hidden": k.base_hidden});
                if let Some(s) = p.get("scale") {
                    q["scale"] = s.clone();
                }
                put = Some(("drawing.view.base", q));
            }
            Tool::Projected => {
                if let Some(parent) = k.parent
                    && let Some((lo, hi, _)) = view_frame(&v, parent)
                {
                    // With Shift (put anywhere) the side is the one the pointer is most toward.
                    let side = side_of(lo, hi, at, !shift_held);
                    let parent_at = drawing(app, k).and_then(|dd| dd.view(parent)).map(|x| x.at);
                    // In line with the parent; with Shift held, anywhere (a free view).
                    let line = if shift_held || side == "iso" {
                        ' '
                    } else if side == "left" || side == "right" {
                        'h'
                    } else {
                        'v'
                    };
                    let place = lined(at, parent_at, line);
                    let f = preview(app, k, json!({"drawing": did, "kind": "projected", "parent": parent, "side": side}));
                    warning = warning.or(frame_at(f, place));
                    if let Some(pa) = parent_at
                        && line != ' '
                    {
                        guide_line(pa, place);
                    }
                    let mut q = json!({"drawing": did, "parent": parent, "side": side, "at": place});
                    if shift_held && side != "iso" {
                        q["free"] = json!(true);
                    }
                    put = Some(("drawing.view.projected", q));
                }
            }
            Tool::Section if k.picks.len() == 1 => {
                let first = k.picks.first().copied().unwrap_or(at);
                let end = k.hover.map(|(_, p)| p.anchors().first().copied().unwrap_or(at)).unwrap_or_else(|| squared(first, at));
                painter.extend(egui::Shape::dashed_line(&[to(first), to(end)], guide, 8.0, 3.0));
            }
            Tool::Section if k.picks.len() == 2 => {
                let (a, b) = (k.picks[0], k.picks[1]);
                painter.extend(egui::Shape::dashed_line(&[to(a), to(b)], guide, 8.0, 3.0));
                if let Some(parent) = k.pick_view {
                    let parent_at = drawing(app, k).and_then(|dd| dd.view(parent)).map(|x| x.at);
                    // A vertical line's section lies beside its parent, a horizontal one's above or below.
                    let line = if (a[0] - b[0]).abs() < 1e-6 {
                        'h'
                    } else if (a[1] - b[1]).abs() < 1e-6 {
                        'v'
                    } else {
                        ' '
                    };
                    let place = lined(at, parent_at, line);
                    let f = preview(app, k, json!({"drawing": did, "kind": "section", "parent": parent, "line": [a, b]}));
                    warning = warning.or(frame_at(f, place));
                    put = Some(("drawing.view.section", json!({"drawing": did, "parent": parent, "line": [a, b], "at": place})));
                }
            }
            Tool::Detail if k.picks.len() == 1 => {
                let c = k.picks[0];
                painter.circle_stroke(to(c), (dist(c, at) * scale) as f32, guide);
            }
            Tool::Detail if k.picks.len() == 2 => {
                let (c, e) = (k.picks[0], k.picks[1]);
                let r = dist(c, e);
                painter.circle_stroke(to(c), (r * scale) as f32, guide);
                if let Some(parent) = k.pick_view {
                    let f = preview(app, k, json!({"drawing": did, "kind": "detail", "parent": parent, "center": c, "radius": r}));
                    warning = warning.or(frame_at(f, at));
                    put = Some(("drawing.view.detail", json!({"drawing": did, "parent": parent, "center": c, "radius": r, "at": at})));
                }
            }
            Tool::Dimension if k.dim_ready() => {
                if let Some(p) = dim_command(k, did, at) {
                    let th = drawing(app, k).map_or(3.5, |dd| dd.dim_text());
                    dim_preview(painter, k, &v, at, &to, scale, th, guide);
                    put = Some(("drawing.dimension", p));
                }
            }
            _ => {}
        }
    }

    if let (Some(wn), Some(at)) = (warning, cursor) {
        painter.text(to(at) + vec2(14.0, -14.0), Align2::LEFT_BOTTOM, crate::i18n::tr_label(wn), FontId::proportional(t.font_label), t.error);
    }
    // Input: only where the viewport itself is under the pointer (not the panel or a menu).
    let (pressed, released, down, secondary) =
        ctx.input(|i| (i.pointer.primary_pressed(), i.pointer.primary_released(), i.pointer.primary_down(), i.pointer.secondary_pressed()));
    let busy = k.tool != Tool::Select || k.drag.is_some() || k.dim_drag.is_some();
    // Esc and a right click step back: the picks so far, then the tool (not the editor).
    let esc = !app.esc_handled && busy && ctx.input(|i| i.key_pressed(egui::Key::Escape));
    if esc {
        app.esc_handled = true;
    }
    if esc || (secondary && over) {
        step_back(k);
    }
    // Backspace takes back the last pick only.
    if (!k.picks.is_empty() || !k.dim_picks.is_empty())
        && ctx.memory(|m| m.focused().is_none())
        && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Backspace))
    {
        if k.dim_picks.pop().is_none() {
            k.picks.pop();
        }
        if k.picks.is_empty() && k.dim_picks.is_empty() {
            k.pick_view = None;
        }
    }
    // Delete removes the selected view or dimension.
    if k.tool == Tool::Select
        && (k.selected.is_some() || k.dim.is_some())
        && ctx.memory(|m| m.focused().is_none())
        && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Delete))
        && let Some(did) = did
    {
        if let Some((dim, ..)) = k.dim.take() {
            run(app, k, "drawing.dimension.delete", json!({"drawing": did, "dimension": dim}));
        } else if let Some(view) = k.selected.take() {
            run(app, k, "drawing.view.delete", json!({"drawing": did, "view": view}));
        }
    }
    if let (Some(at), true) = (cursor, pressed) {
        press(app, k, &v, at, put);
    }
    if released {
        if let Some((id, start, at0)) = k.drag.take()
            && let Some(now) = cursor
            && let Some(did) = did
            && dist(now, start) > 0.5
            && let Some(dd) = drawing(app, k)
        {
            let (to_at, ..) = drag_to(dd, id, start, at0, now, shift_held, &carried, reach * 1.5);
            // Shift-dragged off its line, a projected or section view goes free.
            let breaks = shift_held && dd.view(id).is_some_and(|x| !x.free && matches!(&x.kind, ViewKind::Projected { side, .. } if side != "iso"));
            let mut p = json!({"drawing": did, "view": id, "at": to_at});
            if breaks {
                p["free"] = json!(true);
            }
            run(app, k, "drawing.view.edit", p);
        }
        if let Some((dim, start)) = k.dim_drag.take()
            && let Some(now) = cursor
            && let Some(did) = did
            && dist(now, start) > 0.5
        {
            // The text goes where it was dragged: the dimension through that point.
            let text = v["dims"]
                .as_array()
                .and_then(|a| a.iter().find(|x| x.get(0).and_then(Value::as_u64) == Some(dim)))
                .and_then(|x| x.get(1))
                .and_then(read2);
            if let Some(p) = text {
                let to_at = [p[0] + now[0] - start[0], p[1] + now[1] - start[1]];
                run(app, k, "drawing.dimension.edit", json!({"drawing": did, "dimension": dim, "place": to_at}));
            }
        }
    }
    if down || busy || k.drag.is_some() {
        ctx.request_repaint();
    }
    true
}

/// The `drawing.dimension` call for what the dimension tool has picked, put at `at`.
fn dim_command(k: &DrawUi, drawing: u64, at: [f64; 2]) -> Option<Value> {
    let (view, first) = *k.dim_picks.first()?;
    let mut kind = DIM_KINDS.get(k.dim_kind).copied().unwrap_or("auto");
    // A circle picked for an automatic dimension: its diameter (an arc: its radius).
    if kind == "auto"
        && k.arc == 0
        && let Pick::Circle(.., full) = first
    {
        kind = if full { "diameter" } else { "radius" };
    }
    let points: Vec<[f64; 2]> = k.dim_picks.iter().flat_map(|(_, p)| p.sent()).collect();
    let mut p = json!({"drawing": drawing, "view": view, "kind": kind, "points": points, "place": at});
    if !k.tolerance.trim().is_empty() {
        p["tolerance"] = json!(k.tolerance.trim());
    }
    if k.arc != 0 && k.dim_kind <= 3 {
        p["arc"] = json!(ARCS.get(k.arc).copied().unwrap_or(""));
    }
    Some(p)
}

/// Text on the sheet as the drawing prints it: `height` mm, turned `angle_deg`, its baseline
/// starting (left), centred or ending (right) at `at`.
#[allow(clippy::too_many_arguments)]
fn sheet_text(
    painter: &egui::Painter,
    to: &dyn Fn([f64; 2]) -> Pos2,
    scale: f64,
    at: [f64; 2],
    height: f64,
    angle_deg: f64,
    text: String,
    align: Align2,
    color: Color32,
) {
    let size = (height * scale / 0.72) as f32;
    if !(2.0..=400.0).contains(&size) {
        return;
    }
    let angle = angle_deg.to_radians() as f32;
    let galley = painter.layout_no_wrap(text, FontId::proportional(size), color);
    let wpx = galley.size().x;
    let shift = match align.x() {
        egui::Align::Center => -wpx / 2.0,
        egui::Align::Max => -wpx,
        egui::Align::Min => 0.0,
    };
    let dir = vec2(angle.cos(), -angle.sin());
    let up = vec2(dir.y, -dir.x);
    let pos = to(at) + dir * shift + up * (size * 0.8);
    painter.add(egui::epaint::TextShape::new(pos, galley, color).with_angle(-angle));
}

/// An arrowhead on the sheet: tip at `tip`, pointing along `dir`, `len` mm long (1:3).
fn sheet_arrow(painter: &egui::Painter, to: &dyn Fn([f64; 2]) -> Pos2, tip: [f64; 2], dir: [f64; 2], len: f64, color: Color32) {
    let l = dir[0].hypot(dir[1]);
    if l < 1e-12 {
        return;
    }
    let (dx, dy) = (dir[0] / l, dir[1] / l);
    let (nx, ny) = (-dy, dx);
    let base = [tip[0] - dx * len, tip[1] - dy * len];
    let w = len / 6.0;
    let pts = vec![to(tip), to([base[0] + nx * w, base[1] + ny * w]), to([base[0] - nx * w, base[1] - ny * w])];
    painter.add(egui::Shape::convex_polygon(pts, color, Stroke::NONE));
}

/// The dimension the next click puts, drawn as it will be (the same lines, arrows and text as the
/// drawing draws, `th` mm text) where the pointer puts it.
#[allow(clippy::too_many_arguments)]
fn dim_preview(painter: &egui::Painter, k: &DrawUi, v: &Value, at: [f64; 2], to: &dyn Fn([f64; 2]) -> Pos2, scale: f64, th: f64, stroke: Stroke) {
    let Some((view, first)) = k.dim_picks.first().copied() else { return };
    let vs = view_frame(v, view).map(|f| f.2).unwrap_or(1.0).max(1e-9);
    let ink = stroke.color;
    let thin = Stroke::new(1.0, ink);
    let arrow = th * 3.0 / 3.5;
    let seg = |a: [f64; 2], b: [f64; 2]| {
        painter.line_segment([to(a), to(b)], thin);
    };
    let circle_alone = matches!(first, Pick::Circle(..)) && (k.dim_kind == 4 || k.dim_kind == 5 || (k.dim_kind == 0 && k.arc == 0));
    if circle_alone {
        let Pick::Circle(c, r, full) = first else { return };
        let diameter = k.dim_kind == 4 || (k.dim_kind == 0 && full);
        let text = if diameter { format!("Ø{}", number(2.0 * r / vs)) } else { format!("R{}", number(r / vs)) };
        let l = dist(at, c).max(1e-9);
        let dir = [(at[0] - c[0]) / l, (at[1] - c[1]) / l];
        let tip = [c[0] + dir[0] * r, c[1] + dir[1] * r];
        let start = if diameter { [c[0] - dir[0] * r, c[1] - dir[1] * r] } else { c };
        let end = if l > r + 2.0 { at } else { [tip[0] + dir[0] * 8.0, tip[1] + dir[1] * 8.0] };
        seg(start, end);
        sheet_arrow(painter, to, tip, [-dir[0], -dir[1]], arrow, ink);
        if diameter {
            sheet_arrow(painter, to, start, dir, arrow, ink);
        }
        let right = dir[0] >= 0.0;
        let len = text.chars().count() as f64 * th * 0.62 + 2.0;
        seg(end, [end[0] + if right { len } else { -len }, end[1]]);
        let align = if right { Align2::LEFT_BOTTOM } else { Align2::RIGHT_BOTTOM };
        sheet_text(painter, to, scale, [end[0] + if right { 1.0 } else { -1.0 }, end[1] + 1.0], th, 0.0, text, align, ink);
        return;
    }
    let anchors: Vec<[f64; 2]> = k.dim_picks.iter().flat_map(|(_, p)| p.anchors()).collect();
    if k.dim_kind == 6 {
        if let (Some(c), Some(a), Some(b)) = (anchors.first(), anchors.get(1), anchors.get(2)) {
            seg(*c, *a);
            seg(*c, *b);
            let ang = ((a[1] - c[1]).atan2(a[0] - c[0]) - (b[1] - c[1]).atan2(b[0] - c[0])).abs().to_degrees();
            sheet_text(
                painter,
                to,
                scale,
                at,
                th,
                0.0,
                format!("{}°", number(if ang > 180.0 { 360.0 - ang } else { ang })),
                Align2::CENTER_BOTTOM,
                ink,
            );
        }
        return;
    }
    let (Some(p1), Some(p2)) = (anchors.first().copied(), anchors.get(1).copied()) else { return };
    let kind = if k.dim_kind == 0 { auto_kind(p1, p2, at) } else { k.dim_kind };
    let dir = match kind {
        1 => [1.0, 0.0],
        2 => [0.0, 1.0],
        _ => {
            let l = dist(p1, p2).max(1e-9);
            [(p2[0] - p1[0]) / l, (p2[1] - p1[1]) / l]
        }
    };
    let n = [-dir[1], dir[0]];
    let level = at[0] * n[0] + at[1] * n[1];
    let foot = |p: [f64; 2]| {
        let d = level - (p[0] * n[0] + p[1] * n[1]);
        [p[0] + n[0] * d, p[1] + n[1] * d]
    };
    let (a, b) = (foot(p1), foot(p2));
    // Extension lines from 1 mm off the points to 2 mm past the dimension line.
    for (p, f) in [(p1, a), (p2, b)] {
        let l = dist(p, f);
        if l > 1.0 {
            let u = [(f[0] - p[0]) / l, (f[1] - p[1]) / l];
            seg([p[0] + u[0], p[1] + u[1]], [f[0] + u[0] * 2.0, f[1] + u[1] * 2.0]);
        }
    }
    seg(a, b);
    let ab = dist(a, b);
    let u = if ab > 1e-9 { [(b[0] - a[0]) / ab, (b[1] - a[1]) / ab] } else { dir };
    if ab > 8.0 {
        sheet_arrow(painter, to, a, [-u[0], -u[1]], arrow, ink);
        sheet_arrow(painter, to, b, u, arrow, ink);
    } else {
        sheet_arrow(painter, to, a, u, arrow, ink);
        sheet_arrow(painter, to, b, [-u[0], -u[1]], arrow, ink);
        seg([a[0] - u[0] * 6.0, a[1] - u[1] * 6.0], a);
        seg(b, [b[0] + u[0] * 6.0, b[1] + u[1] * 6.0]);
    }
    let value = ((b[0] - a[0]) * dir[0] + (b[1] - a[1]) * dir[1]).abs() / vs;
    let text = number(value);
    // The text in the middle between the extension lines, else where the pointer is along the
    // line (the line carried on to it), 1 mm above it, turned with it.
    let t = (at[0] - a[0]) * u[0] + (at[1] - a[1]) * u[1];
    let t = if (0.0..=ab).contains(&t) { ab * 0.5 } else { t };
    let half = text.chars().count() as f64 * th * 0.62 * 0.5;
    if t < -half - 1.0 {
        seg([a[0] + u[0] * (t - half), a[1] + u[1] * (t - half)], a);
    } else if t > ab + half + 1.0 {
        seg(b, [a[0] + u[0] * (t + half), a[1] + u[1] * (t + half)]);
    }
    let mut angle = dir[1].atan2(dir[0]).to_degrees();
    if angle > 90.0 + 1e-9 {
        angle -= 180.0;
    } else if angle <= -90.0 + 1e-9 {
        angle += 180.0;
    }
    let up = [-(angle.to_radians().sin()), angle.to_radians().cos()];
    let p = [a[0] + u[0] * t + up[0], a[1] + u[1] * t + up[1]];
    sheet_text(painter, to, scale, p, th, angle, text, Align2::CENTER_BOTTOM, ink);
}

/// A press on the sheet at `at` (sheet mm); `put`: the view or dimension that the tool would put
/// there.
fn press(app: &mut SolveApp, k: &mut DrawUi, v: &Value, at: [f64; 2], put: Option<(&'static str, Value)>) {
    let Some(id) = k.drawing else { return };
    match k.tool {
        Tool::Select => {
            // A dimension's text first (within 5 mm), else a view.
            let near = v["dims"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|x| Some((x.get(0)?.as_u64()?, read2(x.get(1)?)?)))
                .map(|(id, p)| (id, dist(p, at)))
                .filter(|(_, dd)| *dd < 5.0)
                .min_by(|a, b| a.1.total_cmp(&b.1));
            if let Some((dim, _)) = near {
                let x = drawing(app, k).and_then(|d| d.dims.iter().find(|x| x.id == dim)).cloned();
                k.dim = x.map(|x| (dim, x.text, x.tolerance));
                k.dim_drag = Some((dim, at));
                k.selected = None;
                return;
            }
            k.dim = None;
            k.selected = view_at(v, at);
            if let Some(view) = k.selected
                && let Some(place) = drawing(app, k).and_then(|d| d.view(view)).map(|x| x.at)
            {
                k.drag = Some((view, at, place));
            }
        }
        Tool::Note => {
            if k.note.trim().is_empty() {
                k.status = Some(crate::i18n::tr_label("Type the note's text first").to_string());
                return;
            }
            let text = k.note.clone();
            if run(app, k, "drawing.note", json!({"drawing": id, "at": at, "text": text})).is_some() {
                k.note.clear();
            }
        }
        Tool::Base => {
            if let Some((cmd, p)) = put
                && let Some(r) = run(app, k, cmd, p)
            {
                // Then the views projected from it.
                k.selected = r["view"].as_u64();
                k.parent = k.selected;
                k.set_tool(Tool::Projected);
            }
        }
        Tool::Projected => {
            if k.parent.is_none() {
                k.parent =
                    view_at(v, at).filter(|p| drawing(app, k).and_then(|d| d.view(*p)).is_some_and(|x| !matches!(x.kind, ViewKind::Flat { .. })));
                if k.parent.is_none() {
                    k.status = Some(crate::i18n::tr_label("Click a view to project from").to_string());
                }
                return;
            }
            // A click on another view projects from that one; on the parent itself, nothing.
            if let Some(pv) = view_at(v, at) {
                if Some(pv) != k.parent && drawing(app, k).and_then(|d| d.view(pv)).is_some_and(|x| !matches!(x.kind, ViewKind::Flat { .. })) {
                    k.parent = Some(pv);
                }
                return;
            }
            if let Some((cmd, p)) = put
                && let Some(r) = run(app, k, cmd, p)
            {
                k.selected = r["view"].as_u64();
            }
        }
        Tool::Section | Tool::Detail => {
            if k.picks.len() == 2 {
                if let Some((cmd, p)) = put
                    && let Some(r) = run(app, k, cmd, p)
                {
                    k.selected = r["view"].as_u64();
                    k.set_tool(Tool::Select);
                }
                return;
            }
            if k.picks.is_empty() {
                let Some(view) = view_at(v, at) else {
                    k.status = Some(crate::i18n::tr_label("Start inside a view").to_string());
                    return;
                };
                k.pick_view = Some(view);
            }
            // A section's line snaps to the model's points, and goes square when nearly so.
            let p = match (k.tool, k.hover, k.picks.first()) {
                (Tool::Section, Some((_, pick)), _) => pick.anchors().first().copied().unwrap_or(at),
                (Tool::Section, None, Some(first)) => squared(*first, at),
                _ => at,
            };
            if k.picks.len() == 1 && k.picks.first().is_some_and(|f| dist(*f, p) < 0.5) {
                return;
            }
            k.picks.push(p);
            k.status = None;
        }
        Tool::Dimension => {
            if k.dim_ready() {
                if let Some((cmd, p)) = put
                    && run(app, k, cmd, p).is_some()
                {
                    k.dim_picks.clear();
                }
                return;
            }
            match k.hover {
                Some(pick) => {
                    k.dim_picks.push(pick);
                    k.status = None;
                }
                None => k.status = Some(crate::i18n::tr_label("Nothing to measure there: click on an edge, a circle or a point").to_string()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// A 100 × 50 × 20 plate with a 20 mm hole, and the drawing editor opened on it (an empty
    /// sheet; the views are put by the tests).
    fn editor() -> crate::scenario::Harness {
        let mut h = crate::scenario::Harness::new();
        h.app.run("solid.box", json!({"length": 100, "width": 50, "height": 20})).unwrap();
        h.app.run("solid.cylinder", json!({"diameter": 20, "height": 30, "base": [50, 25, -5], "operation": "cut"})).unwrap();
        h.step(&json!({"start": "drawing.create"})).unwrap();
        h.frames(4);
        h
    }

    /// The editor with the standard four views put by commands (front, top, side, iso).
    fn with_views() -> crate::scenario::Harness {
        let mut h = editor();
        let d = h.app.session.doc.drawings[0].id;
        h.app.run("drawing.edit", json!({"drawing": d, "scale": "1:1"})).unwrap();
        let front =
            h.app.run("drawing.view.base", json!({"drawing": d, "orientation": "front", "at": [120, 200]})).unwrap()["view"].as_u64().unwrap();
        h.app.run("drawing.view.projected", json!({"drawing": d, "parent": front, "side": "below", "at": [120, 120]})).unwrap();
        h.app.run("drawing.view.projected", json!({"drawing": d, "parent": front, "side": "right", "at": [240, 200]})).unwrap();
        h.app.run("drawing.view.projected", json!({"drawing": d, "parent": front, "side": "iso", "at": [300, 110]})).unwrap();
        if let Some(Kind::Drawing(k)) = h.app.dialog.as_mut().map(|d| &mut d.kind) {
            k.set_tool(Tool::Select);
        }
        h.frames(3);
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

    fn set(h: &mut crate::scenario::Harness, f: impl FnOnce(&mut DrawUi)) {
        if let Some(Kind::Drawing(k)) = h.app.dialog.as_mut().map(|d| &mut d.kind) {
            f(k);
        }
    }

    /// Move the pointer there and click (a frame between, so the tool sees where it is).
    fn click(h: &mut crate::scenario::Harness, p: [f64; 2]) {
        let s = screen(p);
        h.step(&json!({"move": s})).unwrap();
        h.frames(2);
        h.step(&json!({"click": s})).unwrap();
        h.frames(2);
    }

    /// Left alone, the open editor changes nothing (no edit each frame, which would draw the
    /// sheet again and again).
    #[test]
    fn the_open_editor_leaves_the_design_alone() {
        let mut h = with_views();
        let d = h.app.session.doc.drawings.first().cloned().unwrap();
        // Across the front view: the plate's lower corners (100 mm apart at 1:1, about (120, 200)).
        let dim = h
            .app
            .run("drawing.dimension", json!({"drawing": d.id, "view": d.views[0].id, "kind": "horizontal", "points": [[70, 190], [170, 190]]}))
            .ok()
            .and_then(|v| v["dimension"].as_u64());
        assert!(dim.is_some());
        set(&mut h, |k| k.dim = dim.map(|dim| (dim, String::new(), String::new())));
        h.frames(3);
        let revision = h.app.session.revision;
        h.frames(10);
        assert_eq!(h.app.session.revision, revision, "{:?}", ui(&h).status);
    }

    /// The ribbon's Flat Pattern (DRAWING tab): the editor opens with the sheet metal part's flat
    /// pattern added to the drawing.
    #[test]
    fn the_ribbon_adds_a_flat_pattern_view() {
        let mut h = crate::scenario::Harness::new();
        h.app.run("sketch.create", json!({"plane": "XY", "name": "Square"})).unwrap();
        h.app.run("sketch.rectangle.two_point", json!({"p0": [-50, -50], "p1": [50, 50]})).unwrap();
        h.app.run("sketch.finish", json!({})).unwrap();
        h.app.run("sketch.create", json!({"plane": {"origin": [0, 0, 80], "x_dir": [1, 0, 0], "y_dir": [0, 1, 0]}, "name": "Round"})).unwrap();
        h.app.run("sketch.circle.center", json!({"center": [0, 0], "radius": 30})).unwrap();
        h.app.run("sketch.finish", json!({})).unwrap();
        h.app.run("sheet.loft", json!({"sections": ["Square", "Round"]})).unwrap();
        h.step(&json!({"start": "drawing.view.flat"})).unwrap();
        h.frames(4);
        let d = h.app.session.doc.drawings.first().cloned().unwrap();
        let flat = d.views.iter().find(|v| matches!(v.kind, ViewKind::Flat { .. })).map(|v| v.id);
        assert!(flat.is_some(), "{:?} {:?}", d.views, ui(&h).status);
        assert_eq!(ui(&h).selected, flat);
        assert!(crate::scenario::handle_at(&format!("drawing:view:{}", flat.unwrap_or(0))).is_some());
    }

    /// A new drawing is an empty sheet: no view is put by itself; the base view tool is ready.
    #[test]
    fn a_new_drawing_is_an_empty_sheet_with_the_base_view_tool() {
        let h = editor();
        assert_eq!(h.app.session.doc.drawings.len(), 1);
        let d = h.app.session.doc.drawings.first().cloned().unwrap();
        assert!(d.views.is_empty(), "{:?}", d.views);
        assert!(active(&h.app) && crate::dialogs_sheet::flat_active(&h.app), "the sheet takes the viewport");
        assert!(crate::scenario::rect_of("drawing:paper").is_some_and(|r| r.width() > 100.0));
        assert_eq!(ui(&h).drawing, Some(d.id));
        assert_eq!(ui(&h).tool, Tool::Base);
    }

    /// The base view goes where it is clicked, then projected views go beside it, lined up with
    /// it, on the side the pointer is (a corner: iso).
    #[test]
    fn views_are_put_by_hand_base_then_projected() {
        let mut h = editor();
        set(&mut h, |k| k.base_scale = 4); // 1:1
        click(&mut h, [130.0, 190.0]);
        let d = h.app.session.doc.drawings[0].clone();
        assert_eq!(d.views.len(), 1, "{:?}", ui(&h).status);
        let base = d.views[0].clone();
        assert!(matches!(base.kind, ViewKind::Base { .. }) && dist(base.at, [130.0, 190.0]) < 1.0, "{base:?}");
        assert_eq!((ui(&h).tool, ui(&h).parent), (Tool::Projected, Some(base.id)));
        // To the right but a little high: kept at the base view's height.
        click(&mut h, [260.0, 197.0]);
        // Below.
        click(&mut h, [128.0, 100.0]);
        // Off the lower right corner: an iso view.
        click(&mut h, [280.0, 90.0]);
        let d = h.app.session.doc.drawings[0].clone();
        assert_eq!(d.views.len(), 4, "{:?}", ui(&h).status);
        let side = |v: &solidcraft_engine::doc::drawing::DrawView| match &v.kind {
            ViewKind::Projected { side, .. } => side.clone(),
            _ => String::new(),
        };
        assert_eq!(d.views.iter().skip(1).map(side).collect::<Vec<_>>(), ["right", "below", "iso"]);
        assert!((d.views[1].at[1] - base.at[1]).abs() < 1e-6, "lined up across: {:?}", d.views[1].at);
        assert!((d.views[2].at[0] - base.at[0]).abs() < 1e-6, "lined up below: {:?}", d.views[2].at);
        // Esc ends the tool, not the editor.
        h.step(&json!({"key": "Escape"})).unwrap();
        h.frames(2);
        assert!(active(&h.app));
        assert_eq!(ui(&h).tool, Tool::Select);
        // Moving the base view takes its projected views along.
        let p = crate::scenario::handle_at(&format!("drawing:view:{}", base.id)).unwrap();
        let a = [f64::from(p.x), f64::from(p.y)];
        let dx = screen([base.at[0] + 10.0, 0.0])[0] - screen([base.at[0], 0.0])[0];
        h.step(&json!({"drag": [a, [a[0] + dx, a[1]]], "steps": 6})).unwrap();
        h.frames(3);
        let moved = h.app.session.doc.drawings[0].clone();
        for (old, new) in d.views.iter().zip(&moved.views) {
            assert!((new.at[0] - old.at[0] - 10.0).abs() < 1.0 && (new.at[1] - old.at[1]).abs() < 1e-6, "{old:?} → {new:?}");
        }
    }

    /// Notes are typed and placed; a section takes its line's two clicks, then where it goes.
    #[test]
    fn notes_and_sections_are_placed_by_clicks() {
        let mut h = with_views();
        set(&mut h, |k| {
            k.set_tool(Tool::Note);
            k.note = "Sorjamentes".into();
        });
        click(&mut h, [40.0, 60.0]);
        let notes = h.app.session.doc.drawings.first().map(|d| d.notes.clone()).unwrap();
        assert!(notes.len() == 1 && notes[0].text == "Sorjamentes" && (notes[0].at[0] - 40.0).abs() < 1.0, "{notes:?}");
        // A section across the top view (centred at (120, 120), 100 wide at 1:1): its line, then its place.
        set(&mut h, |k| k.set_tool(Tool::Section));
        click(&mut h, [63.0, 121.0]);
        click(&mut h, [177.0, 122.0]);
        assert_eq!(ui(&h).picks.len(), 2, "{:?}", ui(&h).status);
        let picks = ui(&h).picks;
        assert!(picks[0][1] == picks[1][1] && (picks[0][1] - 121.0).abs() < 0.05, "squared: {picks:?}");
        click(&mut h, [130.0, 50.0]);
        let d = h.app.session.doc.drawings.first().cloned().unwrap();
        assert_eq!(d.views.len(), 5, "{:?}", ui(&h).status);
        let sec = d.views.last().cloned().unwrap();
        assert!(matches!(sec.kind, ViewKind::Section { .. }));
        // A horizontal cutting line: the section lies below its parent, in line with it.
        assert!((sec.at[0] - 120.0).abs() < 1e-6 && (sec.at[1] - 50.0).abs() < 1.0, "{:?}", sec.at);
        assert_eq!(ui(&h).tool, Tool::Select);
    }

    #[test]
    fn the_ribbon_dimension_kinds_open_the_tool_with_their_kind() {
        let mut h = with_views();
        for (id, kind, arc) in [
            ("drawing.dimension", 0, 0),
            ("drawing.dimension.vertical", 2, 0),
            ("drawing.dimension.diameter", 4, 0),
            ("drawing.dimension.arcs", 1, 3),
        ] {
            h.app.dialog = None;
            h.step(&json!({"start": id})).unwrap();
            h.frames(2);
            let k = ui(&h);
            assert_eq!((k.tool, k.dim_kind, k.arc), (Tool::Dimension, kind, arc), "{id}");
            assert_eq!(h.app.session.doc.drawings.len(), 1, "{id}: no new drawing");
        }
    }

    /// Dimensions measure only what is there: a click on empty paper picks nothing; an edge
    /// measures its length and the dimension goes where it is then clicked; a circle its diameter.
    #[test]
    fn dimensions_pick_edges_and_circles_and_go_where_they_are_put() {
        let mut h = with_views();
        set(&mut h, |k| k.set_tool(Tool::Dimension));
        // Empty paper, between the views.
        click(&mut h, [200.0, 150.0]);
        assert!(ui(&h).dim_picks.is_empty());
        assert!(ui(&h).status.is_some());
        // The front view's bottom edge (100 long, at y = 190 about x = 120), then above the view.
        click(&mut h, [100.0, 190.0]);
        assert!(matches!(ui(&h).dim_picks.first().map(|p| p.1), Some(Pick::Line(..))), "{:?}", ui(&h).dim_picks);
        click(&mut h, [110.0, 225.0]);
        let d = h.app.session.doc.drawings[0].clone();
        assert_eq!(d.dims.len(), 1, "{:?}", ui(&h).status);
        let x = &d.dims[0];
        assert_eq!(x.kind, solidcraft_engine::doc::drawing::DimKind::Horizontal);
        let drawn = h.app.session.execute("drawing.render", &json!({"drawing": d.id})).unwrap();
        let texts: Vec<String> =
            drawn["prims"].as_array().unwrap().iter().filter(|p| p["type"] == "text").filter_map(|p| p["text"].as_str().map(String::from)).collect();
        assert!(texts.iter().any(|t| t == "100"), "{texts:?}");
        // Its text sits where it was put.
        let at = drawn["dims"][0][1].clone();
        assert!((at[1].as_f64().unwrap() - 226.0).abs() < 1.5, "{at}");
        // The hole in the top view (r 10 about (120, 120)): its diameter.
        // (On the circle away from its quadrant points, which are points of their own.)
        click(&mut h, [127.07, 127.07]);
        assert!(matches!(ui(&h).dim_picks.first().map(|p| p.1), Some(Pick::Circle(.., true))), "{:?}", ui(&h).dim_picks);
        click(&mut h, [150.0, 150.0]);
        let d = h.app.session.doc.drawings[0].clone();
        assert_eq!(d.dims.last().map(|x| x.kind), Some(solidcraft_engine::doc::drawing::DimKind::Diameter), "{:?}", ui(&h).status);
        let drawn = h.app.session.execute("drawing.render", &json!({"drawing": d.id})).unwrap();
        assert!(drawn["prims"].as_array().unwrap().iter().any(|p| p["text"] == "Ø20"));
    }

    #[test]
    fn a_dimension_is_selected_by_its_text_and_dragged() {
        let mut h = with_views();
        let d = h.app.session.doc.drawings.first().cloned().unwrap();
        let top = d.views.get(1).cloned().unwrap();
        h.app.run("drawing.dimension", json!({"drawing": d.id, "view": top.id, "kind": "diameter", "points": [[130, 120]]})).unwrap();
        h.frames(3);
        let dim = h.app.session.doc.drawings[0].dims[0].id;
        let at = crate::scenario::handle_at(&format!("drawing:dim:{dim}")).unwrap();
        h.step(&json!({"click": [at.x, at.y]})).unwrap();
        h.frames(2);
        assert_eq!(ui(&h).dim.map(|x| x.0), Some(dim));
        assert_eq!(ui(&h).selected, None);
        let a = [f64::from(at.x), f64::from(at.y)];
        h.step(&json!({"drag": [a, [a[0] + 40.0, a[1] - 20.0]], "steps": 6})).unwrap();
        h.frames(3);
        assert!(h.app.session.doc.drawings[0].dims[0].at.is_some(), "{:?}", ui(&h).status);
        // Centre marks on the hole: chain lines in the top view.
        let drawn = h.app.session.execute("drawing.render", &json!({"drawing": d.id})).unwrap();
        assert!(drawn["prims"].as_array().is_some_and(|a| a.iter().filter(|p| p["style"] == "chain").count() >= 2));
    }

    /// Picking at three zoom levels, between two holes 7 mm apart: each centre is taken where it
    /// is, the distance measures 7 at every zoom; Space goes through what is under the pointer and
    /// Backspace takes back the last pick.
    #[test]
    fn picks_are_exact_at_any_zoom_and_close_points_are_told_apart() {
        let mut h = crate::scenario::Harness::new();
        h.app.run("solid.box", json!({"length": 100, "width": 50, "height": 20})).unwrap();
        for x in [30, 37] {
            h.app.run("solid.cylinder", json!({"diameter": 4, "height": 30, "base": [x, 25, -5], "operation": "cut"})).unwrap();
        }
        h.step(&json!({"start": "drawing.create"})).unwrap();
        h.frames(4);
        set(&mut h, |k| {
            k.orientation = 2; // top
            k.base_scale = 4; // 1:1
        });
        click(&mut h, [150.0, 150.0]);
        h.step(&json!({"key": "Escape"})).unwrap();
        h.frames(2);
        assert_eq!(h.app.session.doc.drawings[0].views.len(), 1, "{:?}", ui(&h).status);
        // At a corner of the plate (100, 175) there are its point and its two edges: Space goes on.
        set(&mut h, |k| k.set_tool(Tool::Dimension));
        h.frames(1);
        let s = screen([100.0, 175.0]);
        h.step(&json!({"move": s})).unwrap();
        h.frames(2);
        assert!(ui(&h).choices >= 3, "{}", ui(&h).choices);
        assert_eq!(ui(&h).hover_kind, "end");
        h.step(&json!({"key": "Space"})).unwrap();
        h.frames(2);
        assert_eq!(ui(&h).hover_kind, "line");
        set(&mut h, |k| k.set_tool(Tool::Select));
        // The plate's middle (50, 25) is at (150, 150): the holes' centres at (130, 150) and (137, 150).
        for (zoom, scrolls) in [("fitted", 0), ("closer", 4), ("much closer", 6)] {
            for _ in 0..scrolls {
                let c = screen([133.5, 150.0]);
                h.step(&json!({"call": "ui.scroll", "params": {"x": c[0], "y": c[1], "delta": 120}})).unwrap();
                h.frames(2);
            }
            set(&mut h, |k| {
                k.set_tool(Tool::Dimension);
                k.dim_kind = 0;
            });
            h.frames(1);
            let before = h.app.session.doc.drawings[0].dims.len();
            click(&mut h, [130.0, 150.0]);
            assert_eq!(ui(&h).dim_picks.len(), 1, "{zoom}: {:?}", ui(&h).status);
            // Backspace takes it back; picked again.
            h.step(&json!({"key": "Backspace"})).unwrap();
            h.frames(2);
            assert!(ui(&h).dim_picks.is_empty(), "{zoom}: Backspace");
            click(&mut h, [130.0, 150.0]);
            click(&mut h, [137.0, 150.0]);
            assert!(ui(&h).dim_ready(), "{zoom}: {:?}", ui(&h).dim_picks);
            click(&mut h, [133.5, 158.0]);
            let d = h.app.session.doc.drawings[0].clone();
            assert_eq!(d.dims.len(), before + 1, "{zoom}: {:?}", ui(&h).status);
            let x = d.dims.last().unwrap();
            assert!(x.refs.iter().all(|r| r.at == "center"), "{zoom}: {:?}", x.refs);
            let drawn = h.app.session.execute("drawing.render", &json!({"drawing": d.id})).unwrap();
            let dim_text = drawn["dims"].as_array().unwrap().last().unwrap()[1].clone();
            let near: Vec<&str> = drawn["prims"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|p| p["type"] == "text" && (p["at"][0].as_f64().unwrap() - dim_text[0].as_f64().unwrap()).abs() < 1e-6)
                .filter_map(|p| p["text"].as_str())
                .collect();
            assert!(near.contains(&"7"), "{zoom}: {near:?}");
        }
    }

    /// With Shift held a projected view goes where it is put, off its parent's line (free).
    #[test]
    fn shift_puts_a_projected_view_anywhere() {
        let mut h = editor();
        set(&mut h, |k| k.base_scale = 4);
        click(&mut h, [130.0, 190.0]);
        let s = screen([290.0, 165.0]);
        h.step(&json!({"move": s, "shift": true})).unwrap();
        h.frames(2);
        h.step(&json!({"click": s, "shift": true})).unwrap();
        h.frames(2);
        let d = h.app.session.doc.drawings[0].clone();
        let v = d.views.last().unwrap();
        assert!(
            v.free && dist(v.at, [290.0, 165.0]) < 1.0 && matches!(&v.kind, ViewKind::Projected { side, .. } if side == "right"),
            "{v:?} {:?}",
            ui(&h).status
        );
    }
}
