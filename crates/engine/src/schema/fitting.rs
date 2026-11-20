//! Contracts and result types for the pipe fitting commands in `cmd/fitting.rs`: a tee, a cone
//! (concentric or eccentric, with or without a branch into its mantle), a mitred elbow, a socket
//! and a collar, made of sheet whose pieces have flat patterns, a plate flange (its pattern the
//! part), a threaded socket and a threaded end (turned), and the patterns' DXF export.
//!
//! The diameters are the sheet's outside unless `measured` says otherwise; the flat patterns
//! are worked out on the sheet's middle surface.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::types::{AngleExpr, AxisDir, FeatureCommon, IdOrName, LengthExpr, NumberExpr, PointExpr3, feature_common, present};

/// Which diameter a fitting's sizes give.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum MeasuredParam {
    /// The sheet's outside (the default).
    Outside,
    /// The sheet's inside.
    Inside,
    /// The sheet's middle surface.
    Middle,
}

/// Add a tee: a pipe (or a cone, with `top`) with a branch pipe set into its side. Two bodies:
/// the body with the branch's opening cut, and the branch cut to fit it.
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct TeeParams {
    /// The body's (its bottom's) diameter.
    pub diameter: LengthExpr,
    /// The body's top diameter. Default: `diameter` (a straight pipe).
    #[serde(default)]
    pub top: Option<LengthExpr>,
    /// The body's height, along `axis`.
    pub height: LengthExpr,
    /// How far the top's centre is moved over, across the axis (an eccentric cone). Default 0.
    #[serde(default)]
    pub shift: Option<LengthExpr>,
    /// The branch's diameter, at most the body's where it meets it.
    pub branch: LengthExpr,
    /// The branch's length, from the body's axis to its end. Default: twice the body's larger radius.
    #[serde(default)]
    pub branch_length: Option<LengthExpr>,
    /// The height where the branch's axis meets the body's. Default: half the height.
    #[serde(default)]
    pub branch_at: Option<LengthExpr>,
    /// The branch's angle from the body's axis: 90 deg is square to it. Default 90 deg.
    #[serde(default)]
    pub branch_angle: Option<AngleExpr>,
    /// How far the branch's axis passes beside the body's (off-centre). Default 0.
    #[serde(default)]
    pub branch_offset: Option<LengthExpr>,
    /// The sheet's thickness.
    pub thickness: LengthExpr,
    /// Which diameter the sizes give. Default: outside.
    #[serde(default)]
    pub measured: Option<MeasuredParam>,
    /// The centre of the body's bottom. Default: the origin.
    #[serde(default, deserialize_with = "present", skip_serializing_if = "Option::is_none")]
    #[schemars(with = "PointExpr3")]
    pub base: Option<PointExpr3>,
    /// The body's axis, from its bottom to its top. Default: Z. The branch points along the
    /// fitting's X: the world X, or Y when the axis is along X.
    #[serde(default, deserialize_with = "present", skip_serializing_if = "Option::is_none")]
    #[schemars(with = "AxisDir")]
    pub axis: Option<AxisDir>,
    #[serde(flatten, deserialize_with = "feature_common")]
    pub common: FeatureCommon,
}

