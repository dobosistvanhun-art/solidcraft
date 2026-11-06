//! Section Analysis: a view-only cut through the model by a plane (what is on the plane's
//! positive side is hidden). It changes no geometry and is not an undo step.

use serde_json::Value;
use solidcraft_geom::{Plane, Vec3};

use super::CommandSpec;
use crate::params::{bad, bool_, expr, vec3};
use crate::schema::{TypedContract, output_schema_for, section as contract};
use crate::{Result, Session};
const SECTION: TypedContract<contract::SectionParams> = TypedContract::new();
const SLICE: TypedContract<contract::SliceParams> = TypedContract::new();

pub static COMMANDS: &[CommandSpec] = &[
    CommandSpec::new("inspect.section", "Section Analysis", crate::checked!("inspect.section", section))
    .contract(&SECTION).output(output_schema_for::<contract::SectionResult>)
    .at("SOLID", "INSPECT").icon("section").noundo().params(
        "plane: XY|XZ|YZ | construction plane name or id | {origin, normal}; offset?: expr along the plane's normal (XY: +Z, XZ: +Y, YZ: +X); \
         at?: expr, the cut's world coordinate on the named plane's axis (XY: Z, XZ: Y, YZ: X), instead of offset; \
         flip?: bool; hatch?: bool (hatch the cut faces, in each body's colour; default true); or clear: true. Returns the cut's origin and normal, and for a named plane its axis and at",
    ),
    CommandSpec::new("sketch.slice", "Slice Graphics", crate::checked!("sketch.slice", slice))
    .contract(&SLICE).output(output_schema_for::<contract::SliceResult>)
    .at("SKETCH", "REFERENCE").icon("section").key("F7").noundo().enabled(super::in_sketch).params(
        "on?: bool (default: the other of sliced and not). While a sketch is edited, the model in front of its plane is hidden and the cut through it shown \
         hatched, as Inventor's Slice Graphics (F7): draw on the cut, and Intersect takes its outline into the sketch. Leaving the sketch ends it",
    ),
];

/// Slice Graphics: the sketch being edited cuts the view (the side its normal faces, toward you, is hidden).
fn slice(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "sketch.slice";
    let id = s.active_sketch.ok_or_else(|| bad(cmd, "no sketch is being edited"))?;
    let on = bool_(p, "on").unwrap_or(s.slice != Some(id));
    if !on {
        end_slice(s, true);
        return serde_json::to_value(contract::SliceResult { sliced: false, section: None }).map_err(|e| bad(cmd, e.to_string()));
    }
    let plane = s.model.state().sketch(id).map(|ss| ss.plane).ok_or_else(|| bad(cmd, "the sketch is not built"))?;
    // A hair in front of the plane, so that the cut face does not hide the sketch drawn on it.
    let normal = plane.normal();
    let origin = plane.origin + normal * 1e-3;
    s.section = Some((origin, normal));
    s.section_hatch = true;
    s.slice = Some(id);
    s.status_slot(crate::status_items::StatusId::Section, true);
    s.refresh_display();
    let section = contract::Section { origin: [origin.x, origin.y, origin.z], normal: [normal.x, normal.y, normal.z], hatch: true, axis: None, at: None };
    serde_json::to_value(contract::SliceResult { sliced: true, section: Some(section) }).map_err(|e| bad(cmd, e.to_string()))
}

/// The slice goes when its sketch is no longer edited (finished, cancelled, another opened), or
/// when asked (`always`).
pub(crate) fn end_slice(s: &mut Session, always: bool) {
    let Some(id) = s.slice else { return };
    if !always && s.active_sketch == Some(id) {
        return;
    }
    s.slice = None;
    s.section = None;
    s.status_slot(crate::status_items::StatusId::Section, false);
    s.refresh_display();
}

