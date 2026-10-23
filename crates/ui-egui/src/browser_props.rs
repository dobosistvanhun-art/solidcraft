//! The Properties strip at the foot of the left sidebar: material, mass, volume and surface
//! area of the selected bodies (or of every body when none is selected), from `inspect.measure`
//! (a sheet metal body weighs as its rule's material when it has none of its own). Measured
//! again only when the design or the selection changes.

use egui::{RichText, vec2};
use serde_json::{Value, json};
use solidcraft_engine::Sel;

use crate::SolveApp;
use crate::theme::Tokens;

/// The bodies the strip measures: the selected ones (a face or edge counts for its body), in
/// selection order without repeats; empty for all.
pub fn selected_bodies(app: &SolveApp) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for s in &app.session.selection {
        let name = match s {
            Sel::Body { name } => name.clone(),
            Sel::Face { body, .. } | Sel::Edge { body, .. } => body.clone(),
            _ => continue,
        };
        if !out.contains(&name) {
            out.push(name);
        }
    }
    out
}

/// What the strip shows: (revision, bodies) it was measured for, and the result.
#[derive(Clone, Default)]
struct Cache {
    key: Option<(u64, Vec<String>)>,
    value: Option<Result<Value, String>>,
}

fn measured(app: &mut SolveApp, ui: &egui::Ui) -> Option<Result<Value, String>> {
    let id = egui::Id::new("sc_props_strip_cache");
    let bodies = selected_bodies(app);
    let mut cache: Cache = ui.ctx().data(|d| d.get_temp(id)).unwrap_or_default();
    if cache.key.as_ref() != Some(&(app.session.revision, bodies.clone())) {
        let known: Vec<String> = app.session.world_state().bodies.iter().map(|b| b.name.clone()).collect();
        let bodies: Vec<String> = bodies.into_iter().filter(|b| known.contains(b)).collect();
        cache.value = if known.is_empty() {
            None
        } else {
            Some(app.session.execute("inspect.measure", &json!({ "bodies": bodies })).map_err(|e| e.to_string()))
        };
        // Keyed after measuring, so a measurement that touched the revision is not repeated.
        cache.key = Some((app.session.revision, selected_bodies(app)));
        ui.ctx().data_mut(|d| d.insert_temp(id, cache.clone()));
    }
    cache.value
}

/// The strip (header and, open, the values). Drawn at the bottom of the sidebar.
pub fn strip(app: &mut SolveApp, ui: &mut egui::Ui) {
    let t = Tokens::get();
    let picked = selected_bodies(app);
    let what = match picked.as_slice() {
        [] => crate::i18n::tr_label("all bodies").to_string(),
        [one] => one.clone(),
        many => format!("{} {}", many.len(), crate::i18n::tr_label("bodies")),
    };
    let title = format!("{} · {what}", crate::i18n::tr_label("Properties"));
    // Where the section's header is drawn: for scenarios and the control channel.
    let header = ui.cursor().left_top() + vec2(t.space_10, t.space_10);
    crate::scenario::publish_handle("browser-properties-toggle", header);
    crate::ui_kit::section(ui, "sc_props_strip", &title, true, |ui| values(app, ui));
}

/// The strip's values: material, density (one body), mass, volume and area.
fn values(app: &mut SolveApp, ui: &mut egui::Ui) {
    let t = Tokens::get();
    let value = match measured(app, ui) {
        None => {
            ui.label(RichText::new(crate::i18n::tr_label("No bodies yet")).weak());
            return;
        }
        Some(Err(e)) => {
            ui.label(RichText::new(e).color(t.error));
            return;
        }
        Some(Ok(v)) => v,
    };
    let f = |x: &Value, k: &str| x.get(k).and_then(Value::as_f64).unwrap_or(f64::NAN);
    let total = &value["total"];
    let bodies = value["bodies"].as_array().cloned().unwrap_or_default();
    egui::Grid::new("sc_props_strip_grid").num_columns(2).spacing(vec2(12.0, 2.0)).show(ui, |ui| {
        let row = |ui: &mut egui::Ui, k: &str, v: String| {
            ui.label(RichText::new(crate::i18n::tr_label(k)).color(t.text_dim));
            ui.label(v);
            ui.end_row();
        };
        // One material for all of them, or "mixed".
        let mats: Vec<&str> = bodies.iter().filter_map(|b| b["material"].as_str()).collect();
        let material = match mats.first() {
            // The default material is a word of the interface; library names stay as they are.
            Some(&"Default") if mats.iter().all(|x| *x == "Default") => crate::i18n::tr_label("Default").to_string(),
            Some(m) if mats.iter().all(|x| x == m) => m.to_string(),
            Some(_) => crate::i18n::tr_label("mixed").to_string(),
            None => crate::i18n::tr_label("Default").to_string(),
        };
        row(ui, "Material", material);
        if let [b] = bodies.as_slice() {
            row(ui, "Density", format!("{:.3} g/cm³", f(b, "density_g_cm3")));
        }
        let mass = f(total, "mass_g");
        row(ui, "Mass", if mass >= 1000.0 { format!("{:.3} kg", mass / 1000.0) } else { format!("{mass:.2} g") });
        row(ui, "Volume", crate::prefs::show_mm(app, f(total, "volume_mm3"), 3));
        row(ui, "Area", crate::prefs::show_mm(app, f(total, "area_mm2"), 2));
    });
}
