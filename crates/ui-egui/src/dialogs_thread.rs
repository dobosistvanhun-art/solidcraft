//! Metric thread choices use the document's sizing table and the existing designation
//! parameter. Browsing choices does not edit the model or freeze an automatic feature.

use solidcraft_engine::doc::{metric_thread_sizes, parse_metric_thread};

use crate::dialogs::{check, combo, row_label};
use crate::i18n::tr;

/// Common pitch presets, not an exhaustive catalogue of standard diameter/pitch pairs.
/// The coarse and current pitches are included separately; custom pairs remain editable.
const PITCHES: &[f64] = &[0.2, 0.25, 0.35, 0.5, 0.75, 1.0, 1.25, 1.5, 2.0, 3.0, 4.0];

/// Returns (Enter pressed, automatic sizing requested).
pub(crate) fn rows(ui: &mut egui::Ui, designation: &mut String, suggested: &mut Option<String>) -> (bool, bool) {
    // The thread standard: ISO metric keeps its diameter and pitch pickers below, the others
    // pick from their sizes (Custom is typed: diameter x pitch, angles, depth).
    use solidcraft_engine::doc::thread_spec;
    // The profile picked stays while a designation is half typed (unreadable): otherwise the
    // dialog would switch back to ISO metric under the cursor at every keystroke.
    let memo = egui::Id::new("thread_profile_choice");
    let remembered: Option<usize> = ui.ctx().data(|d| d.get_temp(memo));
    let profile = match (thread_spec::profile_of(designation), remembered.and_then(|i| thread_spec::PROFILES.get(i).copied())) {
        (Some(p), _) => p,
        (None, Some(p)) if !designation.trim().is_empty() => p,
        _ => "ISO metric",
    };
    let mut selected = thread_spec::PROFILES.iter().position(|p| *p == profile).unwrap_or(0);
    let before_profile = selected;
    row_label(ui, "Profile");
    combo(ui, "thread_profile", thread_spec::PROFILES, &mut selected);
    ui.end_row();
    if selected != before_profile
        && let Some(p) = thread_spec::PROFILES.get(selected)
    {
        *designation = match *p {
            "Custom" => "20x2.5 60°".to_string(),
            other => thread_spec::sizes(other).into_iter().nth(4).or_else(|| thread_spec::sizes(other).into_iter().next()).unwrap_or_default(),
        };
        *suggested = None;
    }
    ui.ctx().data_mut(|d| d.insert_temp(memo, selected));
    let profile = thread_spec::PROFILES.get(selected).copied().unwrap_or("ISO metric");
    if profile == "Custom" {
        custom_rows(ui, designation, suggested);
    }
    if profile != "ISO metric" {
        if profile != "Custom" {
            let sizes = thread_spec::sizes(profile);
            let labels: Vec<&str> = sizes.iter().map(String::as_str).collect();
            let shown = thread_spec::parse(designation).map(|s| s.designation).unwrap_or_default();
            let mut at = sizes.iter().position(|d| *d == shown || thread_spec::parse(d).map(|s| s.designation) == Some(shown.clone())).unwrap_or(0);
            let before = at;
            row_label(ui, "Size");
            combo(ui, "thread_family_size", &labels, &mut at);
            if at != before
                && let Some(d) = sizes.get(at)
            {
                *designation = d.clone();
                *suggested = None;
            }
            ui.end_row();
        }
        row_label(ui, "Designation");
        let response = ui.add(egui::TextEdit::singleline(designation).hint_text(tr("e.g. Tr16x4, G1/2, 20x2.5 60°")));
        crate::scenario::publish_handle("dialog:thread_size", response.rect.center());
        if response.changed() {
            *suggested = None;
        }
        let enter = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        ui.end_row();
        return (enter, false);
    }

    row_label(ui, "Automatic size");
    let before = designation.trim().is_empty() || suggested.as_deref() == Some(designation.as_str());
    let mut automatic = before;
    ui.add_enabled_ui(!designation.trim().is_empty(), |ui| check(ui, &mut automatic));
    let reset = automatic && !before;
    if automatic != before {
        *suggested = None;
        if automatic {
            designation.clear();
        }
    }
    ui.end_row();

    let current = parse_metric_thread(designation);
    let mut diameters: Vec<f64> = metric_thread_sizes().iter().map(|(d, _)| *d).collect();
    if let Some((d, _)) = current
        && !diameters.contains(&d)
    {
        diameters.push(d);
        diameters.sort_by(f64::total_cmp);
    }
    row_label(ui, "Size");
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
        *designation = format!("M{d}x{coarse}");
        *suggested = None;
    }
    ui.end_row();

    let current = parse_metric_thread(designation);
    row_label(ui, "Pitch");
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
            *designation = format!("M{d}x{p}");
            *suggested = None;
        }
    });
    ui.end_row();

    row_label(ui, "Designation");
    let response = ui.add(egui::TextEdit::singleline(designation).hint_text(tr("fits the face, e.g. M8")));
    crate::scenario::publish_handle("dialog:thread_size", response.rect.center());
    if response.changed() {
        *suggested = None;
    }
    let enter = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
    ui.end_row();
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
    row_label(ui, "Shape");
    combo(ui, "thread_custom_shape", SHAPES, &mut shape);
    ui.end_row();
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
        row_label(ui, label);
        let r = ui.add(egui::DragValue::new(v).speed(0.05).range(range).max_decimals(3).suffix(unit));
        changed |= r.changed();
        ui.end_row();
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
        row_label(ui, "Points");
        let r = ui.add(egui::TextEdit::multiline(&mut text).desired_rows(2).hint_text(tr("x:h; x:h; … over one pitch")));
        ui.end_row();
        let mut shown_for = designation.clone();
        if r.changed() {
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
    row_label(ui, "");
    preview(ui, &s);
    ui.end_row();
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
