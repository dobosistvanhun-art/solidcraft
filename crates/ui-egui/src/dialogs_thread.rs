//! Metric thread choices use the document's sizing table and the existing designation
//! parameter. Browsing choices does not edit the model or freeze an automatic feature.

use solidcraft_engine::doc::{Document, metric_thread_sizes, parse_metric_thread};

use crate::dialogs::{combo, form_row};
use crate::i18n::tr;
use crate::ui_kit::text_row;

/// Common pitch presets, not an exhaustive catalogue of standard diameter/pitch pairs.
/// The coarse and current pitches are included separately; custom pairs remain editable.
const PITCHES: &[f64] = &[0.2, 0.25, 0.35, 0.5, 0.75, 1.0, 1.25, 1.5, 2.0, 3.0, 4.0];

/// Generated metric designations omit only the shared catalogue's coarse pitch.
/// Authored designation text is never rewritten just by displaying a dialog.
pub(crate) fn designation(diameter: f64, pitch: f64) -> String {
    let coarse = metric_thread_sizes().iter().any(|(d, p)| (d - diameter).abs() < 1e-9 && (p - pitch).abs() < 1e-9);
    if coarse { format!("M{diameter}") } else { format!("M{diameter}x{pitch}") }
}

/// Unsaved numeric inputs for a custom metric designation. The document still stores only
/// the existing designation; an invalid draft must not apply its last valid value.
#[derive(Clone, Debug, PartialEq)]
pub struct HoleThreadDraft {
    source: String,
    diameter: String,
    pitch: String,
    pub(crate) error: Option<String>,
}

impl HoleThreadDraft {
    fn new(source: &str) -> Self {
        let (diameter, pitch) = match parse_metric_thread(source) {
            Some((d, p)) => (format!("{d} mm"), format!("{p} mm")),
            None => (String::new(), String::new()),
        };
        Self { source: source.into(), diameter, pitch, error: None }
    }

