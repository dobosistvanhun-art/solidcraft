//! Drawings: sheets of 2D views of the model, with a frame, a title block, dimensions and
//! notes. A drawing saves only what it is made of — the sheet, the views (where they sit, how
//! they look at the model), the dimensions (which model points they measure) and the notes;
//! the lines are projected from the current model whenever it is shown or exported, so a
//! drawing follows every change of the model.
//!
//! Sheet coordinates are millimetres from the sheet's lower left corner, x to the right and y up.
//! A view's own coordinates are model millimetres in its projection plane (`u` to the right,
//! `v` up), centred on the model's bounding box; a view at scale `s` puts the point (u, v) at
//! `view.at + s·(u, v)` on the sheet.

use serde::{Deserialize, Serialize};
use solidcraft_geom::Vec3;

/// The largest number of drawings, views, dimensions or notes a design may hold (a damaged or
/// hostile file is refused rather than drawn).
pub const MAX_ITEMS: usize = 2000;

/// ISO 216 sheet sizes (width × height in landscape, mm).
pub const SHEET_SIZES: &[(&str, f64, f64)] =
    &[("A4", 297.0, 210.0), ("A3", 420.0, 297.0), ("A2", 594.0, 420.0), ("A1", 841.0, 594.0), ("A0", 1189.0, 841.0)];

/// The scales a drawing offers (enlargements, full size, reductions — ISO 5455).
pub const SCALES: &[&str] = &["10:1", "5:1", "2:1", "1:1", "1:2", "1:2.5", "1:5", "1:10", "1:20", "1:50", "1:100"];

/// A scale such as `1:2` or `2:1` as a factor (sheet mm per model mm).
pub fn parse_scale(text: &str) -> Option<f64> {
    let (a, b) = text.trim().split_once(':')?;
    let (a, b) = (a.trim().replace(',', ".").parse::<f64>().ok()?, b.trim().replace(',', ".").parse::<f64>().ok()?);
    let s = a / b;
    (s.is_finite() && (1e-4..=1e3).contains(&s)).then_some(s)
}

/// How the views of a drawing are laid out around their parent: ISO E (first angle, the European
/// standard: the view from the left is put to the right) or ISO A (third angle).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Projection {
    #[default]
    First,
    Third,
}

/// The sheet: size, orientation and the title block's texts.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Sheet {
    /// A4 … A0.
    pub size: String,
    #[serde(default = "yes")]
    pub landscape: bool,
}

fn yes() -> bool {
    true
}

impl Default for Sheet {
    fn default() -> Self {
        Sheet { size: "A3".into(), landscape: true }
    }
}

impl Sheet {
    /// Width and height (mm); an unknown size is A3.
    pub fn dims(&self) -> (f64, f64) {
        let (w, h) = SHEET_SIZES.iter().find(|(n, ..)| n.eq_ignore_ascii_case(self.size.trim())).map(|(_, w, h)| (*w, *h)).unwrap_or((420.0, 297.0));
        if self.landscape { (w, h) } else { (h, w) }
    }
}

/// The title block's fields (ISO 7200, the commonly filled ones).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TitleBlock {
    pub title: String,
    pub number: String,
    pub company: String,
    pub drawn_by: String,
    pub checked_by: String,
    pub date: String,
    /// Empty: the part's physical material.
    pub material: String,
    pub revision: String,
    /// General tolerances (e.g. "ISO 2768-m").
    pub tolerances: String,
}

impl TitleBlock {
    /// The fields by key, in the order the block shows them.
    pub fn fields_mut(&mut self) -> [(&'static str, &mut String); 9] {
        [
            ("title", &mut self.title),
            ("number", &mut self.number),
            ("company", &mut self.company),
            ("drawn_by", &mut self.drawn_by),
            ("checked_by", &mut self.checked_by),
            ("date", &mut self.date),
            ("material", &mut self.material),
            ("revision", &mut self.revision),
            ("tolerances", &mut self.tolerances),
        ]
    }
}

/// Where a view looks from.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ViewKind {
    /// Looking at the model from a standard direction: front, back, top, bottom, left, right or iso.
    Base { orientation: String },
    /// Projected from another view toward one side of it: right, left, above or below (iso: a
    /// pictorial view of the parent's orientation).
    Projected { parent: u64, side: String },
    /// A section of another view along the line `a`–`b` (the parent view's own coordinates),
    /// looking the way the arrows point (`flip` turns them round).
    Section {
        parent: u64,
        a: [f64; 2],
        b: [f64; 2],
        #[serde(default)]
        flip: bool,
    },
    /// An enlarged part of another view: the circle about `center` (parent coordinates) of `radius`.
    Detail { parent: u64, center: [f64; 2], radius: f64 },
    /// A sheet metal body's flat pattern, as cut: its outline and cut-outs, with its bend lines
    /// (chain lines, each with its direction, angle and radius). `body` empty: the last sheet
    /// metal body.
    Flat {
        #[serde(default, skip_serializing_if = "String::is_empty")]
        body: String,
    },
}

