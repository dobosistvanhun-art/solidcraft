//! Contracts and result types for `cmd/nest.rs`: the design's nesting job, its automatic layout,
//! hand edits of it and its DXF.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::types::IdOrName;

/// A part of the job: a sketch (its closed contours) or a sheet metal body (its flat pattern).
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct NestPartParam {
    /// The sketch, by id or name.
    #[serde(default)]
    pub sketch: Option<IdOrName>,
    /// The sheet metal body, by name.
    #[serde(default)]
    pub body: Option<String>,
    /// How many copies (default 1).
    #[serde(default)]
    pub count: Option<u32>,
    /// It has a grain direction: it turns only by a half turn.
    #[serde(default)]
    pub grain: Option<bool>,
    /// It may not turn at all.
    #[serde(default)]
    pub fixed_angle: Option<bool>,
}

/// A change to one part of the job's list.
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct NestPartEdit {
    /// The part's index, as `nest.info` lists the parts.
    pub index: usize,
    #[serde(default)]
    pub count: Option<u32>,
    #[serde(default)]
    pub grain: Option<bool>,
    #[serde(default)]
    pub fixed_angle: Option<bool>,
    /// Take it out of the job (its copies leave the layout).
    #[serde(default)]
    pub remove: Option<bool>,
}

/// Bring a DXF's closed contours into the job as parts, kept with it.
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct NestImportParams {
    /// The DXF file.
    #[serde(default)]
    pub path: Option<String>,
    /// The DXF itself, instead of a file.
    #[serde(default)]
    pub text: Option<String>,
    /// The part's name (default: the file's).
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub count: Option<u32>,
    #[serde(default)]
    pub grain: Option<bool>,
    #[serde(default)]
    pub fixed_angle: Option<bool>,
    /// Each outer contour a part of its own.
    #[serde(default)]
    pub split: Option<bool>,
    /// How near two ends must be to join (mm, default 0.01).
    #[serde(default)]
    pub tolerance: Option<f64>,
}

/// The material sheet (mm).
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct NestSheetParam {
    pub width: f64,
    pub height: f64,
    /// How far parts stay from the sheet's edges (default 5).
    #[serde(default)]
    pub margin: Option<f64>,
}

/// How hard the automatic layout tries.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum NestQualityParam {
    Fast,
    Normal,
    Precise,
}

/// Set up the design's nesting job: the parts and their counts, the sheet, the gap, the cut
/// width and the quality. Changing the parts drops the layout; the other settings keep it.
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct NestSetupParams {
    /// The parts, replacing the job's list.
    #[serde(default)]
    pub parts: Option<Vec<NestPartParam>>,
    /// Parts added to the job's list (its layout kept).
    #[serde(default)]
    pub add: Option<Vec<NestPartParam>>,
    /// One part of the list changed, or taken out.
    #[serde(default)]
    pub edit: Option<NestPartEdit>,
    #[serde(default)]
    pub sheet: Option<NestSheetParam>,
    /// The least distance between parts (mm).
    #[serde(default)]
    pub gap: Option<f64>,
    /// The laser's cut width (mm): parts also stay this much further apart.
    #[serde(default)]
    pub kerf: Option<f64>,
    #[serde(default)]
    pub quality: Option<NestQualityParam>,
    /// Remove the job from the design.
    #[serde(default)]
    pub clear: Option<bool>,
}

/// Lay the job's parts out on sheets automatically (locked copies stay where they are), or take
/// one of the other layouts the last run found.
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct NestRunParams {
    /// Use the last run's layout with this index (0 is the best) instead of running again.
    #[serde(default)]
    pub variant: Option<usize>,
    #[serde(default)]
    pub quality: Option<NestQualityParam>,
    /// The `layouts` a run answered, kept as they are (the first in use) instead of running.
    #[serde(default)]
    pub apply: Option<Value>,
}

/// Move, turn, lock or remove one placed copy by hand. The reply says whether it now comes
/// too close to another or past the margin.
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct NestEditParams {
    /// The placement's index, as `nest.info` lists it.
    pub placement: usize,
    /// Where its centre goes on its sheet (mm).
    #[serde(default)]
    pub x: Option<f64>,
    #[serde(default)]
    pub y: Option<f64>,
    /// Its turn (degrees, counter-clockwise).
    #[serde(default)]
    pub angle: Option<f64>,
    /// The sheet it goes on.
    #[serde(default)]
    pub sheet: Option<usize>,
    #[serde(default)]
    pub locked: Option<bool>,
    /// Take it out of the layout.
    #[serde(default)]
    pub remove: Option<bool>,
}

/// The nesting job: its settings, parts, placements with their outlines, and what it uses.
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct NestInfoParams {
    /// Leave the outlines out (default false).
    #[serde(default)]
    pub brief: Option<bool>,
}

/// Save a sheet of the layout as a cutting DXF: each part where it lies, its outer contours on
/// OUTER and holes on INNER, in millimetres.
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct NestExportParams {
    /// The sheet (default 0).
    #[serde(default)]
    pub sheet: Option<usize>,
    /// Every sheet, each to its own file (`path` with -1, -2… before the extension).
    #[serde(default)]
    pub all: Option<bool>,
    /// The file to write (else the DXF text is returned).
    #[serde(default)]
    pub path: Option<String>,
    /// Also draw the sheet's outline, on layer SHEET (default false: a cutter would cut it).
    #[serde(default)]
    pub sheet_outline: Option<bool>,
}

/// What the nest commands return: a JSON object as each one describes.
#[derive(Debug, Serialize, JsonSchema)]
pub struct NestReply {
    #[serde(flatten)]
    pub fields: serde_json::Map<String, Value>,
    /// In the warn parameter-check mode: what the call's parameters got wrong.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub param_warnings: Vec<String>,
}
