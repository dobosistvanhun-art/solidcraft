//! Thread standards: what a thread designation means — its diameter, pitch and the shape of
//! its basic profile in the axial section.
//!
//! | Family | Designation | Profile |
//! |---|---|---|
//! | ISO metric (coarse / fine) | `M8`, `M8x1`, `MF8x1` | 60° (ISO 68-1) |
//! | Unified | `1/4-20 UNC`, `#10-32 UNF`, `1/2 UNC` | 60° |
//! | Whitworth | `1/4 BSW`, `3/8 BSF` | 55° |
//! | Pipe, parallel | `G1/2` (BSPP), `Rp1/2` | 55° |
//! | Pipe, tapered 1:16 | `R1/2` (external), `Rc1/2` (internal) | 55° |
//! | American pipe, tapered 1:16 | `1/2 NPT`, `1/2-14 NPT`, `NPTF` | 60° |
//! | Trapezoidal | `Tr16x4`, `Tr16x8(P4)` (two starts) | 30° (ISO 2904) |
//! | Buttress | `S16x4` | 3° / 30° (DIN 513) |
//! | Round (knuckle) | `Rd16x1/8`, `Rd40x4` | 30°, rounded (DIN 405) |
//! | Steel conduit | `Pg13.5` | 80° (DIN 40430) |
//! | Custom | `20x2.5 60°`, `20x3 3/30° h2.2` | an included angle, or the two flank angles from square to the axis, and an optional depth |
//!
//! Any designation may end in `LH` (left-handed). Inch sizes are converted to millimetres.

use std::f64::consts::PI;

/// Which standard a thread follows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    Metric,
    Unified,
    Whitworth,
    /// G (BSPP) and Rp.
    PipeParallel,
    /// R (external) and Rc (internal), tapered 1:16.
    PipeTapered,
    Npt,
    Trapezoidal,
    Buttress,
    Round,
    Pg,
    Custom,
}

/// A thread: its size and basic profile.
#[derive(Clone, Debug, PartialEq)]
pub struct ThreadSpec {
    pub family: Family,
    /// As it is shown (normalised).
    pub designation: String,
    /// Major (outside) diameter, mm (a tapered thread's at its large end).
    pub major: f64,
    /// Pitch: the axial distance between neighbouring threads, mm.
    pub pitch: f64,
    /// Starts (the lead is pitch × starts).
    pub starts: u32,
    /// The two flanks' angles from square to the axis (radians): the one facing the start of
    /// the threaded length, then the other.
    pub flanks: (f64, f64),
    /// The external thread's depth below the major diameter, mm.
    pub depth: f64,
    /// The width of the external thread's flat crest, mm.
    pub crest_flat: f64,
    /// The radius rounding the root corners, mm (0: sharp).
    pub root_round: f64,
    /// The radius's change per unit length along the axis (1:16 on the diameter is 1/32).
    pub taper: f64,
    pub left_hand: bool,
}

impl ThreadSpec {
    pub fn lead(&self) -> f64 {
        self.pitch * self.starts as f64
    }
    /// The width of the groove's flat root (the space between two external threads at the
    /// minor diameter).
    pub fn root_flat(&self) -> f64 {
        self.pitch - self.crest_flat - self.depth * (self.flanks.0.tan() + self.flanks.1.tan())
    }
    /// The minor diameter of the external thread.
    pub fn minor(&self) -> f64 {
        self.major - 2.0 * self.depth
    }
    /// The hole a tap needs (about the internal thread's minor diameter).
    pub fn tap_drill(&self) -> f64 {
        match self.family {
            Family::Metric | Family::Unified => self.major - self.pitch,
            _ => self.major - 2.0 * self.depth * 0.9,
        }
    }
}

const INCH: f64 = 25.4;

/// ISO metric coarse pitches by nominal diameter.
const METRIC_COARSE: &[(f64, f64)] = &[
    (1.0, 0.25),
    (1.2, 0.25),
    (1.6, 0.35),
    (2.0, 0.4),
    (2.5, 0.45),
    (3.0, 0.5),
    (3.5, 0.6),
    (4.0, 0.7),
    (5.0, 0.8),
    (6.0, 1.0),
    (7.0, 1.0),
    (8.0, 1.25),
    (10.0, 1.5),
    (12.0, 1.75),
    (14.0, 2.0),
    (16.0, 2.0),
    (18.0, 2.5),
    (20.0, 2.5),
    (22.0, 2.5),
    (24.0, 3.0),
    (27.0, 3.0),
    (30.0, 3.5),
    (33.0, 3.5),
    (36.0, 4.0),
    (39.0, 4.0),
    (42.0, 4.5),
    (45.0, 4.5),
    (48.0, 5.0),
    (52.0, 5.0),
    (56.0, 5.5),
    (60.0, 5.5),
    (64.0, 6.0),
];