    fn update(&mut self, doc: &Document, authored: &mut String) {
        let candidate = || -> Result<String, String> {
            let d = doc.eval(&self.diameter, solidcraft_engine::doc::expr::Kind::Length).map_err(|e| format!("nominal diameter: {e}"))?;
            let p = doc.eval(&self.pitch, solidcraft_engine::doc::expr::Kind::Length).map_err(|e| format!("pitch: {e}"))?;
            let text = designation(d, p);
            parse_metric_thread(&text).ok_or("use a positive nominal diameter and a pitch smaller than it")?;
            Ok(text)
        };
        match candidate() {
            Ok(text) => {
                *authored = text;
                self.source = authored.clone();
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
    }
}

/// Tapped holes use the existing metric catalogue; the engine derives the tap drill.
/// Custom remains a dialog mode even when its current values match a catalogue size.
pub(crate) fn hole_rows(ui: &mut egui::Ui, doc: &Document, designation: &mut String, custom: &mut Option<HoleThreadDraft>) -> bool {
    let designation_id = ui.make_persistent_id("hole_thread_designation");
    form_row(ui, "Profile", |ui| {
        ui.label(tr("thread.iso_metric"));
    });

    let sizes = metric_thread_sizes();
    let current = parse_metric_thread(designation);
    let catalogue = current.and_then(|pair| sizes.iter().position(|s| *s == pair));
    if custom.as_ref().is_some_and(|draft| draft.source != *designation) || (custom.is_none() && catalogue.is_none()) {
        *custom = Some(HoleThreadDraft::new(designation));
    }
    form_row(ui, "Size", |ui| {
        let mut selected = if custom.is_some() { sizes.len() } else { catalogue.unwrap_or(sizes.len()) };
        let before = selected;
        let mut labels: Vec<String> = sizes.iter().map(|(d, p)| self::designation(*d, *p)).collect();
        labels.push("Custom".to_string());
        let labels: Vec<&str> = labels.iter().map(String::as_str).collect();
        combo(ui, "hole_thread_catalogue", &labels, &mut selected);
        if selected != before {
            if let Some((d, p)) = sizes.get(selected) {
                *designation = self::designation(*d, *p);
                *custom = None;
            } else {
                *custom = Some(HoleThreadDraft::new(designation));
            }
        }
    });

    let mut enter = false;
    if let Some(draft) = custom {
        let (diameter_enter, diameter_changed) = text_row(ui, "Nominal diameter", &mut draft.diameter, "");
        let (pitch_enter, pitch_changed) = text_row(ui, "Pitch", &mut draft.pitch, "");
        if diameter_changed || pitch_changed {
            draft.update(doc, designation);
        }
        enter |= diameter_enter || pitch_enter;
    }

    let response = form_row(ui, "Designation", |ui| crate::ui_kit::cell_editor(ui, designation_id, designation, "M5 or M5x0.5", false));
    if response.changed() && custom.is_some() {
        *custom = Some(HoleThreadDraft::new(designation));
    }
    form_row(ui, "Tap drill diameter", |ui| {
        match parse_metric_thread(designation).filter(|_| custom.as_ref().is_none_or(|draft| draft.error.is_none())) {
            Some((d, p)) => {
                ui.label(format!("{:.3} mm", d - p));
            }
            None => {
                ui.label(tr("thread.select_size"));
            }
        }
    });
    enter || (response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
}

/// Returns (Enter pressed, automatic sizing requested).
pub(crate) fn rows(ui: &mut egui::Ui, designation: &mut String, suggested: &mut Option<String>, modeled: &mut bool, full: &mut bool) -> (bool, bool) {
    // The thread standard: ISO metric keeps its diameter and pitch pickers below, the others
    // pick from their sizes (Custom is drawn: shape, diameter, pitch, angles, depth).
    use solidcraft_engine::doc::thread_spec;
    // The profile picked stays while a designation is half typed (unreadable): otherwise the
    // dialog would switch back to ISO metric under the cursor at every keystroke.
    let memo = egui::Id::new("thread_profile_choice");
    let remembered: Option<usize> = ui.ctx().data(|d| d.get_temp(memo));
    // A metric designation (M8, M8x1, M5×0.5) is ISO metric whatever was picked before.
    let metric = parse_metric_thread(designation).is_some();
    let profile = match (
        if metric { Some("ISO metric") } else { thread_spec::profile_of(designation) },
        remembered.and_then(|i| thread_spec::PROFILES.get(i).copied()),
    ) {
        (Some(p), _) => p,
        (None, Some(p)) if !designation.trim().is_empty() => p,
        _ => "ISO metric",
    };
    let mut selected = thread_spec::PROFILES.iter().position(|p| *p == profile).unwrap_or(0);
    let before_profile = selected;
    form_row(ui, "Profile", |ui| combo(ui, "thread_profile", thread_spec::PROFILES, &mut selected));
    if selected != before_profile
        && let Some(p) = thread_spec::PROFILES.get(selected)
    {
        *designation = match *p {
            "Custom" => "20x2.5 60°".to_string(),
            "ISO metric" => String::new(),
            other => thread_spec::sizes(other).into_iter().nth(4).or_else(|| thread_spec::sizes(other).into_iter().next()).unwrap_or_default(),
        };
        *suggested = None;
    }
    ui.ctx().data_mut(|d| d.insert_temp(memo, selected));
    let profile = thread_spec::PROFILES.get(selected).copied().unwrap_or("ISO metric");
    if profile != "ISO metric" {
        crate::ui_kit::section(ui, "thread_options", "Options", true, |ui| {
            crate::ui_kit::checkbox_grid(ui, &mut [("Modeled", modeled), ("Full Length", full)], 2);
        });
        if profile == "Custom" {
            custom_rows(ui, designation, suggested);
        } else {
            let sizes = thread_spec::sizes(profile);
            let labels: Vec<&str> = sizes.iter().map(String::as_str).collect();
            let shown = thread_spec::parse(designation).map(|s| s.designation).unwrap_or_default();
            let mut at = sizes.iter().position(|d| *d == shown || thread_spec::parse(d).map(|s| s.designation) == Some(shown.clone())).unwrap_or(0);
            let before = at;
            form_row(ui, "Size", |ui| combo(ui, "thread_family_size", &labels, &mut at));
            if at != before
                && let Some(d) = sizes.get(at)
            {
                *designation = d.clone();
                *suggested = None;
            }
        }
        let (enter, changed) = text_row(ui, "Designation", designation, tr("e.g. Tr16x4, G1/2, 20x2.5 60°"));
        if let Some(at) = crate::scenario::handle_at("dialog:Designation") {
            crate::scenario::publish_handle("dialog:thread_size", at);
        }
        if changed {
            *suggested = None;
        }
        return (enter, false);
    }

    let before = designation.trim().is_empty() || suggested.as_deref() == Some(designation.as_str());
    let mut automatic = before;
    crate::ui_kit::section(ui, "thread_options", "Options", true, |ui| {
        crate::ui_kit::checkbox_grid(ui, &mut [("Modeled", modeled), ("Full Length", full)], 2);
        ui.add_enabled_ui(!designation.trim().is_empty(), |ui| crate::ui_kit::checkbox_grid(ui, &mut [("Automatic size", &mut automatic)], 2));
    });
    let reset = automatic && !before;
    if automatic != before {
        *suggested = None;
        if automatic {
            designation.clear();
        }
    }

    let current = parse_metric_thread(designation);
    let mut diameters: Vec<f64> = metric_thread_sizes().iter().map(|(d, _)| *d).collect();
    if let Some((d, _)) = current
        && !diameters.contains(&d)
    {
        diameters.push(d);
        diameters.sort_by(f64::total_cmp);
    }
    form_row(ui, "Size", |ui| {
        let labels: Vec<String> = diameters.iter().map(|d| format!("{d} mm")).collect();
        let mut selected = current.and_then(|(d, _)| diameters.iter().position(|n| *n == d)).unwrap_or(diameters.len());
        let before = selected;
        let mut labels: Vec<&str> = labels.iter().map(String::as_str).collect();
        if current.is_none() {
            labels.push("Select size");
        }
        combo(ui, "thread_diameter", &labels, &mut selected);
        // A new diameter starts with its coarse pitch. The current custom diameter, if any,
        // is already selected and retains its explicit pitch.
        if selected != before
            && let Some(d) = diameters.get(selected)
            && let Some((_, coarse)) = metric_thread_sizes().iter().find(|(n, _)| n == d)
        {
            *designation = self::designation(*d, *coarse);
            *suggested = None;
        }
    });

    let current = parse_metric_thread(designation);
    form_row(ui, "Pitch", |ui| {
        ui.add_enabled_ui(current.is_some(), |ui| {
            let mut pitches = Vec::new();
            if let Some((d, p)) = current {
                let coarse = metric_thread_sizes().iter().find(|(n, _)| *n == d).map(|(_, p)| *p);
                pitches.extend(PITCHES.iter().copied().filter(|p| *p < d && *p <= coarse.unwrap_or(d)));
                pitches.extend(coarse);
                pitches.push(p);
            }
            pitches.sort_by(|a, b| b.total_cmp(a));
            pitches.dedup();
            let labels: Vec<String> = pitches.iter().map(|p| format!("{p} mm")).collect();
            let labels: Vec<&str> = labels.iter().map(String::as_str).collect();
            let mut selected = current.and_then(|(_, p)| pitches.iter().position(|n| *n == p)).unwrap_or(0);
            let before = selected;
            combo(ui, "thread_pitch", &labels, &mut selected);
            if selected != before
                && let (Some((d, _)), Some(p)) = (current, pitches.get(selected))
            {
                *designation = self::designation(d, *p);
                *suggested = None;
            }
        });
    });

    let (enter, changed) = text_row(ui, "Designation", designation, tr("fits the face, e.g. M8"));
    if let Some(at) = crate::scenario::handle_at("dialog:Designation") {
        crate::scenario::publish_handle("dialog:thread_size", at);
    }
    if changed {
        *suggested = None;
    }
    (enter, reset)
}

/// The shapes of a custom thread.
const SHAPES: &[&str] = &["Triangle", "Trapezoid", "Square", "Own contour"];

fn fmt(x: f64) -> String {
    let r = (x * 1000.0).round() / 1000.0;
    if (r - r.round()).abs() < 1e-9 { format!("{}", r.round()) } else { format!("{r}") }
}

/// A custom thread's shape and sizes, written into its designation as they change.
fn custom_rows(ui: &mut egui::Ui, designation: &mut String, suggested: &mut Option<String>) {
    use solidcraft_engine::doc::thread_spec;
    let spec = thread_spec::parse(designation);
    let up = designation.to_ascii_uppercase();
    let lh = if spec.as_ref().is_some_and(|s| s.left_hand) { " LH" } else { "" };
    let memo = egui::Id::new("thread_custom_shape");
    let derived = match &spec {
        Some(s) if s.contour.is_some() => 3,
        Some(_) if up.contains(" SQ") => 2,
        Some(_) if up.split_whitespace().any(|w| w.starts_with('C')) => 1,
        Some(_) => 0,
        None => ui.ctx().data(|d| d.get_temp(memo)).unwrap_or(0),
    };
    let mut shape = derived;
    form_row(ui, "Shape", |ui| combo(ui, "thread_custom_shape", SHAPES, &mut shape));
    ui.ctx().data_mut(|d| d.insert_temp(memo, shape));
    let (d, p) = spec.as_ref().map(|s| (s.major, s.pitch)).unwrap_or((20.0, 4.0));
    if shape != derived {
        let h = p / 2.0;
        *designation = match shape {
            1 => format!("{}x{} 30° h{} c{}{lh}", fmt(d), fmt(p), fmt(h), fmt(0.366 * p)),
            2 => format!("{}x{} SQ{lh}", fmt(d), fmt(p)),
            3 => format!("{}x{} P0:0;{}:{};{}:{};{}:0{lh}", fmt(d), fmt(p), fmt(p / 4.0), fmt(h), fmt(p / 2.0), fmt(h), fmt(3.0 * p / 4.0)),
            _ => format!("{}x{} 60°{lh}", fmt(d), fmt(p)),
        };
        *suggested = None;
        return;
    }
    let Some(s) = spec else { return };
    let (mut d, mut p) = (s.major, s.pitch);
    let mut angle = (s.flanks.0 + s.flanks.1).to_degrees();
    let (mut h, mut c) = (s.depth, s.crest_flat);
    let mut changed = false;
    let mut field = |ui: &mut egui::Ui, label: &str, v: &mut f64, range: std::ops::RangeInclusive<f64>, unit: &str| {
        let _ = unit;
        let r = form_row(ui, label, |ui| crate::ui_kit::scalar_field(ui, v, range));
        changed |= r.changed();
    };
    field(ui, "Diameter", &mut d, 0.5..=2000.0, " mm");
    field(ui, "Pitch", &mut p, 0.1..=200.0, " mm");
    if shape <= 1 {
        field(ui, "Angle", &mut angle, 0.0..=160.0, "°");
    }
    if (1..=2).contains(&shape) {
        field(ui, "Depth", &mut h, 0.01..=1000.0, " mm");
    }
    if shape == 1 {
        field(ui, "Crest width", &mut c, 0.01..=200.0, " mm");
    }
    if changed {
        let next = match shape {
            1 => format!("{}x{} {}° h{} c{}{lh}", fmt(d), fmt(p), fmt(angle), fmt(h), fmt(c)),
            2 => format!("{}x{} SQ h{}{lh}", fmt(d), fmt(p), fmt(h)),
            3 => {
                let pts: Vec<String> = s.contour.iter().flatten().map(|(x, h)| format!("{}:{}", fmt(*x), fmt(*h))).collect();
                format!("{}x{} P{}{lh}", fmt(d), fmt(p), pts.join(";"))
            }
            _ => format!("{}x{} {}°{lh}", fmt(d), fmt(p), fmt(angle)),
        };
        // Only a thread that reads back replaces the designation (a crest too wide for the
        // pitch, say, keeps the last good one).
        if thread_spec::parse(&next).is_some() {
            *designation = next;
            *suggested = None;
        }
    }
    if shape == 3 {
        // The points are typed as x:h pairs (along the axis, height above the root) over one
        // pitch; the text stays while it doesn't read as a contour yet.
        let key = egui::Id::new("thread_custom_points");
        let current: Vec<String> = s.contour.iter().flatten().map(|(x, h)| format!("{}:{}", fmt(*x), fmt(*h))).collect();
        let current = current.join("; ");
        let mut text: String =
            ui.ctx().data(|dd| dd.get_temp::<(String, String)>(key)).filter(|(of, _)| *of == *designation).map(|(_, t)| t).unwrap_or(current);
        let before = text.clone();
        text_row(ui, "Points", &mut text, tr("x:h; x:h; … over one pitch"));
        let mut shown_for = designation.clone();
        if text != before {
            let compact: String = text.split_whitespace().collect();
            let next = format!("{}x{} P{}{lh}", fmt(s.major), fmt(s.pitch), compact);
            if thread_spec::parse(&next).is_some() {
                *designation = next;
                *suggested = None;
                shown_for = designation.clone();
            }
        }
        ui.ctx().data_mut(|dd| dd.insert_temp(key, (shown_for, text)));
    }
    preview(ui, &s);
}

/// Two pitches of the thread's outline (the material below it), to see the shape.
fn preview(ui: &mut egui::Ui, s: &solidcraft_engine::doc::thread_spec::ThreadSpec) {
    let p = s.pitch;
    let one: Vec<(f64, f64)> = match &s.contour {
        Some(pts) => pts.clone(),
        None => {
            // The falling flank closes the period (to the next root).
            let t1 = s.flanks.0.tan();
            let rf = s.root_flat().max(0.0);
            vec![(0.0, 0.0), (rf, 0.0), (rf + s.depth * t1, s.depth), (rf + s.depth * t1 + s.crest_flat, s.depth)]
        }
    };
    let mut line: Vec<(f64, f64)> = (0..3).flat_map(|k| one.iter().map(move |(x, h)| (x + p * k as f64, *h))).collect();
    if let Some(f) = one.first() {
        line.push((f.0 + 3.0 * p, f.1));
    }
    let (w, ht) = (220.0_f32, 56.0_f32);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, ht), egui::Sense::hover());
    let span = (2.0 * p).max(1e-9);
    let scale = ((w - 8.0) as f64 / span).min((ht - 10.0) as f64 / s.depth.max(1e-9));
    let to = |(x, h): (f64, f64)| egui::pos2(rect.left() + 4.0 + (x * scale) as f32, rect.bottom() - 5.0 - (h * scale) as f32);
    let pts: Vec<egui::Pos2> = line.iter().filter(|q| q.0 <= 2.0 * p + 1e-9).map(|q| to(*q)).collect();
    let stroke = ui.visuals().widgets.active.fg_stroke;
    ui.painter().add(egui::Shape::line(pts, egui::Stroke::new(1.5, stroke.color)));
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod contribution_tests {
    use super::*;
    use crate::{
        Services, SolveApp,
        dialogs::{Dialog, Kind, apply_commands},
    };
    use serde_json::json;
    use solidcraft_engine::{Sel, Session, geom::Vec3};
    fn hole() -> (SolveApp, Dialog) {
        let mut app = SolveApp::new(Session::default(), Services::default());
        app.run("solid.box", json!({"length":20,"width":20,"height":10,"body_name":"Plate"})).unwrap();
        app.session.selection = vec![Sel::Face { body: "Plate".into(), index: 5, point: Vec3::new(10.0, 10.0, 10.0) }];
        let d = Dialog::for_command(&app, "solid.hole").unwrap();
        (app, d)
    }
    fn custom_hole_widget() -> crate::scenario::Harness {
        let (app, dialog) = hole();
        let mut h = crate::scenario::Harness::new();
        h.app = app;
        h.app.dialog = Some(dialog);
        h.frames(3);
        h.step(&json!({"click":{"handle":"dialog:Hole Tap Type"}})).unwrap();
        h.step(&json!({"click":{"handle":"dialog:Hole Tap Type:Tapped"}})).unwrap();
        h.step(&json!({"click":{"handle":"dialog:Size"}})).unwrap();
        let at = crate::scenario::handle_at("dialog:Size:M5").unwrap();
        h.call("ui.scroll", json!({"x":at.x,"y":at.y,"delta":-2000}));
        h.frames(20);
        h.step(&json!({"widget":"Custom"})).unwrap();
        h.frames(3);
        h
    }

    fn replace_field(h: &mut crate::scenario::Harness, field: &str, value: &str) {
        h.step(&json!({"click":{"handle":format!("dialog:{field}")}})).unwrap();
        h.step(&json!({"key":"A","cmd":true})).unwrap();
        h.step(&json!({"text":value})).unwrap();
        h.frames(2);
    }

    #[test]
    fn custom_tapped_size_keeps_its_choice_and_exposes_nominal_diameter() {
        let h = custom_hole_widget();
        assert!(h.widget("Custom").is_some(), "Custom must remain the selected size after the popup closes");
        assert!(crate::scenario::handle_at("dialog:Nominal diameter").is_some(), "a custom tapped hole needs an editable nominal diameter");
        let card = crate::scenario::rect_of("dialog:card").unwrap();
        for label in ["Profile", "Size", "Nominal diameter", "Pitch", "Designation", "Tap drill diameter"] {
            let row = crate::scenario::rect_of(&format!("dialog:row:{label}")).unwrap();
            let control = crate::scenario::rect_of(&format!("dialog:control:{label}")).unwrap();
            assert_eq!(row.height(), crate::theme::Tokens::get().control_height, "{label}");
            assert!(card.contains_rect(row) && row.contains_rect(control), "{label}: {row:?} {control:?} outside {card:?}");
        }
    }

    #[test]
    fn custom_nominal_diameter_and_pitch_change_the_real_hole_and_reopen() {
        let mut h = custom_hole_widget();
        replace_field(&mut h, "Nominal diameter", "8.5 mm");
        replace_field(&mut h, "Pitch", "1 mm");
        let commands = apply_commands(&h.app, h.app.dialog.as_ref().unwrap()).unwrap();
        assert_eq!(commands[0].1["thread"], "M8.5x1");
        assert!(commands[0].1.get("diameter").is_none(), "the nominal size must reach the engine through the thread designation");
        h.step(&json!({"click":{"handle":"dialog:OK"}})).unwrap();
        h.frames(3);
        assert!(h.app.dialog.is_none());
        let faces = h.app.run("model.faces", json!({"body":"Plate"})).unwrap();
        assert!(
            faces["faces"].as_array().unwrap().iter().any(|face| { face["cylinder"]["radius"].as_f64().is_some_and(|r| (r - 3.75).abs() < 1e-9) }),
            "the physical tap drill must be 8.5 - 1 = 7.5 mm"
        );
        let saved = h.app.session.doc.to_json();
        let reopened = Session::new(Document::from_json(&saved).unwrap());
        let world = reopened.world_state();
        assert_eq!(world.threads.len(), 1);
        assert_eq!(world.threads[0].designation, "M8.5x1");
        assert!((world.threads[0].pitch - 1.).abs() < 1e-9);
    }

    #[test]
    fn an_invalid_custom_pitch_blocks_the_old_valid_size_and_cancel_keeps_the_model() {
        let mut h = custom_hole_widget();
        let before = h.app.session.doc.to_json();
        let history = (h.app.session.undo.len(), h.app.session.redo.len());
        replace_field(&mut h, "Pitch", "5 mm");
        assert!(apply_commands(&h.app, h.app.dialog.as_ref().unwrap()).is_err());
        assert_eq!(crate::scenario::count_of("dialog:ok:enabled"), Some(0.0));
        h.step(&json!({"click":{"handle":"dialog:OK"}})).unwrap();
        assert!(h.app.dialog.is_some(), "an invalid custom draft must keep the dialog open");
        assert_eq!(h.app.session.doc.to_json(), before);
        assert_eq!((h.app.session.undo.len(), h.app.session.redo.len()), history);
        h.step(&json!({"key":"Escape"})).unwrap();
        assert!(h.app.dialog.is_none());
        assert_eq!(h.app.session.doc.to_json(), before);
        assert_eq!((h.app.session.undo.len(), h.app.session.redo.len()), history);
    }

    #[test]
    fn manual_designation_keeps_keyboard_focus_when_custom_fields_appear() {
        let (app, dialog) = hole();
        let mut h = crate::scenario::Harness::new();
        h.app = app;
        h.app.dialog = Some(dialog);
        h.frames(3);
        h.step(&json!({"click":{"handle":"dialog:Hole Tap Type"}})).unwrap();
        h.step(&json!({"click":{"handle":"dialog:Hole Tap Type:Tapped"}})).unwrap();
        h.step(&json!({"click":{"handle":"dialog:Designation"}})).unwrap();
        h.step(&json!({"key":"A","cmd":true})).unwrap();
        for text in ["M", "8", "x", "1"] {
            h.step(&json!({"text":text})).unwrap();
            h.frames(2);
        }
        assert_eq!(apply_commands(&h.app, h.app.dialog.as_ref().unwrap()).unwrap()[0].1["thread"], "M8x1");
        assert!(crate::scenario::handle_at("dialog:Nominal diameter").is_some());
    }

    #[test]
    fn custom_lengths_use_explicit_units_and_invalid_drafts_never_apply_stale_values() {
        let (mut app, mut dialog) = hole();
        app.run("document.units", json!({"units":"in"})).unwrap();
        let Kind::Hole { opts, .. } = &mut dialog.kind else { panic!("not a hole") };
        opts.tapped = true;
        opts.all = true;
        let mut draft = HoleThreadDraft::new(&opts.thread);
        draft.diameter = "0.25 in".into();
        draft.pitch = "0.05 in".into();
        draft.update(&app.session.doc, &mut opts.thread);
        assert!(draft.error.is_none());
        let (d, p) = parse_metric_thread(&opts.thread).unwrap();
        assert!((d - 6.35).abs() < 1e-9 && (p - 1.27).abs() < 1e-9);
        opts.custom_thread = Some(draft);
        let commands = apply_commands(&app, &dialog).unwrap();
        app.run(&commands[0].0, commands[0].1.clone()).unwrap();
        let Kind::Hole { opts, .. } = &mut dialog.kind else { panic!("not a hole") };
        for invalid in ["", "0", "-1 mm", "unknown_size", "NaN"] {
            let draft = opts.custom_thread.as_mut().unwrap();
            draft.diameter = invalid.into();
            draft.update(&app.session.doc, &mut opts.thread);
            assert!(draft.error.is_some(), "{invalid}");
        }
        assert!(apply_commands(&app, &dialog).is_err());
    }
    #[test]
    fn contribution_tapped_holes_derive_the_drill_from_the_catalogue() {
        for (designation, expected) in [("M5", 4.2), ("M6x1", 5.0), ("M5x0.5", 4.5)] {
            let (mut app, mut d) = hole();
            if let Kind::Hole { diameter, opts, .. } = &mut d.kind {
                *diameter = "17 mm".into();
                opts.tapped = true;
                opts.all = true;
                opts.thread = designation.into();
            }
            assert!(d.primary().is_none(), "no independent diameter editor for a tapped hole");
            let commands = apply_commands(&app, &d).unwrap();
            let (id, p) = &commands[0];
            assert!(p.get("diameter").is_none());
            assert_eq!(p["thread"], designation);
            let out = app.run(id, p.clone()).unwrap();
            let f = app.session.doc.feature(out["feature"].as_u64().unwrap()).unwrap();
            let solidcraft_engine::doc::FeatureKind::Hole { diameter, .. } = &f.kind else { panic!("not a Hole") };
            let drill = app.session.doc.eval(diameter, solidcraft_engine::doc::expr::Kind::Length).unwrap();
            assert!((drill - expected).abs() < 1e-9, "{designation}: {drill}");
        }
    }
    #[test]
    fn contribution_invalid_tapped_holes_are_refused_and_plain_holes_keep_diameter() {
        let (app, mut d) = hole();
        assert!(d.primary().is_some());
        assert!(apply_commands(&app, &d).unwrap()[0].1.get("diameter").is_some());
        for designation in ["", "M0", "M5x5", "M5xNaN", "not a thread"] {
            if let Kind::Hole { opts, .. } = &mut d.kind {
                opts.tapped = true;
                opts.thread = designation.into();
            }
            assert!(apply_commands(&app, &d).is_err(), "{designation}");
        }
    }
    #[test]
    fn contribution_thread_catalogue_is_parseable_and_rendering_keeps_custom_designations() {
        for (d, p) in metric_thread_sizes() {
            assert_eq!(parse_metric_thread(&format!("M{d}")), Some((*d, *p)));
            assert_eq!(parse_metric_thread(&format!("M{d}x{p}")), Some((*d, *p)));
        }
        let ctx = egui::Context::default();
        for original in ["M5", "M5x0.5", "M33x1", "invalid"] {
            let mut designation = original.to_string();
            let mut custom = None;
            let doc = Document::default();
            let _ = ctx.run_ui(Default::default(), |ui| {
                egui::Grid::new("hole_catalogue").show(ui, |ui| {
                    hole_rows(ui, &doc, &mut designation, &mut custom);
                });
            });
            assert_eq!(designation, original);
        }
    }

    #[test]
    fn contribution_metric_designation_omits_only_the_catalogue_coarse_pitch() {
        for &(d, p) in metric_thread_sizes() {
            assert_eq!(designation(d, p), format!("M{d}"));
            assert_eq!(parse_metric_thread(&designation(d, p)), Some((d, p)));
            assert_eq!(parse_metric_thread(&designation(d, p / 2.)), Some((d, p / 2.)));
            assert!(designation(d, p / 2.).contains('x'));
        }
        assert_eq!(designation(8., 1.25), "M8");
        assert_eq!(designation(8., 1.), "M8x1");
        assert_eq!(designation(5., 0.5), "M5x0.5");
        assert_eq!(designation(33., 1.), "M33x1");
        assert_eq!(designation(8., 1.25 + 1e-12), "M8");
        assert!(designation(8., 1.25 + 1e-6).contains('x'));
    }

    #[test]
    fn contribution_metric_designation_rendering_preserves_authored_legacy_and_custom_text() {
        let context = egui::Context::default();
        crate::fonts::setup_bundled_fonts(&context);
        for original in ["M8x1.25", "M8x1", "M33x1", "M5×0.5", " m8x1.25 ", "invalid"] {
            let mut hole = original.to_string();
            let mut thread = original.to_string();
            let mut suggested = None;
            let mut modeled = false;
            let mut full = true;
            let mut custom = None;
            let doc = Document::default();
            let _ = context.run_ui(Default::default(), |ui| {
                egui::Grid::new("authored_hole").show(ui, |ui| {
                    hole_rows(ui, &doc, &mut hole, &mut custom);
                });
                egui::Grid::new("authored_thread").show(ui, |ui| {
                    rows(ui, &mut thread, &mut suggested, &mut modeled, &mut full);
                });
            });
            assert_eq!(hole, original);
            assert_eq!(thread, original);
        }
    }

    #[test]
    fn contribution_metric_designation_keeps_domain_pitch_drill_units_and_reopen() {
        for (diameter, pitch, drill) in [(8., 1.25, 6.75), (8., 1., 7.), (5., 0.5, 4.5)] {
            let (mut app, mut dialog) = hole();
            app.run("document.units", json!({"units":"in"})).unwrap();
            if let Kind::Hole { opts, .. } = &mut dialog.kind {
                opts.tapped = true;
                opts.all = true;
                opts.thread = designation(diameter, pitch);
            }
            let commands = apply_commands(&app, &dialog).unwrap();
            let result = app.run(&commands[0].0, commands[0].1.clone()).unwrap();
            let feature = app.session.doc.feature(result["feature"].as_u64().unwrap()).unwrap();
            let solidcraft_engine::doc::FeatureKind::Hole { diameter, .. } = &feature.kind else { panic!("not a hole") };
            assert!((app.session.doc.eval(diameter, solidcraft_engine::doc::expr::Kind::Length).unwrap() - drill).abs() < 1e-9);
            let saved = app.session.doc.to_json();
            let reopened = Session::new(solidcraft_engine::doc::Document::from_json(&saved).unwrap());
            let state = reopened.world_state();
            assert_eq!(state.threads.len(), 1);
            assert!((state.threads[0].pitch - pitch).abs() < 1e-9);
        }
    }
}
