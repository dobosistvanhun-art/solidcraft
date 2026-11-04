//! A nesting job saved with the design: the parts to cut (sketches or the flat patterns of sheet
//! metal bodies) and how many of each, the material sheet, the gaps, and where each copy was put.
//! The engine's `nest.*` commands work it out and edit it; this is only what is kept.

use serde::{Deserialize, Serialize};

/// Where a part's outline comes from.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PartSource {
    /// The closed contours of a sketch (its outer contours and their holes), by the sketch's id.
    Sketch(u64),
    /// The flat pattern of a sheet metal body, by the body's name.
    SheetBody(String),
}

/// A part to cut and how many of it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NestPart {
    pub source: PartSource,
    pub count: u32,
    /// The part has a grain direction (wood): it turns only by a half turn.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub grain: bool,
    /// The part may not turn at all.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub fixed_angle: bool,
}

/// The material sheet (mm): every sheet of the job is this size.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MaterialSheet {
    pub width: f64,
    pub height: f64,
    /// How far parts stay from the sheet's edges.
    pub margin: f64,
}

impl Default for MaterialSheet {
    fn default() -> Self {
        // A common plywood board.
        MaterialSheet { width: 1220.0, height: 610.0, margin: 5.0 }
    }
}

/// How hard the automatic nesting tries: the angle step it turns parts by, and how many orders
/// it tries.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NestQuality {
    /// 15° steps.
    Fast,
    /// 5° steps.
    #[default]
    Normal,
    /// 1° steps.
    Precise,
}

impl NestQuality {
    pub fn angle_step_deg(self) -> f64 {
        match self {
            NestQuality::Fast => 15.0,
            NestQuality::Normal => 5.0,
            NestQuality::Precise => 1.0,
        }
    }
}

/// Where one copy of a part lies: on which sheet, its turn (degrees, counter-clockwise, about
/// the part's own centre) and where that centre is on the sheet (mm, from its lower-left corner).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Placement {
    /// Index into the job's parts.
    pub part: usize,
    pub sheet: usize,
    pub x: f64,
    pub y: f64,
    pub angle: f64,
    /// Kept where it is when the nesting runs again.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub locked: bool,
}

/// A layout the nesting found: its placements and the copies it could not place.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct NestLayout {
    pub placements: Vec<Placement>,
    /// Copies that fit on no sheet (a part larger than the sheet), by part index.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unplaced: Vec<usize>,
}

/// The nesting job.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NestJob {
    pub parts: Vec<NestPart>,
    #[serde(default)]
    pub sheet: MaterialSheet,
    /// The least distance between two parts (mm).
    #[serde(default = "default_gap")]
    pub gap: f64,
    /// The laser's cut width (mm), kept apart from the gap: the parts also stay this much further
    /// apart.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub kerf: f64,
    #[serde(default)]
    pub quality: NestQuality,
    /// The layout in use.
    #[serde(default)]
    pub layout: NestLayout,
    /// Other good layouts the last run found, to compare and choose from.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variants: Vec<NestLayout>,
}

fn default_gap() -> f64 {
    3.0
}

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

impl Default for NestJob {
    fn default() -> Self {
        NestJob {
            parts: Vec::new(),
            sheet: MaterialSheet::default(),
            gap: default_gap(),
            kerf: 0.0,
            quality: NestQuality::default(),
            layout: NestLayout::default(),
            variants: Vec::new(),
        }
    }
}

impl NestJob {
    /// The sheets the layout uses.
    pub fn sheet_count(&self) -> usize {
        self.layout.placements.iter().map(|p| p.sheet + 1).max().unwrap_or(0)
    }

    /// The spacing parts keep: the gap and the cut width.
    pub fn spacing(&self) -> f64 {
        self.gap.max(0.0) + self.kerf.max(0.0)
    }
}
