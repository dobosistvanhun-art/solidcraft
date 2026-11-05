//! Contracts and result types for `cmd/sketch_cut.rs`: a sketch saved as a cutting drawing for a
//! laser, plasma or waterjet, its closed contours sorted onto outer and inner layers.

use std::borrow::Cow;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

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

/// A cutting colour: a LightBurn layer colour by name ("red", "light_gray") or number (0 to 29),
/// or null / "none" for none.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CutColour(pub Option<u8>);

impl CutColour {
    /// The colour a value names, or what is wrong with it.
    pub fn read(v: &Value) -> Result<CutColour, String> {
        let bad = || {
            format!(
                "must be a colour: a name ({}), a number 0 to 29, or none",
                solidcraft_sketch::cut_colour::PALETTE[..17].iter().map(|(n, _)| *n).collect::<Vec<_>>().join(", ")
            )
        };
        match v {
            Value::Null => Ok(CutColour(None)),
            Value::String(s) if s.trim().eq_ignore_ascii_case("none") => Ok(CutColour(None)),
            Value::String(s) => solidcraft_sketch::cut_colour::parse(s).map(|c| CutColour(Some(c))).ok_or_else(bad),
            Value::Number(n) => n.as_u64().and_then(|i| u8::try_from(i).ok()).filter(|i| *i < 30).map(|c| CutColour(Some(c))).ok_or_else(bad),
            _ => Err(bad()),
        }
    }
}

impl<'de> Deserialize<'de> for CutColour {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        CutColour::read(&Value::deserialize(d)?).map_err(D::Error::custom)
    }
}

impl JsonSchema for CutColour {
    fn schema_name() -> Cow<'static, str> {
        "CutColour".into()
    }
    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "description": "A LightBurn layer colour: a name (black, blue, red, green, yellow, orange, cyan, magenta, light_gray, dark_blue, dark_red, dark_green, olive, brown, sky_blue, purple, gray), a number 0 to 29, or null / \"none\".",
            "anyOf": [{"type": "integer", "minimum": 0, "maximum": 29}, {"type": "string"}, {"type": "null"}]
        })
    }
}

/// Give closed contours of a sketch their cutting colour: each listed curve's whole contour (or
/// only the curves, with `contour: false`). The cutting and nesting DXFs carry it, so a laser's
/// software (LightBurn) puts the contour on the layer of that colour.
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct CutColourParams {
    #[serde(default)]
    pub sketch: Option<SketchTarget>,
    /// Curves of the contours to colour.
    pub curves: Vec<String>,
    pub colour: CutColour,
    /// Colour the whole contour each curve is in (default true).
    #[serde(default)]
    pub contour: Option<bool>,
}

/// What `sketch.cut_colour` returns.
#[derive(Debug, Serialize, JsonSchema)]
pub struct CutColourResult {
    /// The curves that took the colour.
    pub curves: Vec<String>,
    /// The colour's name, or none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub colour: Option<String>,
    /// The sketch's solve report.
    pub sketch: Value,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub param_warnings: Vec<String>,
}
