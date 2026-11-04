//! `joint.constrain`: assembly constraints placed by what is picked on two parts, the way a
//! classic assembly places them: Mate, Flush, Angle, Tangent and Insert. Each is made of a joint
//! (so it moves, drives, lists and edits as any joint), chosen by what the two picks are:
//!
//! | Constraint | Picks | Joint |
//! |---|---|---|
//! | Mate | face, face | planar, faces against each other (`offset` apart) |
//! | Mate | round, round | cylindrical: one axis |
//! | Mate | face, point | point on plane |
//! | Mate | face, round | line on plane: the axis in the face |
//! | Mate | round, point | point on line |
//! | Flush | face, face | planar, faces the same way |
//! | Angle | face, face (or round, round) | angle between the normals (axes) |
//! | Tangent | face, round | line on plane, the axis a radius off the face |
//! | Insert | round, round | revolute: one axis, the rims' faces against each other |
//!
//! A round pick is a cylindrical face or a round edge. The new joint starts where its parts
//! are, so the moving part goes no further than the constraint needs (a mate does not slide the
//! part to the other face's centre).

use serde_json::{Value, json};
use solidcraft_doc::joints::{self, Joint, JointExprs, JointKind, PickedSurface};

use super::{add_joint, component_of, occurrence_at, occurrence_id, origin_param, typed, value};
use crate::params::{bad, str_, vec3};
use crate::{Result, Session};

const CMD: &str = "joint.constrain";

/// What one pick is.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Pick {
    Face,
    /// A cylindrical face or a round edge; its radius when a face was picked.
    Round {
        radius: Option<f64>,
    },
    Line,
    Point,
}

/// A pick, read: what it is, and the joint origin it gives.
struct Side {
    pick: Pick,
    origin: Value,
}

fn side(s: &Session, v: Option<&Value>, which: &str) -> Result<Side> {
    let v = v
        .filter(|v| v.is_object())
        .ok_or_else(|| bad(CMD, format!("`{which}` must be a pick: {{occurrence, face | circle | edge | point: [x, y, z]}}")))?;
    let key = ["face", "circle", "edge", "point"]
        .into_iter()
        .find(|k| v.get(*k).is_some())
        .ok_or_else(|| bad(CMD, format!("`{which}` needs face, circle, edge or point [x, y, z]")))?;
    let at = v.get(key).and_then(vec3).ok_or_else(|| bad(CMD, format!("`{which}.{key}` must be a world point [x, y, z]")))?;
    let occurrence = match v.get("occurrence") {
        Some(o) => occurrence_id(s, Some(o), CMD)?,
        None => occurrence_at(s, at).ok_or_else(|| bad(CMD, format!("`{which}`: no part there")))?,
    };
    // The pick as joint.create takes a side (its z, corner, end and x kept), on its occurrence.
    let mut origin = v.clone();
    origin["occurrence"] = json!(occurrence);
    // What the pick is: on a cylindrical face (then its radius), on a planar face, or neither.
    let surface = || {
        let t = if occurrence == 0 { solidcraft_doc::IDENTITY } else { joints::occurrence_world(&s.doc, occurrence) };
        let inv = solidcraft_doc::mat_inverse(&t)?;
        joints::surface_at(&s.doc, &s.model.state(), component_of(s, occurrence), solidcraft_doc::apply_point(&inv, at))
    };
    let pick = if v.get("corner").is_some() || key == "point" {
        Pick::Point
    } else {
        match key {
            "circle" => Pick::Round {
                radius: match surface() {
                    Some(PickedSurface::Cylinder { radius, .. }) => Some(radius),
                    _ => None,
                },
            },
            "edge" => Pick::Line,
            _ => match surface() {
                Some(PickedSurface::Plane { .. }) => Pick::Face,
                Some(PickedSurface::Cylinder { radius, .. }) => {
                    // Its axis, through the cylinder's end nearer the pick.
                    if let Some(m) = origin.as_object_mut() {
                        m.remove("face");
                        m.insert("circle".into(), json!([at.x, at.y, at.z]));
                    }
                    Pick::Round { radius: Some(radius) }
                }
                Some(PickedSurface::Other) => {
                    return Err(bad(CMD, format!("`{which}`: pick a planar or a round face, a round edge, a straight edge or a vertex")));
                }
                None => return Err(bad(CMD, format!("`{which}`: no face there"))),
            },
        }
    };
    Ok(Side { pick, origin })
}

