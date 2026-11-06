//! About this chain, under the sprocket dialog's Chain: what kind of chain the number is (ISO A
//! or B, ANSI, motorcycle, bicycle), its pitch in millimetres and inches, its roller diameter and
//! inner width, the other numbers of the same chain, what it is used for and how to measure one,
//! with a sketch of the chain (side and top) carrying p, d1 and b1.

use egui::{Align2, FontId, Pos2, Rect, Stroke, Ui, pos2, vec2};
use solidcraft_engine::doc::gear::{Chain, ChainFamily, inch_pitch, same_size};

use crate::i18n::tr_label;
use crate::theme::Tokens;
use crate::ui_kit;

/// What the chain number says, in a sentence.
pub fn family_text(c: &Chain) -> &'static str {
    match ChainFamily::of(c) {
        ChainFamily::IsoA => "ISO 606 A series: the American (ANSI) chain sizes in metric",
        ChainFamily::IsoB => "ISO 606 B series: the European (British) standard chain",
        ChainFamily::Ansi => "ANSI roller chain: the first digits are the pitch in eighths of an inch",
        ChainFamily::Motorcycle => "Motorcycle chain: the first digit is the pitch in eighths of an inch, the last two the inner width in eightieths",
        ChainFamily::Bicycle => "Bicycle and moped chain, 1/2\" pitch",
        ChainFamily::Custom => "A chain of your own: pitch x roller x inner width",
    }
}

/// What the chain is usually found on.
pub fn use_text(c: &Chain) -> &'static str {
    match c.name {
        "415" | "420" => "Mopeds and small motorcycles (50 cc)",
        "428" => "Light motorcycles (125 cc)",
        "520" => "Off-road and mid-size motorcycles (250-600 cc)",
        "525" | "530" => "Road motorcycles (600-1000 cc and more)",
        "630" => "Older heavy motorcycles",
        "081" => "Single-speed bicycles",
        "083" => "Mopeds and cargo bicycles",
        "25" | "35" | "04B" | "05B" | "06B" => "Light drives: small machines, models, printers",
        _ => match ChainFamily::of(c) {
            ChainFamily::Custom => "",
            _ if c.pitch <= 16.0 => "Industrial drives and conveyors; light machines",
            _ => "Heavy industrial drives and conveyors",
        },
    }
}

fn mm(v: f64) -> String {
    let t = format!("{v:.3}");
    let t = t.trim_end_matches('0').trim_end_matches('.');
    format!("{t} mm")
}

/// The rows under the chain's choice: what it is, its sizes, and its sketch.
pub fn rows(ui: &mut Ui, c: &Chain) {
    ui_kit::section(ui, "sprocket_chain_about", "About this chain", true, |ui| {
        ui_kit::text(ui, tr_label(family_text(c)), ui_kit::Text::Note("chain_family"));
        let pitch = match inch_pitch(c.pitch) {
            Some(i) => format!("{}  ({i})", mm(c.pitch)),
            None => mm(c.pitch),
        };
        let width = match inch_pitch(c.inner_width) {
            Some(i) if ChainFamily::of(c) == ChainFamily::Motorcycle || ChainFamily::of(c) == ChainFamily::Bicycle => format!("{}  ({i})", mm(c.inner_width)),
            _ => mm(c.inner_width),
        };
        for (label, value) in [("Pitch (p)", pitch), ("Roller diameter (d1)", mm(c.roller)), ("Inner width (b1)", width)] {
            ui_kit::row(ui, label, |ui| ui_kit::text(ui, &value, ui_kit::Text::Value(label, false)));
        }
        let same = same_size(c);
        if !same.is_empty() {
            ui_kit::row(ui, "Same chain as", |ui| ui_kit::text(ui, &same.join(", "), ui_kit::Text::Value("Same chain as", false)));
        }
        let used = use_text(c);
        if !used.is_empty() {
            ui_kit::text(ui, &format!("{}: {}", tr_label("Used on"), tr_label(used)), ui_kit::Text::Note("chain_use"));
        }
        sketch(ui, c);
        ui_kit::text(
            ui,
            tr_label("To tell a chain: measure across three rollers, centre to centre, and halve it (the pitch); then the gap between the inner plates (the inner width)."),
            ui_kit::Text::Note("chain_measure"),
        );
    });
}