/// Unified sizes: (name, major diameter in inches, UNC threads per inch, UNF threads per inch).
const UNIFIED: &[(&str, f64, f64, f64)] = &[
    ("#2", 0.086, 56.0, 64.0),
    ("#4", 0.112, 40.0, 48.0),
    ("#5", 0.125, 40.0, 44.0),
    ("#6", 0.138, 32.0, 40.0),
    ("#8", 0.164, 32.0, 36.0),
    ("#10", 0.19, 24.0, 32.0),
    ("#12", 0.216, 24.0, 28.0),
    ("1/4", 0.25, 20.0, 28.0),
    ("5/16", 0.3125, 18.0, 24.0),
    ("3/8", 0.375, 16.0, 24.0),
    ("7/16", 0.4375, 14.0, 20.0),
    ("1/2", 0.5, 13.0, 20.0),
    ("9/16", 0.5625, 12.0, 18.0),
    ("5/8", 0.625, 11.0, 18.0),
    ("3/4", 0.75, 10.0, 16.0),
    ("7/8", 0.875, 9.0, 14.0),
    ("1", 1.0, 8.0, 12.0),
    ("1-1/8", 1.125, 7.0, 12.0),
    ("1-1/4", 1.25, 7.0, 12.0),
    ("1-3/8", 1.375, 6.0, 12.0),
    ("1-1/2", 1.5, 6.0, 12.0),
    ("2", 2.0, 4.5, 12.0),
];

/// Whitworth sizes: (name, major in inches, BSW tpi, BSF tpi; 0 = none).
const WHITWORTH: &[(&str, f64, f64, f64)] = &[
    ("1/8", 0.125, 40.0, 0.0),
    ("3/16", 0.1875, 24.0, 32.0),
    ("1/4", 0.25, 20.0, 26.0),
    ("5/16", 0.3125, 18.0, 22.0),
    ("3/8", 0.375, 16.0, 20.0),
    ("7/16", 0.4375, 14.0, 18.0),
    ("1/2", 0.5, 12.0, 16.0),
    ("5/8", 0.625, 11.0, 14.0),
    ("3/4", 0.75, 10.0, 12.0),
    ("7/8", 0.875, 9.0, 11.0),
    ("1", 1.0, 8.0, 10.0),
];

/// British pipe threads (G, R, Rc, Rp): (size, major diameter mm, threads per inch).
const BSP: &[(&str, f64, f64)] = &[
    ("1/16", 7.723, 28.0),
    ("1/8", 9.728, 28.0),
    ("1/4", 13.157, 19.0),
    ("3/8", 16.662, 19.0),
    ("1/2", 20.955, 14.0),
    ("5/8", 22.911, 14.0),
    ("3/4", 26.441, 14.0),
    ("7/8", 30.201, 14.0),
    ("1", 33.249, 11.0),
    ("1-1/4", 41.91, 11.0),
    ("1-1/2", 47.803, 11.0),
    ("2", 59.614, 11.0),
    ("2-1/2", 75.184, 11.0),
    ("3", 87.884, 11.0),
];

/// NPT: (size, outside diameter mm, threads per inch).
const NPT: &[(&str, f64, f64)] = &[
    ("1/16", 7.938, 27.0),
    ("1/8", 10.287, 27.0),
    ("1/4", 13.716, 18.0),
    ("3/8", 17.145, 18.0),
    ("1/2", 21.336, 14.0),
    ("3/4", 26.67, 14.0),
    ("1", 33.401, 11.5),
    ("1-1/4", 42.164, 11.5),
    ("1-1/2", 48.26, 11.5),
    ("2", 60.325, 11.5),
    ("2-1/2", 73.025, 8.0),
    ("3", 88.9, 8.0),
];

/// ISO 2904 trapezoidal: (nominal diameter, preferred pitch).
const TRAPEZOIDAL: &[(f64, f64)] = &[
    (8.0, 1.5),
    (10.0, 2.0),
    (12.0, 3.0),
    (14.0, 3.0),
    (16.0, 4.0),
    (18.0, 4.0),
    (20.0, 4.0),
    (22.0, 5.0),
    (24.0, 5.0),
    (26.0, 5.0),
    (28.0, 5.0),
    (30.0, 6.0),
    (32.0, 6.0),
    (36.0, 6.0),
    (40.0, 7.0),
    (44.0, 7.0),
    (48.0, 8.0),
    (50.0, 8.0),
    (60.0, 9.0),
];

