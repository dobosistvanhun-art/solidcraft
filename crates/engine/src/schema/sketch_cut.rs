//! Contracts and result types for `cmd/sketch_cut.rs`: a sketch saved as a cutting drawing for a
//! laser, plasma or waterjet, its closed contours sorted onto outer and inner layers.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::sketch_ends::SketchTarget;

/// Save a sketch as a cutting DXF: every closed contour once, outer contours (part edges) on one
/// layer and inner ones (holes) on another, in millimetres.
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct CutDxfParams {
    #[serde(default)]
    pub sketch: Option<SketchTarget>,
    /// The file to write (default: the DXF text is returned).
    #[serde(default)]
    pub path: Option<String>,
    /// Also save open ends and curves in no closed contour, on their own layer (default: they are
    /// an error, since a cutter can't follow them as a part edge).
    #[serde(default)]
    pub allow_open: Option<bool>,
}

/// What `sketch.export_cut_dxf` returns.
#[derive(Debug, Serialize, JsonSchema)]
pub struct CutDxf {
    /// The DXF text, when no `path` was given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dxf: Option<String>,
    /// The file written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Closed contours on the OUTER layer: part edges (inside an even number of other contours).
    pub outer: usize,
    /// Closed contours on the INNER layer: holes (inside an odd number of other contours).
    pub inner: usize,
    /// Profile curve ends that touch no other curve.
    pub open_ends: usize,
    /// Profile curves in no closed contour (on the OPEN layer with `allow_open`, else none saved).
    pub open_curves: usize,
    /// In the warn parameter-check mode: what the call's parameters got wrong.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub param_warnings: Vec<String>,
}