/// A view on the sheet.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DrawView {
    pub id: u64,
    #[serde(flatten)]
    pub kind: ViewKind,
    /// Where its centre sits on the sheet (mm).
    pub at: [f64; 2],
    /// Its own scale (`2:1`); none: the drawing's (a detail view: twice its parent's).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<String>,
    /// Its name on the sheet: `A` for section A–A, detail B; empty for the main views.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
    /// Draw the hidden edges (dashed).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hidden: bool,
}

/// What a dimension measures.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DimKind {
    /// The horizontal distance between two points (in the view).
    Horizontal,
    /// The vertical distance.
    Vertical,
    /// The true distance, along the line between them.
    Aligned,
    /// A circle's or arc's diameter (`a` the centre, `b` a point on it).
    Diameter,
    /// Its radius.
    Radius,
    /// The angle at `a` from `b` to `c`.
    Angle,
}

/// A model point a dimension measures: on a body's vertex or edge, kept by its index so it
/// follows the model, with the point it had when picked for when the index no longer fits.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DimRef {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub body: String,
    /// The edge (in the body's edge order) the point belongs to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edge: Option<usize>,
    /// start | end | center (the circle's centre).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub at: String,
    /// The point when it was picked (model mm), used when the edge is gone.
    pub point: Vec3,
}

impl DimRef {
    pub fn at_point(p: Vec3) -> DimRef {
        DimRef { body: String::new(), edge: None, at: String::new(), point: p }
    }
}

/// A dimension in a view.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DrawDim {
    pub id: u64,
    pub view: u64,
    pub kind: DimKind,
    pub refs: Vec<DimRef>,
    /// How far the dimension line stands off the measured points (sheet mm; signed: the side).
    #[serde(default)]
    pub offset: f64,
    /// Its text in place of the measured value (`<>` stands for the value).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub text: String,
    /// A tolerance after the value: `±0.1`, `+0.2/-0.1`, or a fit such as `H7`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tolerance: String,
    /// Where a linear dimension meets a circle or arc it measures to: its centre (empty), the
    /// near side of it (`min`: between the arcs) or the far side (`max`: over them).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub arc: String,
    /// Where it was put, in its view's own coordinates (model mm about the view's centre, so it
    /// moves with the view): a linear dimension's line runs through it and its text sits there; a
    /// diameter's or radius' leader runs out to it; an angle's arc passes through it. None: `offset`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<[f64; 2]>,
}

/// A note on the sheet.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DrawNote {
    pub id: u64,
    pub at: [f64; 2],
    pub text: String,
    /// Text height (mm).
    #[serde(default = "note_height")]
    pub height: f64,
}

fn note_height() -> f64 {
    3.5
}

/// A drawing: one sheet and what is on it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Drawing {
    pub id: u64,
    pub name: String,
    #[serde(default)]
    pub sheet: Sheet,
    /// The views' scale (`1:2`).
    #[serde(default = "full_size")]
    pub scale: String,
    #[serde(default)]
    pub projection: Projection,
    #[serde(default)]
    pub title: TitleBlock,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub views: Vec<DrawView>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dims: Vec<DrawDim>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<DrawNote>,
    /// The bodies drawn: empty for every visible one.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bodies: Vec<String>,
    #[serde(default)]
    pub next_id: u64,
}

fn full_size() -> String {
    "1:1".into()
}

impl Drawing {
    pub fn new(id: u64, name: &str) -> Drawing {
        Drawing {
            id,
            name: name.into(),
            sheet: Sheet::default(),
            scale: full_size(),
            projection: Projection::First,
            title: TitleBlock::default(),
            views: Vec::new(),
            dims: Vec::new(),
            notes: Vec::new(),
            bodies: Vec::new(),
            next_id: 1,
        }
    }

