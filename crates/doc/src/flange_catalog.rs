//! Plate flanges for welding (EN 1092-1 type 01), PN 6, PN 10 and PN 16, DN 10…600, in
//! millimetres: the pipe's outside diameter, the flange's bore, outside diameter, bolt circle,
//! bolt holes (diameter and count) and bolt size, and its thickness.
//!
//! Data: the dimension tables of projectmaterials.com ("Dimensions and weights of EN 1092-1
//! Type 01 plate flanges"), checked against wermac.org (PN 16, weld neck: the same connecting
//! dimensions) and dutypoint.com (PN 6, 10, 16 connecting dimensions). Where they differ, at
//! DN 65 PN 10/16, the bolt count is EN 1092-1's 8 (older DIN flanges had 4: give `holes`).
//! PN 10 below DN 200 shares PN 16's connecting dimensions; its thickness here is PN 16's.

/// One size of one pressure class.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FlangeSize {
    pub dn: u16,
    /// The pipe's outside diameter the flange takes.
    pub pipe: f64,
    /// The flange's bore.
    pub bore: f64,
    /// The flange's outside diameter.
    pub outside: f64,
    /// The bolt circle's diameter.
    pub pitch: f64,
    /// A bolt hole's diameter.
    pub hole: f64,
    pub holes: u16,
    pub bolt: &'static str,
    pub thickness: f64,
}

const fn f(dn: u16, pipe: f64, bore: f64, pitch: f64, outside: f64, hole: f64, holes: u16, bolt: &'static str, thickness: f64) -> FlangeSize {
    FlangeSize { dn, pipe, bore, outside, pitch, hole, holes, bolt, thickness }
}

const PN6: &[FlangeSize] = &[
    f(15, 21.3, 22.0, 55.0, 80.0, 11.0, 4, "M10", 12.0),
    f(20, 26.9, 27.5, 65.0, 90.0, 11.0, 4, "M10", 14.0),
    f(25, 33.7, 34.5, 75.0, 100.0, 11.0, 4, "M10", 14.0),
    f(32, 42.4, 43.5, 90.0, 120.0, 14.0, 4, "M12", 16.0),
    f(40, 48.3, 49.5, 100.0, 130.0, 14.0, 4, "M12", 16.0),
    f(50, 60.3, 61.5, 110.0, 140.0, 14.0, 4, "M12", 16.0),
    f(65, 76.1, 77.5, 130.0, 160.0, 14.0, 4, "M12", 16.0),
    f(80, 88.9, 90.5, 150.0, 190.0, 18.0, 4, "M16", 18.0),
    f(100, 114.3, 116.0, 170.0, 210.0, 18.0, 4, "M16", 18.0),
    f(125, 139.7, 141.5, 200.0, 240.0, 18.0, 8, "M16", 20.0),
    f(150, 168.3, 170.5, 225.0, 265.0, 18.0, 8, "M16", 20.0),
    f(200, 219.1, 221.5, 280.0, 320.0, 18.0, 8, "M16", 22.0),
    f(250, 273.0, 276.5, 335.0, 375.0, 18.0, 12, "M16", 24.0),
    f(300, 323.9, 327.5, 395.0, 440.0, 22.0, 12, "M20", 24.0),
    f(350, 355.6, 359.5, 445.0, 490.0, 22.0, 12, "M20", 26.0),
    f(400, 406.4, 411.0, 495.0, 540.0, 22.0, 16, "M20", 28.0),
    f(450, 457.0, 462.0, 550.0, 595.0, 22.0, 16, "M20", 30.0),
    f(500, 508.0, 513.5, 600.0, 645.0, 22.0, 20, "M20", 30.0),
    f(600, 610.0, 616.5, 705.0, 755.0, 26.0, 20, "M24", 32.0),
];

const PN10_LARGE: &[FlangeSize] = &[
    f(200, 219.1, 221.5, 295.0, 340.0, 22.0, 8, "M20", 24.0),
    f(250, 273.0, 276.5, 350.0, 395.0, 22.0, 12, "M20", 26.0),
    f(300, 323.9, 327.5, 400.0, 445.0, 22.0, 12, "M20", 26.0),
    f(350, 355.6, 359.5, 460.0, 505.0, 22.0, 16, "M20", 30.0),
    f(400, 406.4, 411.0, 515.0, 565.0, 26.0, 16, "M24", 32.0),
    f(450, 457.2, 462.0, 565.0, 615.0, 26.0, 20, "M24", 36.0),
    f(500, 508.0, 513.5, 620.0, 670.0, 26.0, 20, "M24", 38.0),
    f(600, 610.0, 616.5, 725.0, 780.0, 30.0, 20, "M27", 42.0),
];