/// DIN 40430 steel conduit (Pg): (size, outside diameter mm, pitch mm).
const PG: &[(&str, f64, f64)] = &[
    ("7", 12.5, 1.27),
    ("9", 15.2, 1.41),
    ("11", 18.6, 1.41),
    ("13.5", 20.4, 1.41),
    ("16", 22.5, 1.41),
    ("21", 28.3, 1.588),
    ("29", 37.0, 1.588),
    ("36", 47.0, 1.588),
    ("42", 54.0, 1.588),
    ("48", 59.3, 1.588),
];

/// A size written as a fraction of an inch: `1/2`, `1-1/4`, `1 1/4`, `1`, `0.5`.
fn inches(s: &str) -> Option<f64> {
    let s = s.trim();
    let (whole, frac) = match s.split_once(['-', ' ']) {
        Some((w, f)) if f.contains('/') => (w.trim().parse::<f64>().ok()?, f.trim()),
        _ => (0.0, s),
    };
    let v = match frac.split_once('/') {
        Some((a, b)) => whole + a.trim().parse::<f64>().ok()? / b.trim().parse::<f64>().ok()?,
        None => whole + frac.parse::<f64>().ok()?,
    };
    (v.is_finite() && v > 0.0).then_some(v)
}

fn size_key(s: &str) -> String {
    s.trim().replace(' ', "-")
}

fn num(x: f64) -> String {
    let r = (x * 1000.0).round() / 1000.0;
    if (r - r.round()).abs() < 1e-9 { format!("{}", r.round()) } else { format!("{r}") }
}

/// The basic profile of a family, for a pitch: (flank angles, depth, crest flat, root round).
fn profile(family: Family, p: f64) -> ((f64, f64), f64, f64, f64) {
    let d = |deg: f64| deg.to_radians();
    match family {
        Family::Metric | Family::Unified => ((d(30.0), d(30.0)), 0.541_266 * p, p / 8.0, 0.0),
        Family::Whitworth | Family::PipeParallel | Family::PipeTapered => ((d(27.5), d(27.5)), 0.640_327 * p, 0.166_67 * p, 0.137_329 * p),
        Family::Npt => ((d(30.0), d(30.0)), 0.8 * p, 0.038 * p, 0.0),
        Family::Trapezoidal => ((d(15.0), d(15.0)), 0.5 * p, 0.366 * p, 0.0),
        // DIN 513: the load flank at 3° faces away from the start of the thread.
        Family::Buttress => ((d(30.0), d(3.0)), 0.867_77 * p, 0.263_84 * p, 0.124_27 * p),
        Family::Round => ((d(15.0), d(15.0)), 0.5 * p, 0.366 * p, 0.238_51 * p),
        Family::Pg => ((d(40.0), d(40.0)), 0.5 * p, 0.08 * p, 0.0),
        Family::Custom => ((d(30.0), d(30.0)), 0.541_266 * p, p / 8.0, 0.0),
    }
}

fn spec(family: Family, designation: String, major: f64, pitch: f64, starts: u32, left_hand: bool) -> Option<ThreadSpec> {
    if !(major.is_finite() && pitch.is_finite() && major > 0.2 && pitch > 0.05 && pitch < major && major < 2000.0) {
        return None;
    }
    let (flanks, depth, crest_flat, root_round) = profile(family, pitch);
    let taper = if matches!(family, Family::PipeTapered | Family::Npt) { 1.0 / 32.0 } else { 0.0 };
    let s = ThreadSpec { family, designation, major, pitch, starts, flanks, depth, crest_flat, root_round, taper, left_hand };
    (s.root_flat() > 0.0).then_some(s)
}