/// Three rollers from the side with their plates, p between two of them and d1 across one; and
/// the chain from above, b1 between its inner plates.
fn sketch(ui: &mut Ui, c: &Chain) {
    let t = Tokens::get();
    let w = ui.available_width().max(160.0);
    let (rect, _) = ui.allocate_exact_size(vec2(w, 120.0), egui::Sense::hover());
    crate::scenario::publish_rect("dialog:chain_sketch", rect);
    let p = ui.painter_at(rect);
    let ink = Stroke::new(1.4, t.form_ink);
    let dim = Stroke::new(1.0, t.form_primary);
    let font = FontId::proportional(t.font_small);
    // Side view: rollers at 0, p, 2p; plates a little taller than the rollers.
    let side = Rect::from_min_max(rect.min + vec2(8.0, 22.0), pos2(rect.min.x + w * 0.64, rect.max.y - 22.0));
    let span = 2.0 * c.pitch + c.roller * 1.6;
    let k = (side.width() / span as f32).min(side.height() / (c.roller as f32 * 1.6));
    let cy = side.center().y;
    let x0 = side.center().x - c.pitch as f32 * k;
    let r = c.roller as f32 * k / 2.0;
    let plate = r * 1.45;
    let centres: Vec<Pos2> = (0..3).map(|i| pos2(x0 + i as f32 * c.pitch as f32 * k, cy)).collect();
    for pair in centres.windows(2) {
        if let [a, b] = pair {
            let body = Rect::from_min_max(pos2(a.x, cy - plate), pos2(b.x, cy + plate)).expand2(vec2(plate, 0.0));
            p.rect_stroke(body, plate, ink, egui::StrokeKind::Middle);
        }
    }
    for c0 in &centres {
        p.circle_filled(*c0, r, t.form_primary_fill);
        p.circle_stroke(*c0, r, ink);
    }
    // p: above the first two rollers.
    let (a, b) = (centres[0], centres[1]);
    let y = cy - plate - 9.0;
    arrow_line(&p, pos2(a.x, y), pos2(b.x, y), dim);
    for x in [a.x, b.x] {
        p.line_segment([pos2(x, y - 4.0), pos2(x, cy)], Stroke::new(0.6, t.form_dim));
    }
    p.text(pos2((a.x + b.x) / 2.0, y - 2.0), Align2::CENTER_BOTTOM, format!("p {}", mm(c.pitch)), font.clone(), t.form_primary);
    // d1: across the third roller.
    let c2 = centres[2];
    arrow_line(&p, pos2(c2.x - r, cy), pos2(c2.x + r, cy), dim);
    p.text(pos2(c2.x, cy + plate + 4.0), Align2::CENTER_TOP, format!("d1 {}", mm(c.roller)), font.clone(), t.form_primary);
    // Top view: the outer and inner plates as lines, b1 between the inner ones.
    let top = Rect::from_min_max(pos2(rect.min.x + w * 0.70, rect.min.y + 22.0), rect.max - vec2(8.0, 22.0));
    let kt = top.height() / ((c.inner_width as f32) * 2.4).max(1.0);
    let half = c.inner_width as f32 * kt / 2.0;
    let mid = top.center().y;
    let (l, rr) = (top.min.x, top.max.x);
    for (off, s) in [(half, ink), (half + 4.0, ink)] {
        p.line_segment([pos2(l, mid - off), pos2(rr, mid - off)], s);
        p.line_segment([pos2(l, mid + off), pos2(rr, mid + off)], s);
    }
    let rx = top.center().x;
    p.rect_filled(Rect::from_center_size(pos2(rx, mid), vec2(r.min(top.width() / 3.0) * 2.0, half * 2.0)), 1.0, t.form_primary_fill);
    arrow_line(&p, pos2(rr - 6.0, mid - half), pos2(rr - 6.0, mid + half), dim);
    p.text(pos2(top.center().x, top.max.y + 4.0), Align2::CENTER_TOP, format!("b1 {}", mm(c.inner_width)), font, t.form_primary);
}

fn arrow_line(p: &egui::Painter, a: Pos2, b: Pos2, s: Stroke) {
    p.line_segment([a, b], s);
    let d = (b - a).normalized() * 5.0;
    let n = vec2(-d.y, d.x) * 0.5;
    for (tip, dir) in [(a, d), (b, -d)] {
        p.line_segment([tip, tip + dir + n], s);
        p.line_segment([tip, tip + dir - n], s);
    }
}

#[cfg(test)]
#[path = "chain_help_tests.rs"]
mod tests;
