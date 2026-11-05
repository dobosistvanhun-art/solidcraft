//! The nesting editor: while it is open the viewport shows the job's material sheets side by
//! side with every placed part's outline, and the panel holds the parts and their counts, the
//! sheet, the gaps, the quality, the layouts found and what they use. Everything it changes goes
//! through `nest.*` commands.
//!
//! On the sheets a click picks a part and a drag moves it (onto another sheet too); while it
//! moves its outline turns red where it would come too close to another part or past the
//! margin. With a part picked, R turns it 15° (Shift+R back), L locks or unlocks it and Delete
//! takes it out of the layout. Auto Nest keeps the locked parts where they are.

use std::sync::{Arc, Mutex, Weak};

use egui::{Align2, Color32, FontId, Pos2, Rect, Stroke, pos2, vec2};
use serde_json::{Value, json};
use solidcraft_engine::doc::{Document, FeatureKind};

use crate::SolveApp;
use crate::dialogs::{Dialog, Kind};
use crate::theme::Tokens;
use crate::ui_kit;

/// The qualities, as the command names them, and their labels.
const QUALITIES: [&str; 3] = ["fast", "normal", "precise"];
const QUALITY_LABELS: [&str; 3] = ["Fast", "Normal", "Precise"];
/// How far R turns a picked part (degrees).
const TURN_STEP: f64 = 15.0;

/// A part being dragged: its placement, where it was grabbed against its centre (mm), and where
/// its centre is now (its sheet and the point).
#[derive(Clone, Debug)]
pub struct Drag {
    placement: usize,
    grab: [f64; 2],
    sheet: usize,
    at: [f64; 2],
    /// The moved part comes too close to another or past the margin.
    clash: bool,
}

/// A layout run off the window thread: where its reply lands.
type Pending = Arc<Mutex<Option<Result<Value, String>>>>;

/// The editor's state.
#[derive(Clone, Debug, Default)]
pub struct NestUi {
    pub selected: Option<usize>,
    drag: Option<Drag>,
    pub status: Option<String>,
    running: Option<Pending>,
    /// `nest.info` for the design it was read from.
    info: Option<(Weak<Document>, Value)>,
    /// What the editor was opened for from the ribbon: run the layout or save it.
    pub start: Option<&'static str>,
    /// Import DXF makes a part of each outer contour.
    pub split: bool,
}

/// The editor for a NESTING tab command.
pub fn start(_app: &SolveApp, id: &str) -> Option<(Kind, Vec<crate::selection::SelInput>)> {
    let start = match id {
        "nest.setup" => None,
        "nest.run" => Some("run"),
        "nest.export_dxf" => Some("export"),
        _ => return None,
    };
    Some((Kind::Nest(Box::new(NestUi { start, ..NestUi::default() })), vec![]))
}

/// The job as `nest.info` reads it (with outlines), read again when the design changes.
fn info(app: &mut SolveApp, k: &mut NestUi) -> Option<Value> {
    app.session.doc.nesting.as_ref()?;
    if let Some((doc, v)) = &k.info
        && doc.upgrade().is_some_and(|d| Arc::ptr_eq(&d, &app.session.doc))
    {
        return Some(v.clone());
    }
    let v = app.session.execute("nest.info", &json!({})).ok()?;
    k.info = Some((Arc::downgrade(&app.session.doc), v.clone()));
    Some(v)
}

