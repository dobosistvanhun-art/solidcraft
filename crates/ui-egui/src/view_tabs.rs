//! The view tabs along the bottom of the viewport: **Model**, the **part** opened in a tab of its
//! own, one tab per **drawing** of the design, and **+** for a new drawing. They are views of the
//! same design, not designs of their own, so a part's changes are the assembly's and a drawing
//! follows every change of the model at once, saved or not.
//!
//! - A drawing tab opens the drawing editor on that drawing; Model closes it. Closing a drawing's
//!   tab never deletes the drawing.
//! - A part tab (a component's menu: Open in Tab) activates the component, with its own history,
//!   and isolates its placement; Model activates the assembly again and shows what was shown
//!   before. One part has a tab at a time.
//! - The strip shows once there is something to switch to, and only when no other dialog or tool
//!   is open: switching then must not throw away a half-made feature.

use std::cell::RefCell;

use egui::Rect;
use serde_json::json;

use crate::SolveApp;
use crate::dialogs::Kind;
use crate::ui_kit::{self, Button, Container, Surface};

/// What a view tab shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Shown {
    Model,
    Part(u64),
    Drawing(u64),
}

/// What was hidden before a part's tab isolated it, to show it again on the way back: the tab
/// (the design's tab number and the component) and the hidden placements and bodies then.
struct Before {
    tab: usize,
    component: u64,
    occurrences: Vec<Vec<u64>>,
    bodies: Vec<String>,
}

thread_local! {
    static BEFORE: RefCell<Option<Before>> = const { RefCell::new(None) };
}

/// What the viewport shows now (`None`: a dialog or tool of another kind is open).
fn shown(app: &SolveApp) -> Option<Shown> {
    if app.tool.is_some() || app.applying.is_some() {
        return None;
    }
    match app.dialog.as_ref().map(|d| &d.kind) {
        Some(Kind::Drawing(k)) => k.drawing.map(Shown::Drawing),
        Some(_) => None,
        None if app.session.active_component != 0 => Some(Shown::Part(app.session.active_component)),
        None => Some(Shown::Model),
    }
}

/// Whether the strip is shown (there is something to switch to), and what it marks.
fn state(app: &SolveApp) -> Option<Shown> {
    let now = shown(app)?;
    (now != Shown::Model || !app.session.doc.drawings.is_empty()).then_some(now)
}

/// The part with a tab: the active component, else the one whose tab is kept while a drawing
/// is shown.
fn part(app: &SolveApp) -> Option<u64> {
    let active = app.session.active_component;
    if active != 0 {
        return Some(active);
    }
    BEFORE.with(|b| b.borrow().as_ref().filter(|b| b.tab == app.docs.active).map(|b| b.component))
}

fn close_drawing(app: &mut SolveApp) {
    if matches!(app.dialog.as_ref().map(|d| &d.kind), Some(Kind::Drawing(_))) {
        crate::dialogs::cancel(app);
    }
}

/// Show the model: the drawing editor closes (its changes are already in the design), a part's
/// tab gives the assembly back.
pub(crate) fn show_model(app: &mut SolveApp) {
    close_drawing(app);
    leave_part(app);
}

/// The drawing the drawing editor shows, when it is open.
pub(crate) fn open_drawing(app: &SolveApp) -> Option<u64> {
    match app.dialog.as_ref().map(|d| &d.kind) {
        Some(Kind::Drawing(k)) => k.drawing,
        _ => None,
    }
}

/// Show drawing `id`: the drawing editor opens on it, or turns to it when it is open.
pub(crate) fn show_drawing(app: &mut SolveApp, id: u64) {
    if !app.session.doc.drawings.iter().any(|d| d.id == id) {
        return;
    }
    if !matches!(app.dialog.as_ref().map(|d| &d.kind), Some(Kind::Drawing(_))) {
        if app.dialog.is_some() {
            return;
        }
        app.start("drawing.edit");
    }
    if let Some(Kind::Drawing(k)) = app.dialog.as_mut().map(|d| &mut d.kind) {
        crate::dialogs_drawing::turn_to(k, id);
    }
}