fn section(s: &mut Session, p: &Value) -> Result<Value> {
    let cmd = "inspect.section";
    // A section asked for is the user's own: no sketch's slice any more.
    s.slice = None;
    if bool_(p, "clear").unwrap_or(false) {
        s.section = None;
        s.status_slot(crate::status_items::StatusId::Section, false);
        s.refresh_display();
        return serde_json::to_value(contract::SectionResult { section: None, param_warnings: Vec::new() }).map_err(|e| bad(cmd, e.to_string()));
    }
    // A named plane's world axis: which component of a point lies along its normal.
    let (origin, normal, axis) = match p.get("plane") {
        Some(Value::String(n)) if Plane::named(n.trim()).is_some() => {
            // Apply the same ambiguity check as modelling commands before treating a name as an origin alias.
            super::features::plane_param(s, p.get("plane"), cmd)?;
            let pl = Plane::named(n.trim()).ok_or_else(|| bad(cmd, format!("unknown plane `{n}`")))?;
            let normal = pl.normal();
            let axis = [normal.x, normal.y, normal.z].iter().position(|c| c.abs() > 0.5);
            (pl.origin, normal, axis)
        }
        // A construction plane (name or id): where it is now.
        Some(v @ (Value::String(_) | Value::Number(_))) => {
            let r = super::features::plane_param(s, Some(v), cmd)?;
            let solidcraft_doc::PlaneRef::Construction { feature } = &r else { return Err(bad(cmd, "`plane` must be a plane")) };
            let st = s.model.state();
            let c = st
                .construct
                .iter()
                .find(|c| c.feature == feature.0)
                .ok_or_else(|| bad(cmd, format!("construction plane `{feature}` is not built (rolled back, suppressed or failed)")))?;
            let solidcraft_doc::construct::ConstructGeom::Plane(pl) = &c.geom else {
                return Err(bad(cmd, format!("construction plane `{}` is not built (rolled back, suppressed or failed)", c.name)));
            };
            (pl.origin, pl.normal(), None)
        }
        Some(o @ Value::Object(_)) => {
            let origin = o.get("origin").and_then(vec3).unwrap_or(Vec3::ZERO);
            let n = o.get("normal").and_then(vec3).and_then(|n| n.normalized()).ok_or_else(|| bad(cmd, "the plane needs a non-zero `normal`"))?;
            (origin, n, None)
        }
        _ => return Err(bad(cmd, "`plane` must be XY, XZ, YZ, a construction plane name or id, or {origin, normal}")),
    };
    let length = |key: &str| -> Result<Option<f64>> {
        match expr(p, key) {
            Some(e) => s.doc.eval(&e, solidcraft_doc::expr::Kind::Length).map(Some).map_err(|e| bad(cmd, format!("{key}: {e}"))),
            None => Ok(None),
        }
    };
    let (offset, at) = (length("offset")?, length("at")?);
    let offset = match (offset, at, axis) {
        (Some(_), Some(_), _) => return Err(bad(cmd, "give either `offset` (along the normal) or `at` (a world coordinate), not both")),
        (_, Some(_), None) => return Err(bad(cmd, "`at` needs a named plane (XY, XZ, YZ); move an {origin, normal} plane by its origin")),
        // The named planes pass through the world origin, so the offset is `at` signed by the normal.
        (_, Some(at), Some(i)) => at * axis_component(normal, i).signum(),
        (offset, None, _) => offset.unwrap_or(0.0),
    };
    if !origin.is_finite() || !offset.is_finite() {
        return Err(bad(cmd, "the plane must be finite"));
    }
    let normal = if bool_(p, "flip").unwrap_or(false) { -normal } else { normal };
    let origin = origin + normal * offset * if bool_(p, "flip").unwrap_or(false) { -1.0 } else { 1.0 };
    s.section = Some((origin, normal));
    s.section_hatch = bool_(p, "hatch").unwrap_or(true);
    s.status_slot(crate::status_items::StatusId::Section, true);
    s.status_activated(crate::status_items::StatusId::Section);
    s.refresh_display();
    let section = contract::Section {
        origin: [origin.x, origin.y, origin.z],
        normal: [normal.x, normal.y, normal.z],
        hatch: s.section_hatch,
        axis: axis.and_then(|i| ["X", "Y", "Z"].get(i).map(|v| (*v).to_owned())),
        at: axis.map(|i| axis_component(origin, i)),
    };
    serde_json::to_value(contract::SectionResult { section: Some(section), param_warnings: Vec::new() }).map_err(|e| bad(cmd, e.to_string()))
}

fn axis_component(v: Vec3, i: usize) -> f64 {
    match i {
        0 => v.x,
        1 => v.y,
        _ => v.z,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::Session;

    fn run(s: &mut Session, id: &str, p: serde_json::Value) -> serde_json::Value {
        s.execute(id, &p).unwrap_or_else(|e| panic!("{id}: {e}"))
    }

    /// Slice Graphics (F7): a sketch inside a block cuts the view at its plane, F7 again takes the
    /// cut off, and leaving the sketch ends it; a section asked for stays.
    #[test]
    fn slice_cuts_the_view_at_the_sketch_until_the_sketch_is_left() {
        let mut s = Session::default();
        run(&mut s, "solid.box", json!({"corner": [0, 0, 0], "length": 40, "width": 30, "height": 20}));
        run(&mut s, "sketch.create", json!({"plane": {"origin": [0, 0, 10], "x_dir": [1, 0, 0], "y_dir": [0, 1, 0]}}));
        let r = run(&mut s, "sketch.slice", json!({}));
        assert_eq!(r["sliced"], true);
        let (o, n) = s.section.unwrap_or_default();
        assert!((o.z - 10.0).abs() < 0.01 && (n.z - 1.0).abs() < 1e-9, "cut at the sketch plane, its front hidden: {o:?} {n:?}");
        assert_eq!(run(&mut s, "sketch.slice", json!({}))["sliced"], false);
        assert!(s.section.is_none());
        run(&mut s, "sketch.slice", json!({"on": true}));
        run(&mut s, "sketch.circle.center", json!({"center": [20, 15], "radius": 5}));
        assert!(s.section.is_some(), "drawing keeps the slice");
        run(&mut s, "sketch.finish", json!({}));
        assert!(s.section.is_none() && s.slice.is_none(), "finishing the sketch ends the slice");
        // Outside a sketch there is nothing to slice by; a section of one's own is not ended.
        assert!(s.execute("sketch.slice", &json!({})).is_err());
        run(&mut s, "inspect.section", json!({"plane": "XY", "offset": 5}));
        run(&mut s, "sketch.create", json!({"plane": "XY"}));
        run(&mut s, "sketch.finish", json!({}));
        assert!(s.section.is_some(), "an own section stays");
    }
}