fn run(app: &mut SolveApp, k: &mut NestUi, cmd: &str, p: Value) -> Option<Value> {
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

/// A part the job can take: a sketch (choosable once it has a closed contour) or a sheet metal
/// body.
struct Candidate {
    name: String,
    key: Value,
    closed: bool,
}

fn candidates(app: &SolveApp) -> Vec<Candidate> {
    let st = app.session.model.state();
    let mut out: Vec<Candidate> = app
        .session
        .doc
        .features
        .iter()
        .filter(|f| matches!(f.kind, FeatureKind::Sketch { .. }))
        .map(|f| Candidate { name: f.name.clone(), key: json!({"sketch": f.id}), closed: st.sketch(f.id).is_some_and(|ss| !ss.profiles.is_empty()) })
        .collect();
    out.extend(st.sheets.iter().map(|sh| Candidate { name: sh.body.clone(), key: json!({"body": sh.body}), closed: true }));
    out
}

fn same_part(a: &Value, b: &Value) -> bool {
    (a["sketch"].is_u64() && a["sketch"] == b["sketch"]) || (a["body"].is_string() && a["body"] == b["body"])
}

/// A length field that sends its value once it is set (typed, or the drag let go).
fn length_row(ui: &mut egui::Ui, label: &str, value: &mut f64) -> bool {
    let mut done = false;
    ui_kit::row(ui, label, |ui| {
        let r = ui_kit::length_field(ui, value, "mm", 1);
        done = r.drag_stopped() || (r.changed() && !r.dragged());
    });
    done
}

/// Takes the reply of a layout run off the thread, when it has come.
fn poll(app: &mut SolveApp, k: &mut NestUi) {
    let Some(pending) = &k.running else { return };
    let reply = pending.lock().ok().and_then(|mut slot| slot.take());
    let Some(reply) = reply else { return };
    k.running = None;
    match reply {
        Ok(v) => {
            run(app, k, "nest.run", json!({"apply": v["layouts"]}));
        }
        Err(e) => k.status = Some(e),
    }
}

/// Auto Nest: off the window thread when the app runs commands there, else at once.
fn auto_nest(app: &mut SolveApp, k: &mut NestUi) {
    #[cfg(not(target_arch = "wasm32"))]
    if app.apply_off_thread.is_some() {
        let pending: Pending = Arc::default();
        let (ahead, out) = (app.session.ahead(), pending.clone());
        let spawned = std::thread::Builder::new().name("nest".into()).spawn(move || {
            let reply = ahead.replies(&[("nest.run".to_string(), json!({}))]).into_iter().next().map(|r| r.map_err(|e| e.to_string()));
            if let Ok(mut slot) = out.lock() {
                *slot = Some(reply.unwrap_or_else(|| Err("the layout gave no answer".into())));
            }
        });
        if spawned.is_ok() {
            k.running = Some(pending);
            return;
        }
    }
    run(app, k, "nest.run", json!({}));
}

fn save(app: &mut SolveApp, sheet: usize, all: bool) {
    let name = format!("{}-nest{}.dxf", app.session.doc.name, if all { String::new() } else { format!("-{}", sheet + 1) });
    let p = if all { json!({"all": true}) } else { json!({"sheet": sheet}) };
    app.pick_save_command(&name, &["dxf"], "nest.export_dxf", p);
}

/// The panel's rows.
pub fn rows(app: &mut SolveApp, ui: &mut egui::Ui, k: &mut NestUi) -> bool {
    poll(app, k);
    if k.running.is_some() {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
    }
    let v = info(app, k);
    // Opened for Auto Nest or Save from the ribbon: that, once.
    match k.start.take() {
        Some("run") if v.is_some() => auto_nest(app, k),
        Some("export") if v.as_ref().is_some_and(|v| v["stats"]["sheets"].as_u64().unwrap_or(0) > 0) => save(app, 0, true),
        _ => {}
    }
    ui_kit::section(ui, "nest_parts", "Parts", true, |ui| {
        // The job's part list: one row a part, its count, grain and turn, and a cross to take it out.
        let parts = v.as_ref().and_then(|v| v["parts"].as_array().cloned()).unwrap_or_default();
        let mut edit: Option<Value> = None;
        for (i, p) in parts.iter().enumerate() {
            let name = p["name"].as_str().unwrap_or("").to_string();
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(&name).strong());
                if let Some(e) = p["error"].as_str() {
                    ui.label(egui::RichText::new("⚠").color(Color32::from_rgb(210, 60, 40))).on_hover_text(e);
                }
                if ui_kit::button(ui, "✕", ui_kit::Button::Small).on_hover_text(crate::i18n::tr_label("Remove")).clicked() {
                    edit = Some(json!({"index": i, "remove": true}));
                }
            });
            ui.indent(("nest_part", i), |ui| {
                let mut count = p["count"].as_u64().unwrap_or(1) as u32;
                ui_kit::row(ui, "Count", |ui| {
                    let r = ui_kit::number_field(ui, &mut count, 1..=5000, 0.2, "");
                    if r.drag_stopped() || (r.changed() && !r.dragged()) {
                        edit = Some(json!({"index": i, "count": count}));
                    }
                });
                let (mut grain, mut fixed) = (p["grain"].as_bool().unwrap_or(false), p["fixed_angle"].as_bool().unwrap_or(false));
                ui.horizontal(|ui| {
                    if ui_kit::checkbox(ui, "Grain", &mut grain).changed() {
                        edit = Some(json!({"index": i, "grain": grain}));
                    }
                    if ui_kit::checkbox(ui, "Fixed angle", &mut fixed).changed() {
                        edit = Some(json!({"index": i, "fixed_angle": fixed}));
                    }
                });
                // Its cutting colours, over its contours' own (LightBurn layers).
                for (key, row, salt) in [("outer_colour", "Edge colour", "nest_outer"), ("inner_colour", "Hole colour", "nest_inner")] {
                    ui_kit::row(ui, row, |ui| {
                        if let Some(c) = crate::dialogs_cut_colour::menu(ui, (salt, i), p[key].as_str()) {
                            let mut e = json!({"index": i});
                            e[key] = c.map_or(Value::Null, |c| json!(c));
                            edit = Some(e);
                        }
                    });
                }
            });
        }
        if let Some(e) = edit {
            run(app, k, "nest.setup", json!({"edit": e}));
        }
        // More parts: the design's sketches and sheet metal bodies, or a DXF file.
        let all = candidates(app);
        let mut add: Option<Value> = None;
        ui.horizontal(|ui| {
            ui_kit::dropdown_menu(ui, "nest_add", crate::i18n::tr_label("Add part"), |ui| {
                if all.is_empty() {
                    ui.label(crate::i18n::tr_label("Draw a part's closed contour in a sketch first"));
                }
                for c in &all {
                    let taken = parts.iter().any(|p| same_part(p, &c.key));
                    if !c.closed {
                        ui.add_enabled(false, egui::Button::new(format!("{} — {}", c.name, crate::i18n::tr_label("no closed contour"))));
                    } else if !taken && ui_kit::button(ui, c.name.as_str(), ui_kit::Button::Small).clicked() {
                        let mut p = c.key.clone();
                        p["count"] = json!(1);
                        add = Some(p);
                    }
                }
            });
            if ui_kit::button(ui, crate::i18n::tr_label("Import DXF…"), ui_kit::Button::Normal).clicked() {
                app.pick_open_command("nest.import_dxf", json!({"split": k.split}), &["dxf"]);
            }
        });
        ui_kit::checkbox(ui, "Each contour a part", &mut k.split);
        if let Some(p) = add {
            run(app, k, "nest.setup", json!({"add": [p]}));
        }
    });
    let v = info(app, k);
    ui_kit::section(ui, "nest_sheet", "Sheet", true, |ui| {
        let sheet = v.as_ref().map(|v| v["sheet"].clone()).unwrap_or_else(|| json!({"width": 1220.0, "height": 610.0, "margin": 5.0}));
        let (mut w, mut h, mut m) =
            (sheet["width"].as_f64().unwrap_or(1220.0), sheet["height"].as_f64().unwrap_or(610.0), sheet["margin"].as_f64().unwrap_or(5.0));
        let (mut gap, mut kerf) =
            (v.as_ref().and_then(|v| v["gap"].as_f64()).unwrap_or(3.0), v.as_ref().and_then(|v| v["kerf"].as_f64()).unwrap_or(0.0));
        let mut send = false;
        send |= length_row(ui, "Width", &mut w);
        send |= length_row(ui, "Height", &mut h);
        send |= length_row(ui, "Margin", &mut m);
        let gap_set = length_row(ui, "Gap", &mut gap);
        let kerf_set = length_row(ui, "Cut width", &mut kerf);
        let mut q = v.as_ref().and_then(|v| v["quality"].as_str()).and_then(|q| QUALITIES.iter().position(|x| *x == q)).unwrap_or(1);
        let mut quality = false;
        ui_kit::row(ui, "Quality", |ui| quality = ui_kit::dropdown_picked(ui, "nest_quality", &mut q, &QUALITY_LABELS));
        if send || gap_set || kerf_set || quality {
            let mut p =
                json!({"sheet": {"width": w.max(1.0), "height": h.max(1.0), "margin": m.max(0.0)}, "gap": gap.max(0.0), "kerf": kerf.max(0.0)});
            if let Some(q) = QUALITIES.get(q) {
                p["quality"] = json!(q);
            }
            run(app, k, "nest.setup", p);
        }
    });
    let v = info(app, k);
    let busy = k.running.is_some();
    ui.horizontal(|ui| {
        let ready = v.as_ref().is_some_and(|v| v["parts"].as_array().is_some_and(|p| !p.is_empty()));
        if busy {
            ui.label(crate::i18n::tr_label("Working…"));
        } else if ui_kit::button(ui, crate::i18n::tr_label("Auto Nest"), ui_kit::Button::Normal).clicked() && ready {
            auto_nest(app, k);
        }
    });
    let Some(v) = v else {
        if let Some(s) = &k.status {
            ui_kit::text(ui, s, ui_kit::Text::Report("nest_status", true));
        }
        return false;
    };
    // The picked part.
    if let Some(i) = k.selected.filter(|i| v["placements"].get(*i).is_some()) {
        let pl = v["placements"][i].clone();
        ui_kit::section(ui, "nest_selected", "Selected", true, |ui| {
            let name = v["parts"][pl["part"].as_u64().unwrap_or(0) as usize]["name"].as_str().unwrap_or("").to_string();
            ui_kit::row(ui, "Part", |ui| ui.label(name));
            let mut angle = pl["angle"].as_f64().unwrap_or(0.0);
            let mut turn = false;
            ui_kit::row(ui, "Angle", |ui| {
                let r = ui_kit::number_field(ui, &mut angle, -360.0..=720.0, 0.5, "°");
                turn = r.drag_stopped() || (r.changed() && !r.dragged());
            });
            if turn {
                run(app, k, "nest.edit", json!({"placement": i, "angle": angle}));
            }
            let mut locked = pl["locked"].as_bool().unwrap_or(false);
            ui.horizontal(|ui| {
                if ui_kit::checkbox(ui, "Lock", &mut locked).changed() {
                    run(app, k, "nest.edit", json!({"placement": i, "locked": locked}));
                }
                if ui_kit::button(ui, crate::i18n::tr_label("Remove"), ui_kit::Button::Small).clicked()
                    && run(app, k, "nest.edit", json!({"placement": i, "remove": true})).is_some()
                {
                    k.selected = None;
                }
            });
        });
    }
    // What the layout uses, and the other layouts found.
    let stats = v["stats"].clone();
    ui_kit::section(ui, "nest_result", "Layout", true, |ui| {
        let f = |key: &str| stats[key].as_f64().unwrap_or(0.0);
        ui_kit::row(ui, "Sheets", |ui| ui.label(format!("{}", stats["sheets"].as_u64().unwrap_or(0))));
        ui_kit::row(ui, "Utilisation", |ui| ui.label(format!("{:.1} %", f("utilisation_percent"))));
        ui_kit::row(ui, "Parts area", |ui| ui.label(format!("{:.0} mm²", f("parts_area_mm2"))));
        ui_kit::row(ui, "Waste", |ui| ui.label(format!("{:.0} mm²", f("waste_mm2"))));
        ui_kit::row(ui, "Cut length", |ui| ui.label(format!("{:.0} mm", f("cut_length_mm"))));
        let (hits, off, left) = (
            v["collisions"].as_array().map_or(0, Vec::len),
            v["outside"].as_array().map_or(0, Vec::len),
            v["unplaced"].as_array().map_or(0, Vec::len),
        );
        if hits > 0 {
            ui_kit::text(ui, &format!("{}: {hits}", crate::i18n::tr_label("Too close")), ui_kit::Text::Report("nest_hits", true));
        }
        if off > 0 {
            ui_kit::text(ui, &format!("{}: {off}", crate::i18n::tr_label("Past the margin")), ui_kit::Text::Report("nest_off", true));
        }
        if left > 0 {
            ui_kit::text(ui, &format!("{}: {left}", crate::i18n::tr_label("Not placed")), ui_kit::Text::Report("nest_left", true));
        }
        let variants = v["variants"].as_array().cloned().unwrap_or_default();
        if variants.len() > 1 {
            for (n, s) in variants.iter().enumerate() {
                let text = format!(
                    "{} {}: {} × {:.1} %",
                    crate::i18n::tr_label("Layout"),
                    n + 1,
                    s["sheets"].as_u64().unwrap_or(0),
                    s["utilisation_percent"].as_f64().unwrap_or(0.0)
                );
                if ui_kit::button(ui, text, ui_kit::Button::Small).clicked() {
                    run(app, k, "nest.run", json!({"variant": n}));
                }
            }
        }
        let sheets = stats["sheets"].as_u64().unwrap_or(0) as usize;
        if sheets > 0 {
            ui.horizontal(|ui| {
                let at = k.selected.and_then(|i| v["placements"][i]["sheet"].as_u64()).unwrap_or(0) as usize;
                if ui_kit::button(ui, format!("{}…", crate::i18n::tr_label("DXF")), ui_kit::Button::Normal).clicked() {
                    save(app, at, false);
                }
                if sheets > 1 && ui_kit::button(ui, crate::i18n::tr_label("All sheets"), ui_kit::Button::Normal).clicked() {
                    save(app, 0, true);
                }
            });
        }
    });
    if let Some(s) = &k.status {
        ui_kit::text(ui, s, ui_kit::Text::Report("nest_status", true));
    }
    false
}