/// Add a cone of sheet: concentric, or eccentric with `shift`; with `branch`, a branch pipe set
/// into its mantle (as a tee's).
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct ConeParams {
    /// The bottom diameter.
    pub diameter: LengthExpr,
    /// The top diameter.
    pub top: LengthExpr,
    /// The height, along `axis`.
    pub height: LengthExpr,
    /// How far the top's centre is moved over, across the axis (an eccentric cone; the top's
    /// edge over the bottom's on one side when it is the difference of the radii). Default 0.
    #[serde(default)]
    pub shift: Option<LengthExpr>,
    /// A branch's diameter: a branch pipe into the mantle. Default: none.
    #[serde(default)]
    pub branch: Option<LengthExpr>,
    /// As a tee's.
    #[serde(default)]
    pub branch_length: Option<LengthExpr>,
    /// As a tee's.
    #[serde(default)]
    pub branch_at: Option<LengthExpr>,
    /// As a tee's.
    #[serde(default)]
    pub branch_angle: Option<AngleExpr>,
    /// As a tee's.
    #[serde(default)]
    pub branch_offset: Option<LengthExpr>,
    /// The sheet's thickness.
    pub thickness: LengthExpr,
    /// Which diameter the sizes give. Default: outside.
    #[serde(default)]
    pub measured: Option<MeasuredParam>,
    /// The centre of the bottom. Default: the origin.
    #[serde(default, deserialize_with = "present", skip_serializing_if = "Option::is_none")]
    #[schemars(with = "PointExpr3")]
    pub base: Option<PointExpr3>,
    /// The axis, from the bottom to the top. Default: Z. The shift and the branch go along the
    /// fitting's X: the world X, or Y when the axis is along X.
    #[serde(default, deserialize_with = "present", skip_serializing_if = "Option::is_none")]
    #[schemars(with = "AxisDir")]
    pub axis: Option<AxisDir>,
    #[serde(flatten, deserialize_with = "feature_common")]
    pub common: FeatureCommon,
}

/// Add a mitred elbow: a bend made of straight segments, the two at the ends half ones. One
/// body for each segment.
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct ElbowParams {
    /// The pipe's diameter.
    pub diameter: LengthExpr,
    /// The bend's angle. Default 90 deg.
    #[serde(default)]
    pub angle: Option<AngleExpr>,
    /// The segments, 2 to 48, counting the two half ones at the ends. Default 5.
    #[serde(default)]
    pub segments: Option<NumberExpr>,
    /// The centre line's bend radius, more than the pipe's radius. Default: 1.5 × the diameter.
    #[serde(default)]
    pub bend: Option<LengthExpr>,
    /// A straight length at each end, part of the end segments. Default 0.
    #[serde(default)]
    pub straight: Option<LengthExpr>,
    /// The sheet's thickness.
    pub thickness: LengthExpr,
    /// Which diameter `diameter` is. Default: outside.
    #[serde(default)]
    pub measured: Option<MeasuredParam>,
    /// The centre of the start. Default: the origin.
    #[serde(default, deserialize_with = "present", skip_serializing_if = "Option::is_none")]
    #[schemars(with = "PointExpr3")]
    pub base: Option<PointExpr3>,
    /// The direction the elbow starts in. Default: Z. It bends toward the fitting's X: the
    /// world X, or Y when the axis is along X.
    #[serde(default, deserialize_with = "present", skip_serializing_if = "Option::is_none")]
    #[schemars(with = "AxisDir")]
    pub axis: Option<AxisDir>,
    #[serde(flatten, deserialize_with = "feature_common")]
    pub common: FeatureCommon,
}

/// Add a plate flange (EN 1092-1 type 01 by `pn` and `dn`, or any sizes): a ring of plate with
/// its bolt holes, cut flat (its flat pattern is the part). Blind without a bore.
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct FlangeParams {
    /// The pressure class of the standard sizes: 6, 10 or 16. Default 16.
    #[serde(default)]
    pub pn: Option<u16>,
    /// The nominal size (DN 10…600) of the standard sizes; without it every size is given.
    #[serde(default)]
    pub dn: Option<u16>,
    /// A blind flange: no bore. Default false.
    #[serde(default)]
    pub blind: Option<bool>,
    /// The outside diameter (the standard's, or given).
    #[serde(default)]
    pub diameter: Option<LengthExpr>,
    /// The bore (the standard's: for the pipe's outside, welded in).
    #[serde(default)]
    pub bore: Option<LengthExpr>,
    /// The bolt circle's diameter.
    #[serde(default)]
    pub bolt_circle: Option<LengthExpr>,
    /// The bolt holes' count.
    #[serde(default)]
    pub holes: Option<NumberExpr>,
    /// A bolt hole's diameter.
    #[serde(default)]
    pub hole: Option<LengthExpr>,
    /// The plate's thickness.
    #[serde(default)]
    pub thickness: Option<LengthExpr>,
    /// The centre of the flange's face. Default: the origin.
    #[serde(default, deserialize_with = "present", skip_serializing_if = "Option::is_none")]
    #[schemars(with = "PointExpr3")]
    pub base: Option<PointExpr3>,
    /// The flange's axis (its plate square to it, its thickness along it). Default: Z.
    #[serde(default, deserialize_with = "present", skip_serializing_if = "Option::is_none")]
    #[schemars(with = "AxisDir")]
    pub axis: Option<AxisDir>,
    #[serde(flatten, deserialize_with = "feature_common")]
    pub common: FeatureCommon,
}