/// The joint a constraint is made of: its type, which pick is its A side (`swap`: b's), and
/// whether it lines B's z up with A's.
struct Plan {
    kind: JointKind,
    swap: bool,
    flip: bool,
    /// Added to the offset: a tangent's radius.
    extra: f64,
}

fn plan(ty: &str, solution: Option<&str>, a: Pick, b: Pick, offset: f64) -> Result<Plan> {
    use Pick::{Face, Line, Point, Round};
    let aligned = solution == Some("aligned");
    let join = |kind, swap, flip| Ok(Plan { kind, swap, flip, extra: 0.0 });
    match (ty, a, b) {
        ("mate", Face, Face) => join(JointKind::Planar, false, aligned),
        ("flush", Face, Face) => join(JointKind::Planar, false, solution != Some("opposed")),
        ("mate", Round { .. }, Round { .. }) => {
            if offset != 0.0 {
                return Err(bad(CMD, "a mate of two axes puts them on one line: it takes no offset"));
            }
            join(JointKind::Cylindrical, false, aligned)
        }
        ("mate", Face, Point) | ("mate", Point, Face) => join(JointKind::PointOnPlane, matches!(a, Point), false),
        ("mate", Face, Round { .. }) | ("mate", Round { .. }, Face) => join(JointKind::LineOnPlane, matches!(a, Round { .. }), false),
        ("mate", Face, Line) | ("mate", Line, Face) => join(JointKind::LineOnPlane, matches!(a, Line), false),
        ("mate", Round { .. }, Point) | ("mate", Point, Round { .. }) => join(JointKind::PointOnLine, matches!(a, Point), false),
        ("angle", Face, Face) | ("angle", Round { .. }, Round { .. }) => join(JointKind::Angle, false, false),
        ("tangent", Face, Round { radius }) | ("tangent", Round { radius }, Face) => {
            let r = radius.ok_or_else(|| bad(CMD, "a tangent takes the cylinder's face, not its edge"))?;
            let inside = solution == Some("inside");
            Ok(Plan { kind: JointKind::LineOnPlane, swap: matches!(a, Round { .. }), flip: false, extra: if inside { -r } else { r } })
        }
        ("tangent", Round { .. }, Round { .. }) => Err(bad(CMD, "not supported yet: a tangent between two cylinders")),
        ("insert", Round { .. }, Round { .. }) => join(JointKind::Revolute, false, aligned),
        ("mate", ..) => Err(bad(CMD, "a mate takes two faces, two round faces or edges, or a face or an axis with a point")),
        ("flush", ..) => Err(bad(CMD, "a flush takes two planar faces")),
        ("angle", ..) => Err(bad(CMD, "an angle takes two planar faces or two round ones")),
        ("tangent", ..) => Err(bad(CMD, "a tangent takes a planar face and a round face")),
        ("insert", ..) => Err(bad(CMD, "an insert takes two round edges (or round faces)")),
        _ => Err(bad(CMD, "`type` is mate, flush, angle, tangent or insert")),
    }
}