/// The gap between two sheets as shown side by side (a twentieth of a sheet's width).
fn pitch(w: f64) -> f64 {
    w * 1.05
}

fn points(v: &Value) -> Vec<[f64; 2]> {
    v.as_array().into_iter().flatten().filter_map(|p| Some([p.get(0)?.as_f64()?, p.get(1)?.as_f64()?])).collect()
}

fn inside(p: [f64; 2], poly: &[[f64; 2]]) -> bool {
    let mut inside = false;
    for (i, a) in poly.iter().enumerate() {
        let Some(b) = poly.get((i + 1) % poly.len()) else { continue };
        if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]) {
            inside = !inside;
        }
    }
    inside
}

/// Paint the sheets over the viewport and handle picks, drags and keys on them. True when the
/// dialog is the nesting editor.
pub fn overlay(app: &mut SolveApp, painter: &egui::Painter, rect: Rect, d: &mut Dialog) -> bool {
    let Kind::Nest(k) = &mut d.kind else { return false };
    let t = Tokens::get();
    painter.rect_filled(rect, 0.0, t.panel);
    let Some(v) = info(app, k) else {
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            crate::i18n::tr_label("Add the parts to cut in the panel"),
            FontId::proportional(t.font_body),
            t.text_dim,
        );
        return true;
    };
    let (w, h, margin) = (
        v["sheet"]["width"].as_f64().unwrap_or(1220.0),
        v["sheet"]["height"].as_f64().unwrap_or(610.0),
        v["sheet"]["margin"].as_f64().unwrap_or(0.0),
    );
    let sheets = (v["stats"]["sheets"].as_u64().unwrap_or(0) as usize).max(1);
    let extent = pitch(w) * (sheets as f64 - 1.0) + w;
    let panel_left = painter
        .ctx()
        .memory(|m| m.area_rect(egui::Id::new(("sc_dialog", "NESTING"))))
        .map(|panel| panel.left())
        .unwrap_or(rect.right() - crate::frame::DIALOG_MAX_W);
    let area = Rect::from_min_max(rect.min + vec2(20.0, 20.0), pos2(panel_left.min(rect.right()) - 20.0, rect.bottom() - 20.0));
    if !app.flat_view.fitted && area.width() > 10.0 && area.height() > 10.0 {
        app.flat_view.fit(rect, area, (0.0, 0.0), (extent, h));
    }
    let cam = app.flat_view.cam;
    let scale = f64::from(rect.height()) / (2.0 * cam.half_height());
    let ox = f64::from(rect.center().x) - cam.target.x * scale;
    let oy = f64::from(rect.center().y) + cam.target.y * scale;
    let to = |p: [f64; 2]| -> Pos2 { pos2((ox + p[0] * scale) as f32, (oy - p[1] * scale) as f32) };
    let from = |p: Pos2| -> [f64; 2] { [(f64::from(p.x) - ox) / scale, (oy - f64::from(p.y)) / scale] };
    // The sheets: light wood, the margin dashed.
    let wood = Color32::from_rgb(232, 214, 182);
    for s in 0..sheets {
        let x0 = pitch(w) * s as f64;
        let r = Rect::from_two_pos(to([x0, 0.0]), to([x0 + w, h]));
        painter.rect_filled(r.translate(vec2(3.0, 3.0)), 0.0, Color32::from_black_alpha(50));
        painter.rect_filled(r, 0.0, wood);
        if margin > 0.0 {
            let m = Rect::from_two_pos(to([x0 + margin, margin]), to([x0 + w - margin, h - margin]));
            for (a, b) in [
                (m.left_top(), m.right_top()),
                (m.right_top(), m.right_bottom()),
                (m.right_bottom(), m.left_bottom()),
                (m.left_bottom(), m.left_top()),
            ] {
                painter.extend(egui::Shape::dashed_line(&[a, b], Stroke::new(1.0, Color32::from_rgb(160, 130, 90)), 6.0, 4.0));
            }
        }
        painter.text(to([x0, h]) + vec2(0.0, -4.0), Align2::LEFT_BOTTOM, format!("{}", s + 1), FontId::proportional(t.font_small), t.text_dim);
    }
    let bad: Vec<u64> = v["collisions"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|pair| pair.as_array().cloned().unwrap_or_default())
        .filter_map(|x| x.as_u64())
        .chain(v["outside"].as_array().into_iter().flatten().filter_map(Value::as_u64))
        .collect();
    let ctx = painter.ctx().clone();
    let pointer = ctx.input(|i| i.pointer.latest_pos());
    let over = pointer.is_some_and(|p| rect.contains(p) && ctx.layer_id_at(p).is_none_or(|l| l.order == egui::Order::Background));
    let cursor = pointer.filter(|_| over).map(from);
    let (pressed, released, down) = ctx.input(|i| (i.pointer.primary_pressed(), i.pointer.primary_released(), i.pointer.primary_down()));
    let placements = v["placements"].as_array().cloned().unwrap_or_default();
    // Each part's outlines on the screen's layout (its sheet's offset added).
    let polys_of = |pl: &Value, key: &str| -> Vec<Vec<[f64; 2]>> {
        let dx = pitch(w) * pl["sheet"].as_u64().unwrap_or(0) as f64;
        pl[key].as_array().into_iter().flatten().map(|o| points(o).into_iter().map(|p| [p[0] + dx, p[1]]).collect()).collect()
    };
    let shown = |pl: &Value| polys_of(pl, "outlines");
    for (i, pl) in placements.iter().enumerate() {
        let moving = k.drag.as_ref().filter(|dg| dg.placement == i);
        let mut polys = shown(pl);
        // Its holes (inner contours), drawn thinner, go with it.
        let outer = polys.len();
        polys.extend(polys_of(pl, "holes"));
        if let Some(dg) = moving {
            let (cx, cy) = (pl["x"].as_f64().unwrap_or(0.0) + pitch(w) * pl["sheet"].as_u64().unwrap_or(0) as f64, pl["y"].as_f64().unwrap_or(0.0));
            let to_x = dg.at[0] + pitch(w) * dg.sheet as f64;
            for poly in &mut polys {
                for p in poly.iter_mut() {
                    *p = [p[0] - cx + to_x, p[1] - cy + dg.at[1]];
                }
            }
        }
        let clash = moving.map_or(bad.contains(&(i as u64)), |dg| dg.clash);
        let colour = if clash {
            Color32::from_rgb(210, 40, 40)
        } else if k.selected == Some(i) {
            t.accent
        } else if pl["locked"].as_bool().unwrap_or(false) {
            Color32::from_rgb(90, 90, 90)
        } else {
            Color32::from_rgb(40, 60, 110)
        };
        let width = if k.selected == Some(i) { 2.2 } else { 1.4 };
        for (n, poly) in polys.iter().enumerate() {
            let pts: Vec<Pos2> = poly.iter().map(|p| to(*p)).collect();
            if pts.len() >= 3 {
                let line = if n < outer { width } else { width * 0.7 };
                painter.add(egui::Shape::closed_line(pts, Stroke::new(line, colour)));
            }
        }
        if pl["locked"].as_bool().unwrap_or(false) {
            let c = to([pl["x"].as_f64().unwrap_or(0.0) + pitch(w) * pl["sheet"].as_u64().unwrap_or(0) as f64, pl["y"].as_f64().unwrap_or(0.0)]);
            painter.text(c, Align2::CENTER_CENTER, "🔒", FontId::proportional(t.font_small), colour);
        }
    }
    // A pick, the start of a drag.
    if pressed && let Some(c) = cursor {
        let hit = placements.iter().enumerate().rev().find(|(_, pl)| shown(pl).iter().any(|poly| inside(c, poly))).map(|(i, _)| i);
        k.selected = hit;
        k.drag = hit.and_then(|i| {
            let pl = placements.get(i)?;
            let s = pl["sheet"].as_u64().unwrap_or(0) as usize;
            let centre = [pl["x"].as_f64()?, pl["y"].as_f64()?];
            let grab = [centre[0] - (c[0] - pitch(w) * s as f64), centre[1] - c[1]];
            Some(Drag { placement: i, grab, sheet: s, at: centre, clash: false })
        });
    }
    // Moving: the sheet under the pointer, the part's centre with it, checked on a scratch copy.
    if down && let (Some(c), Some(dg)) = (cursor, k.drag.as_mut()) {
        let s = (c[0] / pitch(w)).floor().clamp(0.0, sheets as f64) as usize;
        let at = [c[0] - pitch(w) * s as f64 + dg.grab[0], c[1] + dg.grab[1]];
        if at != dg.at || s != dg.sheet {
            dg.at = at;
            dg.sheet = s;
            let mut probe = app.session.scratch();
            dg.clash = probe
                .execute("nest.edit", &json!({"placement": dg.placement, "x": at[0], "y": at[1], "sheet": s}))
                .map(|r| r["collides"].as_array().is_some_and(|c| !c.is_empty()) || r["outside"].as_bool().unwrap_or(false))
                .unwrap_or(true);
        }
    }
    if released && let Some(dg) = k.drag.take() {
        let moved = placements.get(dg.placement).is_some_and(|pl| {
            pl["sheet"].as_u64() != Some(dg.sheet as u64)
                || (pl["x"].as_f64().unwrap_or(0.0) - dg.at[0]).abs() > 1e-6
                || (pl["y"].as_f64().unwrap_or(0.0) - dg.at[1]).abs() > 1e-6
        });
        if moved {
            run(app, k, "nest.edit", json!({"placement": dg.placement, "x": dg.at[0], "y": dg.at[1], "sheet": dg.sheet}));
        }
    }
    // Keys on the picked part.
    if let Some(i) = k.selected
        && !ctx.egui_wants_keyboard_input()
        && let Some(pl) = placements.get(i)
    {
        let (r, shift, l, del) =
            ctx.input(|i| (i.key_pressed(egui::Key::R), i.modifiers.shift, i.key_pressed(egui::Key::L), i.key_pressed(egui::Key::Delete)));
        let angle = pl["angle"].as_f64().unwrap_or(0.0);
        if r {
            run(app, k, "nest.edit", json!({"placement": i, "angle": angle + if shift { -TURN_STEP } else { TURN_STEP }}));
        } else if l {
            run(app, k, "nest.edit", json!({"placement": i, "locked": !pl["locked"].as_bool().unwrap_or(false)}));
        } else if del && run(app, k, "nest.edit", json!({"placement": i, "remove": true})).is_some() {
            k.selected = None;
        }
    }
    if k.running.is_some() {
        painter.text(
            rect.left_top() + vec2(24.0, 24.0),
            Align2::LEFT_TOP,
            crate::i18n::tr_label("Working…"),
            FontId::proportional(t.font_body),
            t.text,
        );
    }
    true
}

#[cfg(test)]
#[path = "dialogs_nest_tests.rs"]
mod tests;