const PN16: &[FlangeSize] = &[
    f(10, 17.2, 18.0, 60.0, 90.0, 14.0, 4, "M12", 14.0),
    f(15, 21.3, 22.0, 65.0, 95.0, 14.0, 4, "M12", 14.0),
    f(20, 26.9, 27.5, 75.0, 105.0, 14.0, 4, "M12", 16.0),
    f(25, 33.7, 34.5, 85.0, 115.0, 14.0, 4, "M12", 16.0),
    f(32, 42.4, 43.5, 100.0, 140.0, 18.0, 4, "M16", 18.0),
    f(40, 48.3, 49.5, 110.0, 150.0, 18.0, 4, "M16", 18.0),
    f(50, 60.3, 61.5, 125.0, 165.0, 18.0, 4, "M16", 20.0),
    f(65, 76.1, 77.5, 145.0, 185.0, 18.0, 8, "M16", 20.0),
    f(80, 88.9, 90.5, 160.0, 200.0, 18.0, 8, "M16", 20.0),
    f(100, 114.3, 116.0, 180.0, 220.0, 18.0, 8, "M16", 22.0),
    f(125, 139.7, 141.5, 210.0, 250.0, 18.0, 8, "M16", 22.0),
    f(150, 168.3, 170.5, 240.0, 285.0, 22.0, 8, "M20", 24.0),
    f(200, 219.1, 221.5, 295.0, 340.0, 22.0, 12, "M20", 26.0),
    f(250, 273.0, 276.5, 355.0, 405.0, 26.0, 12, "M24", 29.0),
    f(300, 323.9, 327.5, 410.0, 460.0, 26.0, 12, "M24", 32.0),
    f(350, 355.6, 359.0, 470.0, 520.0, 26.0, 16, "M24", 35.0),
    f(400, 406.4, 411.0, 525.0, 580.0, 30.0, 16, "M27", 38.0),
    f(450, 457.0, 462.0, 585.0, 640.0, 30.0, 20, "M27", 42.0),
    f(500, 508.0, 513.5, 650.0, 715.0, 33.0, 20, "M30", 46.0),
    f(600, 610.0, 616.5, 770.0, 840.0, 36.0, 20, "M33", 55.0),
];

/// The pressure classes the catalogue has.
pub const CLASSES: [u16; 3] = [6, 10, 16];

/// A plate flange's size, `None` when the class has no such DN.
pub fn size(pn: u16, dn: u16) -> Option<FlangeSize> {
    let table = |t: &[FlangeSize]| t.iter().find(|s| s.dn == dn).copied();
    match pn {
        6 => table(PN6),
        10 if dn < 200 => table(PN16),
        10 => table(PN10_LARGE),
        16 => table(PN16),
        _ => None,
    }
}

/// The DNs of a class, smallest first.
pub fn sizes(pn: u16) -> Vec<u16> {
    match pn {
        6 => PN6.iter().map(|s| s.dn).collect(),
        10 => PN16.iter().filter(|s| s.dn < 200).chain(PN10_LARGE).map(|s| s.dn).collect(),
        16 => PN16.iter().map(|s| s.dn).collect(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every size is a flange that holds together: the bore inside the holes, the holes inside
    /// the rim, and room between neighbouring holes; the tables run from small to large.
    #[test]
    fn every_size_is_a_sound_flange() {
        for pn in CLASSES {
            let dns = sizes(pn);
            assert!(dns.windows(2).all(|w| w[0] < w[1]), "PN {pn}: {dns:?}");
            for dn in dns {
                let Some(s) = size(pn, dn) else { panic!("PN {pn} DN {dn}") };
                assert!(s.pipe < s.bore && s.bore < s.pitch - s.hole, "PN {pn} DN {dn}: {s:?}");
                assert!(s.pitch + s.hole < s.outside, "PN {pn} DN {dn}: {s:?}");
                let gap = s.pitch * (std::f64::consts::PI / s.holes as f64).sin() - s.hole;
                assert!(gap > 3.0 && s.thickness > 10.0, "PN {pn} DN {dn}: {s:?}");
            }
        }
    }

    /// A few sizes as the tables print them; PN 10 small is PN 16's, PN 10 large its own.
    #[test]
    fn known_sizes() {
        let dn50 = size(16, 50).map(|s| (s.outside, s.pitch, s.holes, s.hole, s.thickness));
        assert_eq!(dn50, Some((165.0, 125.0, 4, 18.0, 20.0)));
        assert_eq!(size(10, 100), size(16, 100));
        assert_eq!(size(10, 200).map(|s| s.holes), Some(8));
        assert_eq!(size(16, 200).map(|s| s.holes), Some(12));
        assert_eq!(size(6, 100).map(|s| (s.outside, s.pitch, s.bolt)), Some((210.0, 170.0, "M16")));
        assert!(size(25, 50).is_none() && size(6, 10).is_none() && size(16, 55).is_none());
    }
}
