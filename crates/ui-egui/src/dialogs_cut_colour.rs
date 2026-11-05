//! Cut Colour, on the CUTTING tab: the contours picked in the sketch take one of LightBurn's
//! layer colours (a click on a curve takes its whole closed contour), which the cutting and the
//! nesting DXFs carry, so LightBurn puts each contour on the layer of its colour. The dialog
//! stays open to colour one group of contours after another; everything goes through
//! `sketch.cut_colour`.

use egui::{Color32, Stroke, vec2};
use serde_json::{Value, json};
use solidcraft_engine::Sel;
use solidcraft_engine::sketch::cut_colour::PALETTE;

use crate::SolveApp;
use crate::dialogs::Kind;
use crate::selection::{CURVES, SelInput};
use crate::ui_kit;

/// The palette's named colours as the dialog names them (the others go by their number).
pub const NAMES: [&str; 17] = [
    "Black",
    "Blue",
    "Red",
    "Green",
    "Yellow",
    "Orange",
    "Cyan",
    "Magenta",
    "Light gray",
    "Dark blue",
    "Dark red",
    "Dark green",
    "Olive",
    "Brown",
    "Sky blue",
    "Purple",
    "Gray",
];

/// The dialog's state: what the last click did.
#[derive(Clone, Debug, Default)]
pub struct CutColourUi {
    pub status: Option<String>,
}

/// A colour's label: its name, else its LightBurn layer number.
pub fn label(index: usize) -> String {
    match NAMES.get(index) {
        Some(n) => format!("{} ({index:02})", crate::i18n::tr_label(n)),
        None => format!("{index:02}"),
    }
}

/// The dialog for `sketch.cut_colour`, with the sketch curves already picked.
pub fn start(app: &SolveApp, id: &str) -> Option<(Kind, Vec<SelInput>)> {
    if id != "sketch.cut_colour" {
        return None;
    }
    let mut input = SelInput::new("Contours", CURVES, true);
    input.items = app.session.selection.iter().filter(|s| matches!(s, Sel::SketchCurve { .. })).cloned().collect();
    Some((Kind::CutColour(Box::default()), vec![input]))
}

fn picked(inputs: &[SelInput]) -> Vec<String> {
    inputs
        .first()
        .map(|i| i.items.iter().filter_map(|s| if let Sel::SketchCurve { id, .. } = s { Some(id.clone()) } else { None }).collect())
        .unwrap_or_default()
}

/// Give the picked contours a colour (none: back to red edges and blue holes).
pub fn apply(app: &mut SolveApp, k: &mut CutColourUi, inputs: &[SelInput], colour: Option<usize>) {
    let curves = picked(inputs);
    if curves.is_empty() {
        k.status = Some(crate::i18n::tr_label("Select the contours first").to_string());
        return;
    }
    let colour = colour.map_or(Value::Null, |c| json!(c));
    k.status = Some(match app.run("sketch.cut_colour", json!({"curves": curves, "colour": colour})) {
        Ok(r) => format!(
            "{}: {}",
            crate::i18n::tr_label("Coloured curves"),
            r["curves"].as_array().map_or(0, Vec::len)
        ),
        Err(e) => e,
    });
}

/// The dialog's rows: the colours as swatches, No Colour, and what the last click did.
pub fn rows(app: &mut SolveApp, ui: &mut egui::Ui, k: &mut CutColourUi, inputs: &[SelInput]) -> bool {
    ui.label(crate::i18n::tr_label("Pick contours in the sketch, then click a colour"));
    let mut chosen: Option<Option<usize>> = None;
    let size = vec2(26.0, 22.0);
    for row in PALETTE.chunks(6).enumerate() {
        let (r, colours) = row;
        ui.horizontal(|ui| {
            for (c, (_, rgb)) in colours.iter().enumerate() {
                let i = r * 6 + c;
                let fill = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
                let swatch = egui::Button::new("").fill(fill).stroke(Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color)).min_size(size);
                if ui.add(swatch).on_hover_text(label(i)).clicked() {
                    chosen = Some(Some(i));
                }
            }
        });
    }
    if ui_kit::button(ui, crate::i18n::tr_label("No colour"), ui_kit::Button::Normal).clicked() {
        chosen = Some(None);
    }
    if let Some(c) = chosen {
        apply(app, k, inputs, c);
    }
    if let Some(s) = &k.status {
        ui.label(s.as_str());
    }
    false
}

/// A colour menu (a part's in the nesting panel): the colour by its name, or "Contours' own";
/// what was chosen, `Some(None)` for none.
pub fn menu(ui: &mut egui::Ui, id: impl egui::AsIdSalt, current: Option<&str>) -> Option<Option<usize>> {
    let index = current.and_then(solidcraft_engine::sketch::cut_colour::parse).map(usize::from);
    let text = index.map_or_else(|| crate::i18n::tr_label("Contours' own").to_string(), label);
    let mut chosen = None;
    ui_kit::dropdown_menu(ui, id, &text, |ui| {
        if ui_kit::button(ui, crate::i18n::tr_label("Contours' own"), ui_kit::Button::Small).clicked() {
            chosen = Some(None);
        }
        for (i, (_, rgb)) in PALETTE.iter().enumerate() {
            let fill = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(vec2(14.0, 14.0), egui::Sense::hover());
                ui.painter().rect_filled(rect, 2.0, fill);
                if ui_kit::button(ui, label(i), ui_kit::Button::Small).clicked() {
                    chosen = Some(Some(i));
                }
            });
        }
    });
    chosen
}

#[cfg(test)]
#[path = "dialogs_cut_colour_tests.rs"]
mod tests;