/// Add a socket (a coupling sleeve): of sheet, a sleeve over a pipe with its flat pattern; with
/// `thread`, a turned socket with a pipe thread inside.
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct SocketParams {
    /// Of sheet: the pipe's diameter it goes over (the sleeve's inside, unless `measured` says
    /// otherwise). Threaded: the socket's outside diameter.
    pub diameter: LengthExpr,
    /// The socket's length, along `axis`.
    pub height: LengthExpr,
    /// The sheet's (the wall's) thickness.
    pub thickness: LengthExpr,
    /// A pipe thread inside, as `G1/2` or `Rp1/2`: a turned socket.
    #[serde(default)]
    pub thread: Option<String>,
    /// How far the thread runs from the top end. Default: all the length.
    #[serde(default)]
    pub thread_length: Option<LengthExpr>,
    /// A threaded socket's bore. Default: the thread's minor diameter.
    #[serde(default)]
    pub bore: Option<LengthExpr>,
    /// Which diameter a sheet socket's `diameter` is. Default: inside.
    #[serde(default)]
    pub measured: Option<MeasuredParam>,
    /// The centre of the bottom. Default: the origin.
    #[serde(default, deserialize_with = "present", skip_serializing_if = "Option::is_none")]
    #[schemars(with = "PointExpr3")]
    pub base: Option<PointExpr3>,
    /// The axis, from the bottom to the top. Default: Z.
    #[serde(default, deserialize_with = "present", skip_serializing_if = "Option::is_none")]
    #[schemars(with = "AxisDir")]
    pub axis: Option<AxisDir>,
    #[serde(flatten, deserialize_with = "feature_common")]
    pub common: FeatureCommon,
}

/// Add a threaded end (a pipe nipple): a turned tube with a pipe thread outside, from its top.
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct NippleParams {
    /// The pipe thread outside, as `G1/2` or `R1/2`; its major diameter is the nipple's.
    pub thread: String,
    /// The nipple's length, along `axis`.
    pub height: LengthExpr,
    /// The wall's thickness (the bore is the thread's major diameter less twice it).
    pub thickness: LengthExpr,
    /// How far the thread runs from the top end. Default: all the length.
    #[serde(default)]
    pub thread_length: Option<LengthExpr>,
    /// The bore, instead of the thickness's.
    #[serde(default)]
    pub bore: Option<LengthExpr>,
    /// The centre of the bottom. Default: the origin.
    #[serde(default, deserialize_with = "present", skip_serializing_if = "Option::is_none")]
    #[schemars(with = "PointExpr3")]
    pub base: Option<PointExpr3>,
    /// The axis, from the bottom to the threaded top. Default: Z.
    #[serde(default, deserialize_with = "present", skip_serializing_if = "Option::is_none")]
    #[schemars(with = "AxisDir")]
    pub axis: Option<AxisDir>,
    #[serde(flatten, deserialize_with = "feature_common")]
    pub common: FeatureCommon,
}