/// What a thread designation means; `None` when it is not one of the forms above.
pub fn parse(text: &str) -> Option<ThreadSpec> {
    let raw = text.trim();
    let up0 = raw.to_ascii_uppercase().replace('×', "X").replace(',', ".");
    let (up, left) = match up0.trim_end().strip_suffix("LH") {
        Some(rest) => (rest.trim_end().trim_end_matches(['-', ' ']).to_string(), true),
        None => (up0.trim().to_string(), false),
    };
    let lh = if left { " LH" } else { "" };
    let compact: String = up.chars().filter(|c| !c.is_whitespace()).collect();

    // Pg13.5
    if let Some(rest) = compact.strip_prefix("PG") {
        let (_, d, p) = PG.iter().find(|(n, ..)| *n == rest)?;
        return spec(Family::Pg, format!("Pg{rest}{lh}"), *d, *p, 1, left);
    }
    // Tr16x4, Tr16x8(P4)
    if let Some(rest) = compact.strip_prefix("TR") {
        let (d, tail) = match rest.split_once('X') {
            Some((d, t)) => (d.parse::<f64>().ok()?, Some(t)),
            None => (rest.parse::<f64>().ok()?, None),
        };
        let (pitch, starts, shown) = match tail {
            None => {
                let p = TRAPEZOIDAL.iter().find(|(n, _)| (n - d).abs() < 1e-9)?.1;
                (p, 1, format!("Tr{}x{}", num(d), num(p)))
            }
            Some(t) => match t.split_once("(P") {
                Some((lead, p)) => {
                    let (lead, p) = (lead.parse::<f64>().ok()?, p.trim_end_matches(')').parse::<f64>().ok()?);
                    let n = (lead / p).round();
                    if !((lead / p - n).abs() < 1e-6 && (1.0..=8.0).contains(&n)) {
                        return None;
                    }
                    (p, n as u32, format!("Tr{}x{}(P{})", num(d), num(lead), num(p)))
                }
                None => {
                    let p = t.parse::<f64>().ok()?;
                    (p, 1, format!("Tr{}x{}", num(d), num(p)))
                }
            },
        };
        return spec(Family::Trapezoidal, format!("{shown}{lh}"), d, pitch, starts, left);
    }
    // Rd16x1/8 (a fraction: inches), Rd40x4 (mm)
    if let Some(rest) = compact.strip_prefix("RD") {
        let (d, p) = rest.split_once('X')?;
        let d = d.parse::<f64>().ok()?;
        let pitch = if p.contains('/') { inches(p)? * INCH } else { p.parse::<f64>().ok()? };
        return spec(Family::Round, format!("Rd{}x{}{lh}", num(d), p.to_ascii_lowercase()), d, pitch, 1, left);
    }
    // Rc1/2, Rp1/2, R1/2, G1/2 (an A or B tolerance class after a G size is ignored)
    for (prefix, family, name) in
        [("RC", Family::PipeTapered, "Rc"), ("RP", Family::PipeParallel, "Rp"), ("R", Family::PipeTapered, "R"), ("G", Family::PipeParallel, "G")]
    {
        if let Some(rest) = compact.strip_prefix(prefix) {
            let rest = if name == "G" { rest.trim_end_matches(['A', 'B']) } else { rest };
            if let Some((size, d, tpi)) = BSP.iter().find(|(n, ..)| size_key(n) == size_key(rest) || inches(n) == inches(rest)) {
                return spec(family, format!("{name}{size}{lh}"), *d, INCH / tpi, 1, left);
            }
        }
    }
    // S16x4 (buttress)
    if let Some(rest) = compact.strip_prefix('S')
        && let Some((d, p)) = rest.split_once('X')
        && let (Ok(d), Ok(p)) = (d.parse::<f64>(), p.parse::<f64>())
    {
        return spec(Family::Buttress, format!("S{}x{}{lh}", num(d), num(p)), d, p, 1, left);
    }
    // M8, M8x1, MF8x1
    if let Some(rest) = compact.strip_prefix("MF").or_else(|| compact.strip_prefix('M')) {
        let fine = compact.starts_with("MF");
        let (d, p) = match rest.split_once('X') {
            Some((d, p)) => (d.parse::<f64>().ok()?, p.parse::<f64>().ok()?),
            None if !fine => {
                let d = rest.parse::<f64>().ok()?;
                (d, METRIC_COARSE.iter().find(|(n, _)| (n - d).abs() < 1e-9)?.1)
            }
            None => return None,
        };
        let coarse = METRIC_COARSE.iter().any(|(n, q)| (n - d).abs() < 1e-9 && (q - p).abs() < 1e-9);
        let shown = if coarse && !fine { format!("M{}", num(d)) } else { format!("M{}x{}", num(d), num(p)) };
        return spec(Family::Metric, format!("{shown}{lh}"), d, p, 1, left);
    }
    // 1/2 NPT, 1/2-14 NPT, NPT 1/2, 1/2 NPTF
    if up.contains("NPT") {
        let size = up.replace("NPTF", " ").replace("NPT", " ");
        let size = size.trim();
        let size = match size.rsplit_once('-') {
            Some((s, tpi)) if tpi.trim().parse::<f64>().is_ok() && !tpi.contains('/') => s.trim().to_string(),
            _ => size.to_string(),
        };
        let (n, d, tpi) = NPT.iter().find(|(n, ..)| size_key(n) == size_key(&size) || inches(n) == inches(&size))?;
        return spec(Family::Npt, format!("{n} NPT{lh}"), *d, INCH / tpi, 1, left);
    }
    // 1/4 BSW, 3/8 BSF
    for (kind, fine) in [("BSW", false), ("BSF", true)] {
        if let Some(size) = up.strip_suffix(kind).map(str::trim).map(|s| s.trim_end_matches('-').trim().to_string()) {
            let (n, d, w, f) = WHITWORTH.iter().find(|(n, ..)| size_key(n) == size_key(&size) || inches(n) == inches(&size))?;
            let tpi = if fine { *f } else { *w };
            if tpi <= 0.0 {
                return None;
            }
            return spec(Family::Whitworth, format!("{n} {kind}{lh}"), d * INCH, INCH / tpi, 1, left);
        }
    }
    // 1/4-20 UNC, #10-32 UNF, 1/2 UNC, 1/4-28 (UN)
    let unified_kind = ["UNEF", "UNC", "UNF", "UN"].into_iter().find(|k| up.trim_end().ends_with(k));
    let body = unified_kind.map(|k| up.trim_end().trim_end_matches(k).trim().to_string()).unwrap_or_else(|| up.trim().to_string());
    if unified_kind.is_some() || body.starts_with('#') || (body.contains('/') && body.contains('-')) {
        let (size, tpi) = match body.rsplit_once('-') {
            Some((s, t)) if !t.contains('/') && t.trim().parse::<f64>().is_ok() => (s.trim().to_string(), t.trim().parse::<f64>().ok()),
            _ => (body.clone(), None),
        };
        let (n, d, c, f) = UNIFIED.iter().find(|(n, ..)| size_key(n) == size_key(&size) || (!size.starts_with('#') && inches(n) == inches(&size)))?;
        let tpi = match (tpi, unified_kind) {
            (Some(t), _) => t,
            (None, Some("UNF")) => *f,
            (None, _) => *c,
        };
        let kind = unified_kind.unwrap_or(if (tpi - c).abs() < 1e-9 {
            "UNC"
        } else if (tpi - f).abs() < 1e-9 {
            "UNF"
        } else {
            "UN"
        });
        return spec(Family::Unified, format!("{n}-{} {kind}{lh}", num(tpi)), d * INCH, INCH / tpi, 1, left);
    }
    custom(&up, left)
}

