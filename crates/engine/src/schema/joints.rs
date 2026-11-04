//! Contracts and result types for the commands in `cmd/joints.rs`.

use std::borrow::Cow;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

use super::types::{AngleExpr, IdOrName, LengthExpr, present};

/// A joint origin as joint.create takes it: a joint origin's name, or an object with the snap
/// (`occurrence`, then `face`, `circle`, `edge` or `point`, or a `frame`). Kept as given, for the
/// handler, which reads it as it reads joint.create's sides.
#[derive(Clone, Debug, PartialEq)]
pub struct JointOriginParam(pub Value);

impl<'de> Deserialize<'de> for JointOriginParam {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        match Value::deserialize(d)? {
            v @ (Value::String(_) | Value::Object(_)) => Ok(JointOriginParam(v)),
            _ => Err(D::Error::custom("must be a joint origin's name or an object")),
        }
    }
}

impl JsonSchema for JointOriginParam {
    fn schema_name() -> Cow<'static, str> {
        "JointOrigin".into()
    }
    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "description": "A joint origin, as joint.create takes a side: a joint origin's name, or an object with occurrence and face, circle, edge or point (world coordinates, or a face's name), with z?, x?, end?, on?, corner?, or with a frame {origin, z, x?}.",
            "anyOf": [
                {"type": "string", "minLength": 1},
                {"type": "object"}
            ]
        })
    }
}

/// Edit a joint in place. The sides `a` and `b` take a joint origin as joint.create does; a side
/// left out keeps its place, and the two must stay on different occurrences. The joint re-solves
/// and keeps its limits, its drive and the expressions it follows; a relation's values start
/// again where its parts are when its sides, type, offset, angle or flip change.
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct EditJointParams {
    /// The joint, by id or name.
    pub joint: IdOrName,
    /// Side a: a joint origin, as joint.create takes it. Default: a keeps its place.
    #[serde(default, deserialize_with = "present")]
    pub a: Option<JointOriginParam>,
    /// Side b: a joint origin, as joint.create takes it. Default: b keeps its place.
    #[serde(default, deserialize_with = "present")]
    pub b: Option<JointOriginParam>,
    /// The joint's type, as joint.create takes it (rigid, revolute, slider, ...).
    #[serde(default, rename = "type", deserialize_with = "present")]
    pub kind: Option<String>,
    /// Whether b's z lies along a's instead of against it, as joint.create's `flip`.
    #[serde(default, deserialize_with = "present")]
    pub flip: Option<bool>,
    /// The distance from a's frame to b's along z: a length, or an expression the joint follows.
    #[serde(default, deserialize_with = "present")]
    pub offset: Option<LengthExpr>,
    /// The turn about that distance: an angle, or an expression the joint follows.
    #[serde(default, deserialize_with = "present")]
    pub angle: Option<AngleExpr>,
    /// Suppress the joint (true) or bring it back (false).
    #[serde(default, deserialize_with = "present")]
    pub suppressed: Option<bool>,
    /// The joint's new name.
    #[serde(default, deserialize_with = "present")]
    pub name: Option<String>,
    /// Leave out the degrees of freedom and the fixed occurrences in the reply.
    #[serde(default, deserialize_with = "present")]
    pub quiet: Option<bool>,
}

/// A joint the solve reports: its id, name and why.
#[derive(Debug, Serialize, JsonSchema)]
pub struct JointProblem {
    pub joint: u64,
    pub name: String,
    pub why: String,
}

/// The degrees of freedom one occurrence keeps (6 free, 0 fixed).
#[derive(Debug, Serialize, JsonSchema)]
pub struct OccurrenceDof {
    pub occurrence: u64,
    pub dof: usize,
}

/// What `joint.edit` returns: the joints that disagree with the solved placement, and those the
/// solve could not apply. Unless `quiet`, also each occurrence's degrees of freedom and the
/// occurrences fixed in place.
#[derive(Debug, Serialize, JsonSchema)]
pub struct EditJointResult {
    pub conflicts: Vec<JointProblem>,
    pub errors: Vec<JointProblem>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dof: Option<Vec<OccurrenceDof>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fixed: Option<Vec<u64>>,
    /// In the warn parameter-check mode: what the call's parameters got wrong.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub param_warnings: Vec<String>,
}

/// Which assembly constraint `joint.constrain` places, as a designer names it.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ConstraintType {
    /// Two faces against each other (or an axis on an axis, a point or an axis on a face).
    Mate,
    /// Two faces side by side, pointing the same way.
    Flush,
    /// Two faces (or two axes) at an angle.
    Angle,
    /// A cylinder touching a face.
    Tangent,
    /// A round edge into a round edge: on one axis, their faces against each other.
    Insert,
}

/// How a constraint meets: faces against each other or the same way; a cylinder outside or
/// inside the face it touches.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ConstraintSolution {
    Opposed,
    Aligned,
    Outside,
    Inside,
}

/// Place an assembly constraint between two parts by what was picked on them, as in a classic
/// assembly: a face (planar or round), a round edge (`circle`), a straight edge or a vertex
/// (`point`), each a world point on the part. The constraint is a joint (`joint.list` lists it,
/// `joint.edit` and `joint.delete` work on it); the part moves no further than it needs.
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct ConstrainParams {
    #[serde(rename = "type")]
    pub kind: ConstraintType,
    /// The first pick: `{occurrence, face | circle | edge | point: [x, y, z]}` in world coordinates.
    pub a: JointOriginParam,
    /// The second pick, on another part.
    pub b: JointOriginParam,
    /// The gap between the two (mm or an expression). Default: 0.
    #[serde(default)]
    pub offset: Option<LengthExpr>,
    /// An angle constraint's angle between the faces' normals (or the axes). Default: 0.
    #[serde(default)]
    pub angle: Option<AngleExpr>,
    /// opposed (default for mate and insert) | aligned (flush's); a tangent's outside (default) | inside.
    #[serde(default)]
    pub solution: Option<ConstraintSolution>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub quiet: Option<bool>,
}

/// What `joint.constrain` returns: the joint made for the constraint, the joint type it is,
/// and the solve's report as `joint.create` gives it.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ConstrainResult {
    pub joint: u64,
    pub name: String,
    /// The joint type the constraint is made of (planar, cylindrical, revolute, angle, …).
    pub kind: String,
    pub conflicts: Vec<JointProblem>,
    pub errors: Vec<JointProblem>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dof: Option<Vec<OccurrenceDof>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fixed: Option<Vec<u64>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    /// In the warn parameter-check mode: what the call's parameters got wrong.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub param_warnings: Vec<String>,
}

#[cfg(test)]
#[path = "joints_tests.rs"]
mod tests;