/// Add a pipe with a turned-out collar at its top (a bördel, a lap-joint stub end).
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct CollarParams {
    /// The pipe's diameter.
    pub diameter: LengthExpr,
    /// The pipe's height up to where it bends out.
    pub height: LengthExpr,
    /// The sheet's thickness.
    pub thickness: LengthExpr,
    /// How far the collar reaches beyond the pipe's outside. Default: a quarter of the diameter.
    #[serde(default)]
    pub width: Option<LengthExpr>,
    /// The bend's inside radius. Default: the thickness.
    #[serde(default)]
    pub bend: Option<LengthExpr>,
    /// Which diameter `diameter` is. Default: outside.
    #[serde(default)]
    pub measured: Option<MeasuredParam>,
    /// The centre of the bottom. Default: the origin.
    #[serde(default, deserialize_with = "present", skip_serializing_if = "Option::is_none")]
    #[schemars(with = "PointExpr3")]
    pub base: Option<PointExpr3>,
    /// The axis, from the bottom to the collar. Default: Z.
    #[serde(default, deserialize_with = "present", skip_serializing_if = "Option::is_none")]
    #[schemars(with = "AxisDir")]
    pub axis: Option<AxisDir>,
    #[serde(flatten, deserialize_with = "feature_common")]
    pub common: FeatureCommon,
}

/// A fitting's flat patterns, or one DXF of them per piece.
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct FittingPatternParams {
    /// The fitting: its feature's id or name. Default: the newest fitting.
    #[serde(default, deserialize_with = "present", skip_serializing_if = "Option::is_none")]
    #[schemars(with = "IdOrName")]
    pub feature: Option<IdOrName>,
    /// Only this piece: `body`, `branch`, or `segment N` (or its number from 1). Default: all.
    #[serde(default)]
    pub piece: Option<String>,
    /// Where to write the DXF: one file a piece, `<path>-<piece>.dxf`. Without it the DXF texts
    /// come back.
    #[serde(default)]
    pub path: Option<String>,
}

/// One piece's flat pattern.
#[derive(Debug, Serialize, JsonSchema)]
pub struct FittingPiece {
    /// `body`, `branch`, `segment N` or `flange`.
    pub piece: String,
    /// The body the piece is in the model.
    pub body: Option<String>,
    /// The flat pattern's size in mm, along its x and y.
    pub width: f64,
    pub height: f64,
    /// Its area in mm², the holes taken out.
    pub area: f64,
    /// The outline, closed, in mm (`fitting.patterns` only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outline: Option<Vec<[f64; 2]>>,
    /// The holes cut in it (`fitting.patterns` only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub holes: Option<Vec<Vec<[f64; 2]>>>,
    /// The lines marked on it, each from one point to another (`fitting.patterns` only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub marks: Option<Vec<[[f64; 2]; 2]>>,
    /// The DXF text (`fitting.export_dxf` without `path`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dxf: Option<String>,
    /// The file written (`fitting.export_dxf` with `path`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

/// A fitting's pieces.
#[derive(Debug, Serialize, JsonSchema)]
pub struct FittingPatternResult {
    /// The fitting's feature id.
    pub feature: u64,
    /// The sheet's thickness in mm.
    pub thickness: f64,
    /// Whether the patterns are exact developments (false: a collar's blank, which stretches
    /// as it is turned out).
    pub exact: bool,
    pub pieces: Vec<FittingPiece>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub param_warnings: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::{CollarParams, ConeParams, ElbowParams, FittingPatternParams, FlangeParams, NippleParams, SocketParams, TeeParams};
    use crate::schema::input_schema_for;

    fn keys(schema: schemars::Schema) -> Vec<String> {
        schema.as_value()["properties"].as_object().map(|o| o.keys().cloned().collect()).unwrap_or_default()
    }

    #[test]
    fn the_fitting_contracts_take_their_keys() {
        assert!(keys(input_schema_for::<TeeParams>()).contains(&"branch_angle".to_string()));
        assert!(keys(input_schema_for::<ConeParams>()).contains(&"shift".to_string()));
        assert!(keys(input_schema_for::<ElbowParams>()).contains(&"segments".to_string()));
        assert_eq!(keys(input_schema_for::<FittingPatternParams>()), ["feature", "path", "piece"]);
        assert!(keys(input_schema_for::<FlangeParams>()).contains(&"dn".to_string()));
        assert!(keys(input_schema_for::<SocketParams>()).contains(&"thread".to_string()));
        assert!(keys(input_schema_for::<NippleParams>()).contains(&"thread_length".to_string()));
        assert!(keys(input_schema_for::<CollarParams>()).contains(&"width".to_string()));
    }
}