/// `20x2.5 60°`, `20x3 3/30° h2.2`: diameter x pitch, then the included angle (or the two flank
/// angles from square to the axis), then an optional depth.
fn custom(up: &str, left: bool) -> Option<ThreadSpec> {
    let mut words = up.split_whitespace();
    let size = words.next()?;
    let (d, p) = size.split_once('X')?;
    let (d, p) = (d.trim_start_matches('D').parse::<f64>().ok()?, p.parse::<f64>().ok()?);
    let mut flanks = None;
    let mut depth = None;
    for w in words {
        if let Some(h) = w.strip_prefix('H') {
            depth = Some(h.parse::<f64>().ok()?);
        } else {
            let a = w.trim_end_matches("DEG").trim_end_matches('°');
            flanks = Some(match a.split_once('/') {
                Some((a, b)) => (a.parse::<f64>().ok()?.to_radians(), b.parse::<f64>().ok()?.to_radians()),
                None => {
                    let half = a.parse::<f64>().ok()?.to_radians() / 2.0;
                    (half, half)
                }
            });
        }
    }
    let flanks = flanks?;
    if !(flanks.0 >= 0.0 && flanks.1 >= 0.0 && flanks.0 < PI / 2.2 && flanks.1 < PI / 2.2) {
        return None;
    }
    let tsum = flanks.0.tan() + flanks.1.tan();
    // By default the crest and root flats are each an eighth of the pitch.
    let depth = depth.unwrap_or(if tsum > 1e-9 { 0.75 * p / tsum } else { 0.5 * p });
    let crest_flat = (p - depth * tsum) / 2.0;
    if !(depth > 0.0 && depth < d / 2.0 && crest_flat > 0.0) {
        return None;
    }
    let angles = if (flanks.0 - flanks.1).abs() < 1e-9 {
        format!("{}°", num((flanks.0 * 2.0).to_degrees()))
    } else {
        format!("{}/{}°", num(flanks.0.to_degrees()), num(flanks.1.to_degrees()))
    };
    let s = ThreadSpec {
        family: Family::Custom,
        designation: format!("{}x{} {angles} h{}{}", num(d), num(p), num(depth), if left { " LH" } else { "" }),
        major: d,
        pitch: p,
        starts: 1,
        flanks,
        depth,
        crest_flat,
        root_round: 0.0,
        taper: 0.0,
        left_hand: left,
    };
    (s.root_flat() > 0.0 && p < d).then_some(s)
}