/// A new constraint's name: its type and the first free number ("Mate:1").
fn name(s: &Session, p: &Value, ty: &str) -> String {
    if let Some(n) = str_(p, "name").filter(|n| !n.trim().is_empty()) {
        return n.to_string();
    }
    let label = match ty {
        "flush" => "Flush",
        "angle" => "Angle",
        "tangent" => "Tangent",
        "insert" => "Insert",
        _ => "Mate",
    };
    let names: std::collections::BTreeSet<&str> = s.doc.assembly.joints.iter().map(|j| j.name.as_str()).collect();
    let n = (1..=s.doc.assembly.joints.len().saturating_add(1)).find(|n| !names.contains(format!("{label}:{n}").as_str())).unwrap_or(1);
    format!("{label}:{n}")
}

pub(super) fn constrain(s: &mut Session, p: &Value) -> Result<Value> {
    let ty = str_(p, "type").ok_or_else(|| bad(CMD, "`type` is mate, flush, angle, tangent or insert"))?.to_ascii_lowercase();
    if !matches!(ty.as_str(), "mate" | "flush" | "angle" | "tangent" | "insert") {
        return Err(bad(CMD, "`type` is mate, flush, angle, tangent or insert"));
    }
    let solution = str_(p, "solution").map(str::to_ascii_lowercase);
    if let Some(sol) = solution.as_deref() {
        let ok = match ty.as_str() {
            "tangent" => matches!(sol, "outside" | "inside"),
            "angle" => false,
            _ => matches!(sol, "opposed" | "aligned"),
        };
        if !ok {
            return Err(bad(CMD, format!("a {ty} takes no solution `{sol}` (mate, flush and insert: opposed | aligned; tangent: outside | inside)")));
        }
    }
    let offset = match p.get("offset") {
        Some(v) => value(s, v, false, CMD)?,
        None => 0.0,
    };
    let angle = match p.get("angle") {
        Some(_) if ty != "angle" => return Err(bad(CMD, "`angle` is for an angle constraint")),
        Some(v) => value(s, v, true, CMD)?,
        None => 0.0,
    };
    let (a, b) = (side(s, p.get("a"), "a")?, side(s, p.get("b"), "b")?);
    let plan = plan(&ty, solution.as_deref(), a.pick, b.pick, offset)?;
    let (a, b) = if plan.swap { (b, a) } else { (a, b) };
    let oa = origin_param(s, Some(&a.origin), CMD)?;
    let ob = origin_param(s, Some(&b.origin), CMD)?;
    if oa.occurrence == ob.occurrence {
        return Err(bad(CMD, "pick the two on different parts"));
    }
    let id = s.doc_mut().allocate_id()?;
    let name = name(s, p, &ty);
    let mut j = Joint {
        id,
        name,
        kind: plan.kind,
        a: oa,
        b: ob,
        values: vec![0.0; plan.kind.dofs().len()],
        limits: vec![None; plan.kind.dofs().len()],
        // An angle is measured between the normals: 0 lines them up.
        flip: plan.flip || plan.kind == JointKind::Angle,
        offset: offset + plan.extra,
        angle,
        suppressed: false,
        exprs: JointExprs { offset: if plan.extra == 0.0 { typed(p.get("offset")) } else { None }, angle: typed(p.get("angle")), values: Vec::new() },
    };
    if !j.kind.is_relation() {
        j.angle = joints::least_turn_angle(&s.doc, &s.model.state(), &j);
    }
    // Where the parts are: the part moves no further than the constraint needs.
    if let Some(q) = joints::relation_values(&s.doc, &s.model.state(), &j) {
        j.values = q;
    }
    let kind = format!("{:?}", j.kind);
    let mut out = add_joint(s, j, &json!({"values": [], "quiet": p.get("quiet").cloned().unwrap_or(Value::Bool(false))}))?;
    out["kind"] = json!(snake(&kind));
    Ok(out)
}

/// `PointOnPlane` → `point_on_plane`, as `joint.create` names the types.
fn snake(t: &str) -> String {
    let mut out = String::new();
    for (i, c) in t.chars().enumerate() {
        if c.is_ascii_uppercase() && i > 0 {
            out.push('_');
        }
        out.push(c.to_ascii_lowercase());
    }
    out
}
