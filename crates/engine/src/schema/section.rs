//! Section clipping contract. Explicit planes use numerical millimetre coordinates.
use super::{AxisDir, LengthExpr, Point3, present};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Plane {
    Name(String),
    Id(u64),
    Explicit { origin: Option<Point3>, normal: AxisDir },
}
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct SectionParams {
    /// Required unless clearing. XY/XZ/YZ, construction name/id, or {origin?,normal}.
    #[serde(default, deserialize_with = "present", skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Plane")]
    pub plane: Option<Plane>,
    /// Signed distance along the original plane normal, in design units or an expression.
    pub offset: Option<LengthExpr>,
    /// World coordinate on a named plane's normal axis; mutually exclusive with offset.
    pub at: Option<LengthExpr>,
    pub flip: Option<bool>,
    pub hatch: Option<bool>,
    pub clear: Option<bool>,
}
#[derive(Serialize, JsonSchema)]
pub struct Section {
    pub origin: [f64; 3],
    pub normal: [f64; 3],
    pub hatch: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub axis: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at: Option<f64>,
}
/// Slice Graphics (F7): the sketch being edited cuts the view; again, the cut goes.
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct SliceParams {
    /// true: slice, false: stop slicing; omitted: the other of the two.
    pub on: Option<bool>,
}
#[derive(Serialize, JsonSchema)]
pub struct SliceResult {
    /// Whether the view is now sliced at the sketch.
    pub sliced: bool,
    pub section: Option<Section>,
}
#[derive(Serialize, JsonSchema)]
pub struct SectionResult {
    pub section: Option<Section>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub param_warnings: Vec<String>,
}