/// The thread profiles a picker offers, by label, and the label a designation belongs to.
pub const PROFILES: &[&str] = &[
    "ISO metric",
    "UNC",
    "UNF",
    "BSW (Whitworth)",
    "G (BSPP)",
    "R (BSPT external)",
    "Rc (BSPT internal)",
    "Rp (parallel internal)",
    "NPT",
    "Tr (trapezoidal)",
    "S (buttress)",
    "Rd (round)",
    "Pg (conduit)",
    "Custom",
];

/// The profile label of a designation (see [`PROFILES`]).
pub fn profile_of(designation: &str) -> Option<&'static str> {
    let s = parse(designation)?;
    let d = s.designation.to_ascii_uppercase();
    Some(match s.family {
        Family::Metric => "ISO metric",
        Family::Unified if d.contains("UNF") || d.contains("UNEF") => "UNF",
        Family::Unified => "UNC",
        Family::Whitworth => "BSW (Whitworth)",
        Family::PipeParallel if d.starts_with("RP") => "Rp (parallel internal)",
        Family::PipeParallel => "G (BSPP)",
        Family::PipeTapered if d.starts_with("RC") => "Rc (BSPT internal)",
        Family::PipeTapered => "R (BSPT external)",
        Family::Npt => "NPT",
        Family::Trapezoidal => "Tr (trapezoidal)",
        Family::Buttress => "S (buttress)",
        Family::Round => "Rd (round)",
        Family::Pg => "Pg (conduit)",
        Family::Custom => "Custom",
    })
}

/// The standard sizes of a profile (see [`PROFILES`]), as designations; ISO metric's coarse ones.
pub fn sizes(profile: &str) -> Vec<String> {
    match profile {
        "ISO metric" => METRIC_COARSE.iter().map(|(d, _)| format!("M{}", num(*d))).collect(),
        "UNC" => UNIFIED.iter().map(|(n, _, c, _)| format!("{n}-{} UNC", num(*c))).collect(),
        "UNF" => UNIFIED.iter().map(|(n, _, _, f)| format!("{n}-{} UNF", num(*f))).collect(),
        "BSW (Whitworth)" => WHITWORTH.iter().map(|(n, ..)| format!("{n} BSW")).collect(),
        "G (BSPP)" => BSP.iter().map(|(n, ..)| format!("G{n}")).collect(),
        "R (BSPT external)" => BSP.iter().map(|(n, ..)| format!("R{n}")).collect(),
        "Rc (BSPT internal)" => BSP.iter().map(|(n, ..)| format!("Rc{n}")).collect(),
        "Rp (parallel internal)" => BSP.iter().map(|(n, ..)| format!("Rp{n}")).collect(),
        "NPT" => NPT.iter().map(|(n, ..)| format!("{n} NPT")).collect(),
        "Tr (trapezoidal)" => TRAPEZOIDAL.iter().map(|(d, p)| format!("Tr{}x{}", num(*d), num(*p))).collect(),
        "S (buttress)" => TRAPEZOIDAL.iter().filter(|(d, _)| *d >= 10.0).map(|(d, p)| format!("S{}x{}", num(*d), num(*p))).collect(),
        "Rd (round)" => {
            [(10.0, "1/8"), (12.0, "1/8"), (16.0, "1/8"), (20.0, "1/8"), (24.0, "1/8"), (30.0, "1/8"), (40.0, "1/6"), (50.0, "1/6"), (60.0, "1/6")]
                .iter()
                .map(|(d, p)| format!("Rd{}x{p}", num(*d)))
                .collect()
        }
        "Pg (conduit)" => PG.iter().map(|(n, ..)| format!("Pg{n}")).collect(),
        _ => Vec::new(),
    }
}

/// The designations in a family for a picker (with a size), and how to write the others.
pub const HELP: &str = "M8, M8x1, MF10x1, 1/4-20 UNC, #10-32 UNF, 1/4 BSW, G1/2, R1/2, Rc1/2, Rp1/2, 1/2 NPT, Tr16x4, Tr16x8(P4), S16x4, Rd16x1/8, Pg13.5, or custom: 20x2.5 60° (included angle), 20x3 3/30° h2.2 (flank angles, depth); add LH for left-handed";