/// Open `component` in a tab of its own: it becomes the active component (its own history and
/// commands) and its placement (`path`, else its first) is isolated. Its changes are the
/// assembly's: it is the same design.
pub(crate) fn open_part(app: &mut SolveApp, component: u64, path: Option<Vec<u64>>) {
    if component == 0 || !app.session.doc.components.iter().any(|c| c.id == component) {
        return;
    }
    close_drawing(app);
    if app.dialog.is_some() {
        return;
    }
    // Another part's tab gives back what it hid first.
    if part(app).is_some_and(|p| p != component) {
        leave_part(app);
    }
    let path = path.filter(|p| !p.is_empty()).unwrap_or_else(|| app.session.doc.first_path(component));
    let mine = BEFORE.with(|b| b.borrow().as_ref().is_some_and(|b| b.tab == app.docs.active && b.component == component));
    if !mine {
        let before = Before {
            tab: app.docs.active,
            component,
            occurrences: app.session.visibility.hidden_occurrences.clone(),
            bodies: app.session.visibility.hidden_bodies.clone(),
        };
        BEFORE.with(|b| *b.borrow_mut() = Some(before));
    }
    // as the browser's Activate does: the component, its placement and the path to it
    let mut p = json!({"component": component});
    if let Some(occurrence) = path.last() {
        p = json!({"component": component, "occurrence": occurrence, "path": path});
    }
    if let Err(e) = app.run("component.activate", p) {
        app.set_status(e, true);
        return;
    }
    if !path.is_empty() {
        let _ = app.run("browser.visibility", json!({"visible": true, "paths": [path], "isolate": true}));
    }
    app.fit_view();
}

/// Leave a part's tab: the assembly is active again and what was shown before the tab is
/// shown again (what was hidden then stays hidden).
pub(crate) fn leave_part(app: &mut SolveApp) {
    let before = BEFORE.with(|b| b.borrow_mut().take()).filter(|b| b.tab == app.docs.active);
    if app.session.active_component == 0 && before.is_none() {
        return;
    }
    if app.session.active_component != 0 {
        let _ = app.run("component.activate", json!({"component": 0}));
    }
    let Some(before) = before else { return };
    let v = &app.session.visibility;
    let show_occ: Vec<Vec<u64>> = v.hidden_occurrences.iter().filter(|p| !before.occurrences.contains(p)).cloned().collect();
    let hide_occ: Vec<Vec<u64>> = before.occurrences.iter().filter(|p| !v.hidden_occurrences.contains(p)).cloned().collect();
    let show_bodies: Vec<String> = v.hidden_bodies.iter().filter(|b| !before.bodies.contains(b)).cloned().collect();
    let hide_bodies: Vec<String> = before.bodies.iter().filter(|b| !v.hidden_bodies.contains(b)).cloned().collect();
    for (paths, visible) in [(show_occ, true), (hide_occ, false)] {
        if !paths.is_empty() {
            let _ = app.run("browser.visibility", json!({"visible": visible, "paths": paths}));
        }
    }
    for (items, visible) in [(show_bodies, true), (hide_bodies, false)] {
        if !items.is_empty() {
            let _ = app.run("browser.visibility", json!({"visible": visible, "items": items}));
        }
    }
    app.fit_view();
}