    /// A new id for a view, dimension or note.
    pub fn take_id(&mut self) -> u64 {
        let used = self.views.iter().map(|v| v.id).chain(self.dims.iter().map(|d| d.id)).chain(self.notes.iter().map(|n| n.id)).max().unwrap_or(0);
        self.next_id = self.next_id.max(used + 1);
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn view(&self, id: u64) -> Option<&DrawView> {
        self.views.iter().find(|v| v.id == id)
    }

    /// The drawing's scale factor (1:1 when it doesn't read).
    pub fn scale_factor(&self) -> f64 {
        parse_scale(&self.scale).unwrap_or(1.0)
    }

    /// A view's scale factor: its own, a detail's twice its parent's, else the drawing's.
    pub fn view_scale(&self, v: &DrawView) -> f64 {
        if let Some(s) = v.scale.as_deref().and_then(parse_scale) {
            return s;
        }
        match &v.kind {
            ViewKind::Detail { parent, .. } => {
                self.view(*parent).filter(|p| p.id != v.id).map_or(self.scale_factor(), |p| self.view_scale_once(p)) * 2.0
            }
            _ => self.scale_factor(),
        }
    }

    fn view_scale_once(&self, v: &DrawView) -> f64 {
        v.scale.as_deref().and_then(parse_scale).unwrap_or(self.scale_factor())
    }

    /// The views that hang off `id`, directly or further down.
    pub fn descendants(&self, id: u64) -> Vec<u64> {
        let mut out = vec![id];
        let mut i = 0;
        while let Some(&cur) = out.get(i) {
            for v in &self.views {
                let p = match &v.kind {
                    ViewKind::Base { .. } | ViewKind::Flat { .. } => None,
                    ViewKind::Projected { parent, .. } | ViewKind::Section { parent, .. } | ViewKind::Detail { parent, .. } => Some(*parent),
                };
                if p == Some(cur) && !out.contains(&v.id) && out.len() < MAX_ITEMS {
                    out.push(v.id);
                }
            }
            i += 1;
        }
        out
    }

    /// The next free letter for a section or detail view (A, B, … skipping I, O, Q).
    pub fn next_label(&self) -> String {
        let used: Vec<&str> = self.views.iter().map(|v| v.label.as_str()).collect();
        let letters = "ABCDEFGHJKLMNPRSTUVWXYZ";
        for c in letters.chars() {
            let s = c.to_string();
            if !used.contains(&s.as_str()) {
                return s;
            }
        }
        for n in 2.. {
            for c in letters.chars() {
                let s = format!("{c}{n}");
                if !used.contains(&s.as_str()) {
                    return s;
                }
            }
        }
        String::new()
    }

    /// A drawing whose numbers all make sense (a damaged file is refused, not drawn).
    pub fn is_sane(&self) -> bool {
        let finite2 = |p: &[f64; 2]| p.iter().all(|x| x.is_finite() && x.abs() < 1e6);
        self.views.len() <= MAX_ITEMS
            && self.dims.len() <= MAX_ITEMS
            && self.notes.len() <= MAX_ITEMS
            && self.views.iter().all(|v| {
                finite2(&v.at)
                    && match &v.kind {
                        ViewKind::Section { a, b, .. } => finite2(a) && finite2(b),
                        ViewKind::Detail { center, radius, .. } => finite2(center) && radius.is_finite() && *radius > 0.0,
                        _ => true,
                    }
            })
            && self.dims.iter().all(|d| d.offset.is_finite() && d.at.is_none_or(|p| finite2(&p)) && d.refs.len() <= 3 && d.refs.iter().all(|r| r.point.is_finite()))
            && self.notes.iter().all(|n| finite2(&n.at) && n.height.is_finite() && n.height > 0.0 && n.height < 100.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scales_and_sheets_read() {
        assert_eq!(parse_scale("1:2"), Some(0.5));
        assert_eq!(parse_scale(" 5 : 1 "), Some(5.0));
        assert_eq!(parse_scale("1:2,5"), Some(0.4));
        for bad in ["", "1", "1:0", "0:1", "a:b", "1:1e9"] {
            assert_eq!(parse_scale(bad), None, "{bad}");
        }
        assert!(SCALES.iter().all(|s| parse_scale(s).is_some()));
        assert_eq!(Sheet { size: "a4".into(), landscape: false }.dims(), (210.0, 297.0));
        assert_eq!(Sheet { size: "?".into(), landscape: true }.dims(), (420.0, 297.0));
    }

    #[test]
    fn ids_labels_and_descendants() {
        let mut d = Drawing::new(1, "D");
        let base = d.take_id();
        d.views.push(DrawView {
            id: base,
            kind: ViewKind::Base { orientation: "front".into() },
            at: [100.0, 100.0],
            scale: None,
            label: String::new(),
            hidden: false,
        });
        let side = d.take_id();
        d.views.push(DrawView {
            id: side,
            kind: ViewKind::Projected { parent: base, side: "right".into() },
            at: [200.0, 100.0],
            scale: None,
            label: String::new(),
            hidden: false,
        });
        let sec = d.take_id();
        d.views.push(DrawView {
            id: sec,
            kind: ViewKind::Section { parent: side, a: [0.0, -10.0], b: [0.0, 10.0], flip: false },
            at: [300.0, 100.0],
            scale: None,
            label: d.next_label(),
            hidden: false,
        });
        assert_eq!(d.views.last().map(|v| v.label.as_str()), Some("A"));
        assert_eq!(d.next_label(), "B");
        assert_eq!(d.descendants(base), vec![base, side, sec]);
        assert_eq!(d.descendants(sec), vec![sec]);
        assert!(d.take_id() > sec && d.is_sane());
        // A view naming itself as its parent ends the walk.
        d.views.push(DrawView {
            id: 99,
            kind: ViewKind::Detail { parent: 99, center: [0.0, 0.0], radius: 5.0 },
            at: [0.0, 0.0],
            scale: None,
            label: "B".into(),
            hidden: false,
        });
        assert_eq!(d.descendants(99), vec![99]);
        assert!(d.view_scale(d.view(99).unwrap()) > 0.0);
        let json = serde_json::to_string(&d).unwrap();
        let back: Drawing = serde_json::from_str(&json).unwrap();
        assert_eq!(back, d);
    }
}