/// The groove between two neighbouring threads in the axial section, as (radius, axial offset)
/// corners in order: cut from a shaft (`internal` false) from below the root out to `outer`, or
/// from a hole wall from `inner` out to past the major diameter.
pub fn groove(s: &ThreadSpec, internal: bool, inner: f64, outer: f64) -> Vec<(f64, f64)> {
    let (t1, t2) = (s.flanks.0.tan(), s.flanks.1.tan());
    let r_major = s.major / 2.0;
    let r_root = r_major - s.depth;
    // The space at radius r: its two edges as axial offsets from the groove's centre line, which
    // runs through the middle of the root flat.
    // Past where the groove is this wide it runs on straight (a tapered thread's groove reaches
    // further than its depth), so neighbouring turns never meet.
    let widest = 0.92 * s.pitch;
    let tsum = (t1 + t2).max(1e-9);
    if internal {
        let half = s.crest_flat / 2.0;
        let edge_lo = |r: f64| -(half + (r_major - r) * t1);
        let edge_hi = |r: f64| half + (r_major - r) * t2;
        let r_out = r_major + 0.02 * s.pitch;
        let r_cap = r_major - (widest - s.crest_flat) / tsum;
        if inner < r_cap {
            vec![(inner, edge_lo(r_cap)), (r_cap, edge_lo(r_cap)), (r_out, edge_lo(r_out)), (r_out, edge_hi(r_out)), (r_cap, edge_hi(r_cap)), (inner, edge_hi(r_cap))]
        } else {
            vec![(inner, edge_lo(inner)), (r_out, edge_lo(r_out)), (r_out, edge_hi(r_out)), (inner, edge_hi(inner))]
        }
    } else {
        let half = s.root_flat() / 2.0;
        let edge_lo = |r: f64| -(half + (r - r_root) * t1);
        let edge_hi = |r: f64| half + (r - r_root) * t2;
        let r_cap = r_root + (widest - s.root_flat()) / tsum;
        let mut pts = if outer > r_cap {
            vec![(r_root, edge_lo(r_root)), (r_cap, edge_lo(r_cap)), (outer, edge_lo(r_cap)), (outer, edge_hi(r_cap)), (r_cap, edge_hi(r_cap)), (r_root, edge_hi(r_root))]
        } else {
            vec![(r_root, edge_lo(r_root)), (outer, edge_lo(outer)), (outer, edge_hi(outer)), (r_root, edge_hi(r_root))]
        };
        if s.root_round > 1e-9 {
            pts = round_root(&pts, s.root_round);
        }
        pts
    }
}