/// The strip in the free area at the bottom of the viewport (`free`, screen points).
pub(crate) fn show(app: &mut SolveApp, ui: &mut egui::Ui, free: Rect) {
    let Some(current) = state(app) else { return };
    let t = crate::theme::Tokens::get();
    let drawings: Vec<(u64, String)> = app.session.doc.drawings.iter().map(|d| (d.id, d.name.clone())).collect();
    let part = part(app).and_then(|id| app.session.doc.components.iter().find(|c| c.id == id).map(|c| (id, c.name.clone())));
    let mut go: Option<Shown> = None;
    let mut new = false;
    let container = Container::new(ui.ctx(), Surface::ViewportStatus);
    let result = container
        .area(egui::Id::new("view_tabs"))
        .order(egui::Order::Middle)
        .movable(false)
        .pivot(egui::Align2::LEFT_BOTTOM)
        .fixed_pos(free.left_bottom() + egui::vec2(t.space_12, -t.space_8))
        .show(ui.ctx(), |ui| {
            let frame = container.frame(ui.ctx(), t.overlay, crate::frame::Side::Left).inner_margin(t.space_4);
            container.show_frame(ui, frame, free, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = t.space_4;
                    let mut tab = |ui: &mut egui::Ui, label: &str, tip: &str, me: Shown| {
                        let on = current == me;
                        if ui_kit::button(ui, label, if on { Button::SmallSelected } else { Button::Small })
                            .on_hover_text(crate::i18n::tr_label(tip))
                            .clicked()
                            && !on
                        {
                            go = Some(me);
                        }
                    };
                    tab(ui, crate::i18n::tr_label("Model"), "Show the model", Shown::Model);
                    if let Some((id, name)) = &part {
                        tab(ui, name, "Show this part on its own: its changes are the assembly's", Shown::Part(*id));
                    }
                    for (id, name) in &drawings {
                        tab(ui, name, "Show this drawing: it follows every change of the model", Shown::Drawing(*id));
                    }
                    if ui_kit::button(ui, "+", Button::Small).on_hover_text(crate::i18n::tr_label("New drawing…")).clicked() {
                        new = true;
                    }
                });
            });
        });
    crate::scenario::publish_target("view_tabs", result.response.rect);
    if new {
        close_drawing(app);
        if app.dialog.is_none() {
            app.start("drawing.create");
        }
        return;
    }
    match go {
        Some(Shown::Model) => show_model(app),
        Some(Shown::Part(id)) => {
            close_drawing(app);
            if app.session.active_component != id {
                open_part(app, id, None);
            }
        }
        Some(Shown::Drawing(id)) => show_drawing(app, id),
        None => {}
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::Shown;

    #[test]
    fn the_tabs_switch_between_the_model_and_its_drawings_on_the_same_design() {
        let mut h = crate::scenario::Harness::new();
        h.app.run("solid.box", json!({"length": 100, "width": 50, "height": 20})).unwrap();
        // No drawing yet: no strip.
        assert!(super::state(&h.app).is_none());
        let a = h.app.run("drawing.create", json!({"views": "none"})).unwrap()["drawing"].as_u64().unwrap();
        let b = h.app.run("drawing.create", json!({"views": "none"})).unwrap()["drawing"].as_u64().unwrap();
        h.frames(2);
        assert_eq!(super::state(&h.app), Some(Shown::Model), "the model is shown");
        super::show_drawing(&mut h.app, a);
        h.frames(2);
        assert_eq!(super::shown(&h.app), Some(Shown::Drawing(a)), "the first drawing's tab shows it");
        super::show_drawing(&mut h.app, b);
        h.frames(2);
        assert_eq!(super::shown(&h.app), Some(Shown::Drawing(b)), "another tab turns the open editor to its drawing");
        super::show_model(&mut h.app);
        h.frames(1);
        assert_eq!(super::shown(&h.app), Some(Shown::Model), "Model closes the editor");
        assert_eq!(h.app.session.doc.drawings.len(), 2, "closing a drawing's tab keeps the drawing");
        // A change of the model while its drawing is shown: the drawing draws the new model.
        super::show_drawing(&mut h.app, a);
        h.app.run("drawing.view.base", json!({"drawing": a, "orientation": "front", "at": [120, 200]})).unwrap();
        let before = h.app.session.execute("drawing.render", &json!({"drawing": a})).unwrap().to_string();
        super::show_model(&mut h.app);
        // a henger kilóg a hasáb tetején: elölnézetben is látszik
        h.app.run("solid.cylinder", json!({"diameter": 20, "height": 30, "base": [50, 25, 15]})).unwrap();
        super::show_drawing(&mut h.app, a);
        let after = h.app.session.execute("drawing.render", &json!({"drawing": a})).unwrap().to_string();
        assert_ne!(before, after, "the drawing follows the model");
    }

    #[test]
    fn a_part_tab_isolates_the_part_and_its_changes_stay_in_the_assembly() {
        let mut h = crate::scenario::Harness::new();
        let first = h.app.run("component.create", json!({"name": "Tarto"})).unwrap()["component"].as_u64().unwrap();
        h.app.run("solid.box", json!({"length": 40, "width": 20, "height": 10})).unwrap();
        h.app.run("component.activate", json!({"component": 0})).unwrap();
        let second = h.app.run("component.create", json!({"name": "Lap"})).unwrap()["component"].as_u64().unwrap();
        h.app.run("solid.box", json!({"length": 30, "width": 30, "height": 3, "base": [0, 0, 20]})).unwrap();
        h.app.run("component.activate", json!({"component": 0})).unwrap();
        let hidden_before = h.app.session.visibility.clone();
        let bodies_before = h.app.session.world_state().bodies.len();

        super::open_part(&mut h.app, first, None);
        h.frames(1);
        assert_eq!(super::shown(&h.app), Some(Shown::Part(first)), "the part's tab is shown");
        assert_eq!(h.app.session.active_component, first, "the part is active: its own history");
        assert!(!h.app.session.visibility.hidden_occurrences.is_empty(), "the other part is hidden");
        // A change made on the part's tab.
        h.app.run("solid.cylinder", json!({"diameter": 8, "height": 10, "base": [20, 10, 10]})).unwrap();

        super::show_model(&mut h.app);
        h.frames(1);
        assert_eq!(super::shown(&h.app), Some(Shown::Model));
        assert_eq!(h.app.session.active_component, 0, "the assembly is active again");
        assert_eq!(h.app.session.visibility.hidden_occurrences, hidden_before.hidden_occurrences, "what was shown is shown again");
        assert_eq!(h.app.session.visibility.hidden_bodies, hidden_before.hidden_bodies);
        assert!(h.app.session.world_state().bodies.len() > bodies_before, "the change is in the assembly");
        let _ = second;
    }

    #[test]
    fn another_dialog_hides_the_tabs() {
        let mut h = crate::scenario::Harness::new();
        h.app.run("solid.box", json!({"length": 10, "width": 10, "height": 10})).unwrap();
        h.app.run("drawing.create", json!({"views": "none"})).unwrap();
        assert!(super::state(&h.app).is_some());
        h.app.start("solid.box");
        if h.app.dialog.is_some() {
            assert!(super::state(&h.app).is_none(), "a half-made feature keeps the screen");
        }
    }

    /// The design's drawings are in the browser too: a click on one opens it, the open one marked.
    #[test]
    fn the_browser_lists_the_drawings_and_opens_one_on_a_click() {
        let mut h = crate::scenario::Harness::new();
        h.app.run("solid.box", json!({"length": 100, "width": 50, "height": 20})).unwrap();
        h.settle();
        assert!(h.step(&json!({"click": {"handle": "row:Drawings"}})).is_err(), "no drawings: no folder");
        h.app.run("drawing.create", json!({"name": "Front sheet", "views": "none"})).unwrap();
        let b = h.app.run("drawing.create", json!({"name": "Detail sheet", "views": "none"})).unwrap()["drawing"].as_u64().unwrap();
        h.settle();
        h.step(&json!({"click": {"handle": "row:Detail sheet"}})).unwrap();
        h.frames(2);
        assert_eq!(super::shown(&h.app), Some(Shown::Drawing(b)), "the drawing's row opens it");
        assert_eq!(super::open_drawing(&h.app), Some(b));
    }
}