/// The groove with its two root corners (the first and last) rounded by `r`.
fn round_root(pts: &[(f64, f64)], r: f64) -> Vec<(f64, f64)> {
    let p = |i: usize| pts.get(i).copied().unwrap_or((0.0, 0.0));
    let corner = |at: (f64, f64), a: (f64, f64), b: (f64, f64)| -> Vec<(f64, f64)> {
        // Unit directions from the corner to its neighbours.
        let unit = |v: (f64, f64)| {
            let l = (v.0 * v.0 + v.1 * v.1).sqrt().max(1e-12);
            (v.0 / l, v.1 / l)
        };
        let (u, w) = (unit((a.0 - at.0, a.1 - at.1)), unit((b.0 - at.0, b.1 - at.1)));
        let cos = (u.0 * w.0 + u.1 * w.1).clamp(-1.0, 1.0);
        let phi = cos.acos();
        let t = r / (phi / 2.0).tan();
        let ea = (at.0 + u.0 * t, at.1 + u.1 * t);
        let eb = (at.0 + w.0 * t, at.1 + w.1 * t);
        let bis = unit((u.0 + w.0, u.1 + w.1));
        let d = r / (phi / 2.0).sin();
        let c = (at.0 + bis.0 * d, at.1 + bis.1 * d);
        let (a0, a1) = ((ea.1 - c.1).atan2(ea.0 - c.0), (eb.1 - c.1).atan2(eb.0 - c.0));
        let mut sweep = a1 - a0;
        while sweep > PI {
            sweep -= 2.0 * PI;
        }
        while sweep < -PI {
            sweep += 2.0 * PI;
        }
        (0..=3).map(|i| (c.0 + r * (a0 + sweep * i as f64 / 3.0).cos(), c.1 + r * (a0 + sweep * i as f64 / 3.0).sin())).collect()
    };
    let n = pts.len();
    // The first corner, from the last point's side round to the second's.
    let first = corner(p(0), p(n - 1), p(1));
    let last = corner(p(n - 1), p(n - 2), p(0));
    let mut out = Vec::new();
    out.extend(first);
    out.extend(pts.iter().copied().take(n - 1).skip(1));
    out.extend(last);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn designations_read_their_size_and_pitch() {
        let cases: &[(&str, Family, f64, f64, &str)] = &[
            ("M8", Family::Metric, 8.0, 1.25, "M8"),
            ("m8x1", Family::Metric, 8.0, 1.0, "M8x1"),
            ("MF10x1", Family::Metric, 10.0, 1.0, "M10x1"),
            ("M10 LH", Family::Metric, 10.0, 1.5, "M10 LH"),
            ("1/4-20 UNC", Family::Unified, 6.35, 1.27, "1/4-20 UNC"),
            ("1/4 UNF", Family::Unified, 6.35, 25.4 / 28.0, "1/4-28 UNF"),
            ("#10-32 UNF", Family::Unified, 4.826, 25.4 / 32.0, "#10-32 UNF"),
            ("1/2 BSW", Family::Whitworth, 12.7, 25.4 / 12.0, "1/2 BSW"),
            ("G1/2", Family::PipeParallel, 20.955, 25.4 / 14.0, "G1/2"),
            ("G 1/2 A", Family::PipeParallel, 20.955, 25.4 / 14.0, "G1/2"),
            ("R1/4", Family::PipeTapered, 13.157, 25.4 / 19.0, "R1/4"),
            ("Rc3/8", Family::PipeTapered, 16.662, 25.4 / 19.0, "Rc3/8"),
            ("1/2 NPT", Family::Npt, 21.336, 25.4 / 14.0, "1/2 NPT"),
            ("1/2-14 NPTF", Family::Npt, 21.336, 25.4 / 14.0, "1/2 NPT"),
            ("Tr16x4", Family::Trapezoidal, 16.0, 4.0, "Tr16x4"),
            ("Tr20", Family::Trapezoidal, 20.0, 4.0, "Tr20x4"),
            ("Tr16x8(P4)", Family::Trapezoidal, 16.0, 4.0, "Tr16x8(P4)"),
            ("S16x4", Family::Buttress, 16.0, 4.0, "S16x4"),
            ("Rd16x1/8", Family::Round, 16.0, 3.175, "Rd16x1/8"),
            ("Pg13.5", Family::Pg, 20.4, 1.41, "Pg13.5"),
            ("20x2.5 60°", Family::Custom, 20.0, 2.5, "20x2.5 60° h1.624"),
            ("20x3 3/30 h2.2", Family::Custom, 20.0, 3.0, "20x3 3/30° h2.2"),
        ];
        for (text, family, d, p, shown) in cases {
            let s = parse(text).unwrap_or_else(|| panic!("{text}"));
            assert_eq!(s.family, *family, "{text}");
            assert!(close(s.major, *d) && close(s.pitch, *p), "{text}: {s:?}");
            assert_eq!(s.designation, *shown, "{text}");
            assert!(s.root_flat() > 0.0, "{text}: {s:?}");
        }
        assert_eq!(parse("Tr16x8(P4)").map(|s| s.starts), Some(2));
        assert!(parse("M10 LH").unwrap().left_hand);
        // Every size a picker offers reads back as itself, in its profile.
        for profile in PROFILES {
            for d in sizes(profile) {
                let s = parse(&d).unwrap_or_else(|| panic!("{profile}: {d}"));
                assert_eq!(profile_of(&d), Some(*profile), "{d}");
                assert!(s.root_flat() > 0.0, "{d}");
            }
        }
        assert_eq!(parse("1/2 NPT").map(|s| s.taper), Some(1.0 / 32.0));
        for bad in ["", "X", "M", "G9/7", "Pg8", "Tr16x8(P3)", "20x2.5", "20x30 60°", "M8x9"] {
            assert!(parse(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn iso_profile_matches_the_basic_dimensions() {
        let s = parse("M10").unwrap();
        // ISO 68-1: depth 5H/8, crest P/8, root P/4.
        assert!(close(s.depth, 0.541_266 * 1.5) && close(s.root_flat(), 1.5 / 4.0), "{s:?}");
        let g = groove(&s, false, 0.0, 5.05);
        assert_eq!(g.len(), 4);
        // The groove is a pitch wide less the crest at the major diameter.
        let at_major = |pts: &[(f64, f64)]| {
            let (t1, t2) = (s.flanks.0.tan(), s.flanks.1.tan());
            pts.first().map(|_| s.root_flat() + s.depth * (t1 + t2))
        };
        assert!(close(at_major(&g).unwrap(), 1.5 - 1.5 / 8.0));
    }

    #[test]
    fn rounded_roots_stay_inside_the_sharp_groove() {
        let s = parse("Rd16x4").unwrap();
        let g = groove(&s, false, 0.0, 8.5);
        assert!(g.len() > 4);
        let r_root = s.major / 2.0 - s.depth;
        assert!(g.iter().all(|(r, _)| *r >= r_root - 1e-9), "{g:?}");
    }
}
