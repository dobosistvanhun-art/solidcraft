//! Standard pipe parts, their sizes in millimetres: weld neck flanges (EN 1092-1 type 11),
//! loose plate flanges (type 02) with their plate collars (type 32), the lapped pipe end a
//! loose flange takes (type 33, a bördel), threaded sockets (EN 10241) and threaded ends on
//! medium steel pipe (EN 10255, an ISO 7-1 thread). The plate flanges (type 01) are in
//! [`crate::flange_catalog`].
//!
//! EN 1092-1 gives the same connecting sizes as ISO 7005-1 (PN series) and DIN 2501; older DIN
//! flanges differ at DN 65 PN 10/16, with 4 bolt holes (see [`Standard::Din`]).
//!
//! Data:
//! - type 11: wermac.org, "Dimensions Weld Neck Flanges Type 11 DIN EN 1092/1" PN 10, 16, 25, 40;
//! - type 02 and 32: zickwolff.de, "Loser Flansch Typ 02 und 04 DIN EN 1092-1", checked against
//!   heco.de (PN 16 loose flanges); PN 16 below DN 50 is PN 10's (the same connecting sizes);
//! - EN 10241 sockets: the Delcorte data sheet "Mild steel full & half socket EN 10241";
//! - EN 10255 medium pipe: the valvestubesfittings.com data sheet; a threaded end's thread
//!   length: engineeringtoolbox.com, "ISO 7 - length of thread to make a tight joint".

/// Whose sizes a flange is made to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Standard {
    /// EN 1092-1 (and ISO 7005-1, the same connecting sizes).
    En,
    /// DIN 2501 flanges: EN 1092-1's, but DN 65 PN 10/16 has 4 bolt holes.
    Din,
}

/// A weld neck flange (type 11).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WeldNeck {
    pub dn: u16,
    /// The flange's outside diameter, its bolt circle and raised face's diameters.
    pub outside: f64,
    pub pitch: f64,
    pub face: f64,
    /// The neck's wall and outside diameter (the pipe it is welded to), the hub's diameter at
    /// the plate.
    pub wall: f64,
    pub neck: f64,
    pub hub: f64,
    pub hole: f64,
    /// The raised face's height, the plate's thickness (with it), the flange's length (with
    /// it) and the neck's straight end.
    pub face_height: f64,
    pub thickness: f64,
    pub length: f64,
    pub straight: f64,
    pub holes: u16,
    pub bolt: &'static str,
}

#[allow(clippy::too_many_arguments)]
const fn wn(
    dn: u16,
    outside: f64,
    pitch: f64,
    face: f64,
    wall: f64,
    neck: f64,
    hub: f64,
    hole: f64,
    face_height: f64,
    thickness: f64,
    length: f64,
    straight: f64,
    holes: u16,
    bolt: &'static str,
) -> WeldNeck {
    WeldNeck { dn, outside, pitch, face, wall, neck, hub, hole, face_height, thickness, length, straight, holes, bolt }
}

/// DN 10…150, the same at PN 10, 16 (and 25, 40 up to DN 40).
const WN_SMALL: &[WeldNeck] = &[
    wn(10, 90.0, 60.0, 40.0, 2.0, 17.2, 28.0, 14.0, 2.0, 16.0, 35.0, 6.0, 4, "M12"),
    wn(15, 95.0, 65.0, 45.0, 2.0, 21.3, 32.0, 14.0, 2.0, 16.0, 38.0, 6.0, 4, "M12"),
    wn(20, 105.0, 75.0, 58.0, 2.3, 26.9, 40.0, 14.0, 2.0, 18.0, 40.0, 6.0, 4, "M12"),
    wn(25, 115.0, 85.0, 68.0, 2.6, 33.7, 46.0, 14.0, 2.0, 18.0, 40.0, 6.0, 4, "M12"),
    wn(32, 140.0, 100.0, 78.0, 2.6, 42.4, 56.0, 18.0, 2.0, 18.0, 42.0, 6.0, 4, "M16"),
    wn(40, 150.0, 110.0, 88.0, 2.6, 48.3, 64.0, 18.0, 3.0, 18.0, 45.0, 7.0, 4, "M16"),
    wn(50, 165.0, 125.0, 102.0, 2.9, 60.3, 74.0, 18.0, 3.0, 18.0, 45.0, 8.0, 4, "M16"),
    wn(65, 185.0, 145.0, 122.0, 2.9, 76.1, 92.0, 18.0, 3.0, 18.0, 45.0, 10.0, 8, "M16"),
    wn(80, 200.0, 160.0, 138.0, 3.2, 88.9, 105.0, 18.0, 3.0, 20.0, 50.0, 10.0, 8, "M16"),
    wn(100, 220.0, 180.0, 158.0, 3.6, 114.3, 131.0, 18.0, 3.0, 20.0, 52.0, 12.0, 8, "M16"),
    wn(125, 250.0, 210.0, 188.0, 4.0, 139.7, 156.0, 18.0, 3.0, 22.0, 55.0, 12.0, 8, "M16"),
    wn(150, 285.0, 240.0, 212.0, 4.5, 168.3, 184.0, 22.0, 3.0, 22.0, 55.0, 12.0, 8, "M20"),
];

const WN_PN10: &[WeldNeck] = &[
    wn(200, 340.0, 295.0, 268.0, 6.3, 219.1, 234.0, 22.0, 3.0, 24.0, 62.0, 16.0, 8, "M20"),
    wn(250, 395.0, 350.0, 320.0, 6.3, 273.0, 292.0, 22.0, 3.0, 26.0, 68.0, 16.0, 12, "M20"),
    wn(300, 445.0, 400.0, 370.0, 7.1, 323.9, 342.0, 22.0, 4.0, 26.0, 68.0, 16.0, 12, "M20"),
    wn(350, 505.0, 460.0, 430.0, 7.1, 355.6, 385.0, 22.0, 4.0, 26.0, 68.0, 16.0, 16, "M20"),
    wn(400, 565.0, 515.0, 482.0, 7.1, 406.4, 440.0, 26.0, 4.0, 26.0, 72.0, 16.0, 16, "M24"),
    wn(450, 615.0, 565.0, 532.0, 7.1, 457.0, 488.0, 26.0, 4.0, 28.0, 72.0, 16.0, 20, "M24"),
    wn(500, 670.0, 620.0, 585.0, 7.1, 508.0, 542.0, 26.0, 4.0, 28.0, 75.0, 16.0, 20, "M24"),
    wn(600, 780.0, 725.0, 685.0, 8.0, 610.0, 642.0, 30.0, 5.0, 30.0, 82.0, 18.0, 20, "M27"),
];

const WN_PN16: &[WeldNeck] = &[
    wn(200, 340.0, 295.0, 268.0, 6.3, 219.1, 235.0, 22.0, 3.0, 24.0, 62.0, 16.0, 12, "M20"),
    wn(250, 405.0, 355.0, 320.0, 6.3, 273.0, 292.0, 26.0, 3.0, 26.0, 70.0, 16.0, 12, "M24"),
    wn(300, 460.0, 410.0, 378.0, 7.1, 323.9, 344.0, 26.0, 4.0, 28.0, 78.0, 16.0, 12, "M24"),
    wn(350, 520.0, 470.0, 438.0, 8.0, 355.6, 390.0, 26.0, 4.0, 30.0, 82.0, 16.0, 16, "M24"),
    wn(400, 580.0, 525.0, 490.0, 8.0, 406.4, 445.0, 30.0, 4.0, 32.0, 85.0, 16.0, 16, "M27"),
    wn(450, 640.0, 585.0, 550.0, 8.0, 457.0, 490.0, 30.0, 4.0, 34.0, 83.0, 16.0, 20, "M27"),
    wn(500, 715.0, 650.0, 610.0, 8.0, 508.0, 548.0, 33.0, 4.0, 36.0, 84.0, 16.0, 20, "M30"),
    wn(600, 840.0, 770.0, 725.0, 10.0, 610.0, 670.0, 36.0, 5.0, 40.0, 88.0, 18.0, 20, "M33"),
];

/// PN 25 and 40 from DN 50 (below it, the small sizes).
const WN_PN25: &[WeldNeck] = &[
    wn(50, 165.0, 125.0, 102.0, 2.9, 60.3, 75.0, 18.0, 3.0, 20.0, 48.0, 8.0, 4, "M16"),
    wn(65, 185.0, 145.0, 122.0, 2.9, 76.1, 90.0, 18.0, 3.0, 22.0, 52.0, 10.0, 8, "M16"),
    wn(80, 200.0, 160.0, 138.0, 3.2, 88.9, 105.0, 18.0, 3.0, 24.0, 58.0, 12.0, 8, "M16"),
    wn(100, 235.0, 190.0, 162.0, 3.6, 114.3, 134.0, 22.0, 3.0, 24.0, 65.0, 12.0, 8, "M20"),
    wn(125, 270.0, 220.0, 188.0, 4.0, 139.7, 162.0, 26.0, 3.0, 26.0, 68.0, 12.0, 8, "M24"),
    wn(150, 300.0, 250.0, 218.0, 4.5, 168.3, 192.0, 26.0, 3.0, 28.0, 75.0, 12.0, 8, "M24"),
    wn(200, 360.0, 310.0, 278.0, 6.3, 219.1, 244.0, 26.0, 3.0, 30.0, 80.0, 16.0, 12, "M24"),
    wn(250, 425.0, 370.0, 335.0, 7.1, 273.0, 298.0, 30.0, 3.0, 32.0, 88.0, 18.0, 12, "M27"),
    wn(300, 485.0, 430.0, 395.0, 8.0, 323.9, 352.0, 30.0, 4.0, 34.0, 92.0, 18.0, 16, "M27"),
    wn(350, 555.0, 490.0, 450.0, 8.0, 355.6, 398.0, 33.0, 4.0, 38.0, 100.0, 20.0, 16, "M30"),
    wn(400, 620.0, 550.0, 505.0, 8.8, 406.4, 452.0, 36.0, 4.0, 40.0, 110.0, 20.0, 16, "M33"),
    wn(450, 670.0, 600.0, 555.0, 8.8, 457.0, 500.0, 36.0, 4.0, 46.0, 110.0, 20.0, 20, "M33"),
    wn(500, 730.0, 660.0, 615.0, 10.0, 508.0, 558.0, 36.0, 4.0, 48.0, 125.0, 20.0, 20, "M33"),
    wn(600, 845.0, 770.0, 720.0, 11.0, 610.0, 660.0, 39.0, 5.0, 48.0, 125.0, 20.0, 20, "M36"),
];

const WN_PN40: &[WeldNeck] = &[
    wn(50, 165.0, 125.0, 102.0, 2.9, 60.3, 75.0, 18.0, 3.0, 20.0, 48.0, 8.0, 4, "M16"),
    wn(65, 185.0, 145.0, 122.0, 2.9, 76.1, 90.0, 18.0, 3.0, 22.0, 52.0, 10.0, 8, "M16"),
    wn(80, 200.0, 160.0, 138.0, 3.2, 88.9, 105.0, 18.0, 3.0, 24.0, 58.0, 12.0, 8, "M16"),
    wn(100, 235.0, 190.0, 162.0, 3.6, 114.3, 134.0, 22.0, 3.0, 24.0, 65.0, 12.0, 8, "M20"),
    wn(125, 270.0, 220.0, 188.0, 4.0, 139.7, 162.0, 26.0, 3.0, 26.0, 68.0, 12.0, 8, "M24"),
    wn(150, 300.0, 250.0, 218.0, 4.5, 168.3, 192.0, 26.0, 3.0, 28.0, 75.0, 12.0, 8, "M24"),
    wn(200, 375.0, 320.0, 285.0, 6.3, 219.1, 244.0, 30.0, 3.0, 34.0, 88.0, 16.0, 12, "M27"),
    wn(250, 450.0, 385.0, 345.0, 7.1, 273.0, 306.0, 33.0, 3.0, 38.0, 105.0, 18.0, 12, "M30"),
    wn(300, 515.0, 450.0, 410.0, 8.0, 323.9, 362.0, 33.0, 4.0, 42.0, 115.0, 18.0, 16, "M30"),
    wn(350, 580.0, 510.0, 465.0, 8.8, 355.6, 408.0, 36.0, 4.0, 46.0, 125.0, 20.0, 16, "M33"),
    wn(400, 660.0, 585.0, 535.0, 11.0, 406.4, 462.0, 39.0, 4.0, 50.0, 135.0, 20.0, 16, "M36"),
    wn(450, 685.0, 610.0, 560.0, 12.5, 457.0, 500.0, 39.0, 4.0, 57.0, 135.0, 20.0, 20, "M36"),
    wn(500, 755.0, 670.0, 615.0, 14.2, 508.0, 562.0, 42.0, 4.0, 57.0, 140.0, 20.0, 20, "M39"),
    wn(600, 890.0, 795.0, 735.0, 16.0, 610.0, 666.0, 48.0, 5.0, 72.0, 150.0, 20.0, 20, "M45"),
];

/// A weld neck flange's size, `None` when its class has no such DN.
pub fn weld_neck(pn: u16, dn: u16, standard: Standard) -> Option<WeldNeck> {
    let find = |t: &[WeldNeck]| t.iter().find(|s| s.dn == dn).copied();
    let s = match pn {
        10 | 16 if dn <= 150 => find(WN_SMALL),
        25 | 40 if dn <= 40 => find(WN_SMALL),
        10 => find(WN_PN10),
        16 => find(WN_PN16),
        25 => find(WN_PN25),
        40 => find(WN_PN40),
        _ => None,
    }?;
    Some(WeldNeck { holes: din_holes(standard, pn, dn, s.holes), ..s })
}

/// A loose plate flange (type 02) and the plate collar (type 32) it goes behind.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Loose {
    pub dn: u16,
    /// The pipe's outside diameter.
    pub pipe: f64,
    pub holes: u16,
    pub hole: f64,
    pub bolt: &'static str,
    /// The collar's outside diameter (its sealing face) and thickness.
    pub collar: f64,
    pub collar_thickness: f64,
    /// The flange's bolt circle, outside diameter, bore and thickness.
    pub pitch: f64,
    pub outside: f64,
    pub bore: f64,
    pub thickness: f64,
}

#[allow(clippy::too_many_arguments)]
const fn lf(
    dn: u16,
    pipe: f64,
    holes: u16,
    hole: f64,
    bolt: &'static str,
    collar: f64,
    pitch: f64,
    outside: f64,
    bore: f64,
    thickness: f64,
    collar_thickness: f64,
) -> Loose {
    Loose { dn, pipe, holes, hole, bolt, collar, collar_thickness, pitch, outside, bore, thickness }
}

const LOOSE_PN10: &[Loose] = &[
    lf(10, 17.2, 4, 14.0, "M12", 40.0, 60.0, 90.0, 21.0, 14.0, 12.0),
    lf(15, 21.3, 4, 14.0, "M12", 45.0, 65.0, 95.0, 25.0, 14.0, 12.0),
    lf(20, 26.9, 4, 14.0, "M12", 58.0, 75.0, 105.0, 31.0, 16.0, 14.0),
    lf(25, 33.7, 4, 14.0, "M12", 68.0, 85.0, 115.0, 38.0, 16.0, 14.0),
    lf(32, 42.4, 4, 18.0, "M16", 78.0, 100.0, 140.0, 47.0, 18.0, 14.0),
    lf(40, 48.3, 4, 18.0, "M16", 88.0, 110.0, 150.0, 53.0, 18.0, 14.0),
    lf(50, 60.3, 4, 18.0, "M16", 102.0, 125.0, 165.0, 65.0, 20.0, 16.0),
    lf(65, 76.1, 8, 18.0, "M16", 122.0, 145.0, 185.0, 81.0, 20.0, 16.0),
    lf(80, 88.9, 8, 18.0, "M16", 138.0, 160.0, 200.0, 94.0, 20.0, 16.0),
    lf(100, 114.3, 8, 18.0, "M16", 158.0, 180.0, 220.0, 120.0, 22.0, 18.0),
    lf(125, 139.7, 8, 18.0, "M16", 188.0, 210.0, 250.0, 145.0, 22.0, 18.0),
    lf(150, 168.3, 8, 22.0, "M20", 212.0, 240.0, 285.0, 174.0, 24.0, 20.0),
    lf(200, 219.1, 8, 22.0, "M20", 268.0, 295.0, 340.0, 226.0, 24.0, 20.0),
    lf(250, 273.0, 12, 22.0, "M20", 320.0, 350.0, 395.0, 281.0, 26.0, 22.0),
    lf(300, 323.9, 12, 22.0, "M20", 370.0, 400.0, 445.0, 333.0, 26.0, 22.0),
    lf(350, 355.6, 16, 22.0, "M20", 430.0, 460.0, 505.0, 365.0, 30.0, 22.0),
    lf(400, 406.4, 16, 26.0, "M24", 482.0, 515.0, 565.0, 416.0, 32.0, 24.0),
    lf(450, 457.0, 20, 26.0, "M24", 532.0, 565.0, 615.0, 467.0, 36.0, 24.0),
    lf(500, 508.0, 20, 26.0, "M24", 585.0, 620.0, 670.0, 519.0, 38.0, 26.0),
    lf(600, 610.0, 20, 30.0, "M27", 685.0, 725.0, 780.0, 622.0, 42.0, 26.0),
];

/// From DN 50; below it, PN 10's.
const LOOSE_PN16: &[Loose] = &[
    lf(50, 60.3, 4, 18.0, "M16", 102.0, 125.0, 165.0, 65.0, 20.0, 16.0),
    lf(65, 76.1, 8, 18.0, "M16", 122.0, 145.0, 185.0, 81.0, 20.0, 16.0),
    lf(80, 88.9, 8, 18.0, "M16", 138.0, 160.0, 200.0, 94.0, 20.0, 16.0),
    lf(100, 114.3, 8, 18.0, "M16", 158.0, 180.0, 220.0, 120.0, 22.0, 18.0),
    lf(125, 139.7, 8, 18.0, "M16", 188.0, 210.0, 250.0, 145.0, 22.0, 18.0),
    lf(150, 168.3, 8, 22.0, "M20", 212.0, 240.0, 285.0, 174.0, 24.0, 20.0),
    lf(200, 219.1, 12, 22.0, "M20", 268.0, 295.0, 340.0, 226.0, 26.0, 20.0),
    lf(250, 273.0, 12, 26.0, "M24", 320.0, 355.0, 405.0, 281.0, 29.0, 22.0),
    lf(300, 323.9, 12, 26.0, "M24", 378.0, 410.0, 460.0, 333.0, 32.0, 24.0),
    lf(350, 355.6, 16, 26.0, "M24", 438.0, 470.0, 520.0, 365.0, 35.0, 26.0),
    lf(400, 406.4, 16, 30.0, "M27", 490.0, 525.0, 580.0, 416.0, 38.0, 28.0),
    lf(450, 457.0, 20, 30.0, "M27", 550.0, 585.0, 640.0, 467.0, 42.0, 30.0),
    lf(500, 508.0, 20, 33.0, "M30", 610.0, 650.0, 715.0, 519.0, 46.0, 32.0),
    lf(600, 610.0, 20, 36.0, "M33", 725.0, 770.0, 840.0, 622.0, 55.0, 32.0),
];

/// A loose flange's (and its collar's) size, `None` when its class has no such DN.
pub fn loose(pn: u16, dn: u16, standard: Standard) -> Option<Loose> {
    let find = |t: &[Loose]| t.iter().find(|s| s.dn == dn).copied();
    let s = match pn {
        10 => find(LOOSE_PN10),
        16 if dn < 50 => find(LOOSE_PN10),
        16 => find(LOOSE_PN16),
        _ => None,
    }?;
    Some(Loose { holes: din_holes(standard, pn, dn, s.holes), ..s })
}

/// DIN 2501's DN 65 PN 10/16: 4 bolt holes where EN 1092-1 has 8.
fn din_holes(standard: Standard, pn: u16, dn: u16, holes: u16) -> u16 {
    if standard == Standard::Din && dn == 65 && matches!(pn, 10 | 16) { 4 } else { holes }
}

/// The DNs a flange type (`"11"`, `"02"`) has in a class, smallest first.
pub fn flange_sizes(kind: &str, pn: u16) -> Vec<u16> {
    let all = [10, 15, 20, 25, 32, 40, 50, 65, 80, 100, 125, 150, 200, 250, 300, 350, 400, 450, 500, 600];
    all.into_iter()
        .filter(|dn| match kind {
            "11" => weld_neck(pn, *dn, Standard::En).is_some(),
            "02" => loose(pn, *dn, Standard::En).is_some(),
            _ => crate::flange_catalog::size(pn, *dn).is_some(),
        })
        .collect()
}

/// The pressure classes a flange type has.
pub fn flange_classes(kind: &str) -> &'static [u16] {
    match kind {
        "11" => &[10, 16, 25, 40],
        "02" => &[10, 16],
        _ => &crate::flange_catalog::CLASSES,
    }
}

/// A medium steel pipe (EN 10255 M), by its thread size: its outside diameter and wall, and how
/// long a thread on it makes a tight joint (ISO 7-1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pipe {
    pub size: &'static str,
    pub outside: f64,
    pub wall: f64,
    pub thread: f64,
}

const PIPES: &[Pipe] = &[
    Pipe { size: "1/4", outside: 13.5, wall: 2.35, thread: 9.9 },
    Pipe { size: "3/8", outside: 17.2, wall: 2.35, thread: 10.4 },
    Pipe { size: "1/2", outside: 21.3, wall: 2.65, thread: 13.5 },
    Pipe { size: "3/4", outside: 26.9, wall: 2.65, thread: 14.0 },
    Pipe { size: "1", outside: 33.7, wall: 3.25, thread: 16.8 },
    Pipe { size: "1 1/4", outside: 42.4, wall: 3.25, thread: 17.3 },
    Pipe { size: "1 1/2", outside: 48.3, wall: 3.25, thread: 17.3 },
    Pipe { size: "2", outside: 60.3, wall: 3.65, thread: 17.8 },
    Pipe { size: "2 1/2", outside: 76.1, wall: 3.65, thread: 23.6 },
    Pipe { size: "3", outside: 88.9, wall: 4.05, thread: 25.9 },
    Pipe { size: "4", outside: 114.3, wall: 4.5, thread: 27.7 },
    Pipe { size: "5", outside: 139.7, wall: 4.85, thread: 30.2 },
    Pipe { size: "6", outside: 165.1, wall: 4.85, thread: 30.7 },
];

/// A threaded socket (EN 10241): its outside diameter, a full one's and a half one's length.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Socket {
    pub size: &'static str,
    pub outside: f64,
    pub full: f64,
    pub half: f64,
}

const SOCKETS: &[Socket] = &[
    Socket { size: "3/8", outside: 22.0, full: 28.0, half: 12.0 },
    Socket { size: "1/2", outside: 27.0, full: 37.0, half: 16.0 },
    Socket { size: "3/4", outside: 32.5, full: 39.0, half: 19.0 },
    Socket { size: "1", outside: 39.5, full: 46.0, half: 21.0 },
    Socket { size: "1 1/4", outside: 49.0, full: 51.0, half: 25.0 },
    Socket { size: "1 1/2", outside: 56.0, full: 51.0, half: 25.0 },
    Socket { size: "2", outside: 68.0, full: 60.0, half: 28.0 },
    Socket { size: "2 1/2", outside: 84.0, full: 69.0, half: 30.0 },
    Socket { size: "3", outside: 98.0, full: 75.0, half: 35.0 },
    Socket { size: "4", outside: 124.0, full: 87.0, half: 40.0 },
    Socket { size: "5", outside: 151.0, full: 96.0, half: 44.0 },
    Socket { size: "6", outside: 177.0, full: 96.0, half: 44.0 },
];

/// A thread size as written (`1/2`, `R1/2`, `Rp 1 1/2`, `G1-1/2`) in the tables' form, `1 1/2`.
pub fn thread_size(text: &str) -> Option<&'static str> {
    let t = text.trim();
    let t = t.strip_prefix("Rp").or_else(|| t.strip_prefix('R')).or_else(|| t.strip_prefix('G')).unwrap_or(t);
    let t = t.trim().replace('-', " ").replace('"', "");
    let t = t.split_whitespace().collect::<Vec<_>>().join(" ");
    PIPES.iter().map(|p| p.size).chain(SOCKETS.iter().map(|s| s.size)).find(|s| *s == t)
}

/// A pipe thread's designation for a table size: `R` (taper outside), `Rp` (parallel inside),
/// as `R1-1/4`.
pub fn thread_name(prefix: &str, size: &str) -> String {
    format!("{prefix}{}", size.replace(' ', "-"))
}

/// A medium pipe's sizes for a thread size.
pub fn pipe(size: &str) -> Option<Pipe> {
    let s = thread_size(size)?;
    PIPES.iter().find(|p| p.size == s).copied()
}

/// A socket's sizes for a thread size.
pub fn socket(size: &str) -> Option<Socket> {
    let s = thread_size(size)?;
    SOCKETS.iter().find(|p| p.size == s).copied()
}

/// The thread sizes the threaded ends and the sockets come in.
pub fn pipe_sizes() -> Vec<&'static str> {
    PIPES.iter().map(|p| p.size).collect()
}

pub fn socket_sizes() -> Vec<&'static str> {
    SOCKETS.iter().map(|p| p.size).collect()
}

/// A standard fitting's size as the design keeps it, read: a flange's type, class, DN and
/// standard (`11 PN16 DN50`, `02 PN10 DN100 DIN`), a collar's class and DN (`PN16 DN50`), a
/// socket's or a threaded end's thread size and form (`1/2 full`, `3/4 half`, `1`).
#[derive(Clone, Debug, PartialEq)]
pub struct Designation {
    pub kind: String,
    pub pn: Option<u16>,
    pub dn: Option<u16>,
    pub thread: Option<&'static str>,
    pub standard: Standard,
}

pub fn designation(text: &str) -> Designation {
    let mut d = Designation { kind: String::new(), pn: None, dn: None, thread: None, standard: Standard::En };
    let mut rest = Vec::new();
    for w in text.split_whitespace() {
        let up = w.to_ascii_uppercase();
        if let Some(n) = up.strip_prefix("PN").and_then(|n| n.parse().ok()) {
            d.pn = Some(n);
        } else if let Some(n) = up.strip_prefix("DN").and_then(|n| n.parse().ok()) {
            d.dn = Some(n);
        } else if up == "DIN" {
            d.standard = Standard::Din;
        } else if matches!(up.as_str(), "EN" | "ISO") {
        } else if matches!(w, "01" | "02" | "11" | "32" | "33" | "full" | "half") {
            d.kind = w.to_string();
        } else {
            rest.push(w);
        }
    }
    d.thread = thread_size(&rest.join(" "));
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every weld neck flange holds together: the neck inside the hub inside the face inside
    /// the holes inside the rim, the plate thinner than the flange is long.
    #[test]
    fn every_weld_neck_is_a_sound_flange() {
        for pn in flange_classes("11") {
            let dns = flange_sizes("11", *pn);
            assert!(dns.len() >= 18, "PN {pn}: {dns:?}");
            for dn in dns {
                let Some(s) = weld_neck(*pn, dn, Standard::En) else { panic!("PN {pn} DN {dn}") };
                assert!(s.neck - 2.0 * s.wall > 0.0 && s.neck < s.hub && s.hub < s.face, "PN {pn} DN {dn}: {s:?}");
                assert!(s.face < s.pitch - s.hole && s.pitch + s.hole < s.outside, "PN {pn} DN {dn}: {s:?}");
                assert!(s.face_height < s.thickness && s.thickness + s.straight < s.length, "PN {pn} DN {dn}: {s:?}");
                let gap = s.pitch * (std::f64::consts::PI / f64::from(s.holes)).sin() - s.hole;
                assert!(gap > 3.0, "PN {pn} DN {dn}: {s:?}");
            }
        }
    }

    /// Every loose flange slides over its collar's pipe, its bore inside its holes, its collar's
    /// face inside its bolt circle.
    #[test]
    fn every_loose_flange_fits_its_collar() {
        for pn in flange_classes("02") {
            for dn in flange_sizes("02", *pn) {
                let Some(s) = loose(*pn, dn, Standard::En) else { panic!("PN {pn} DN {dn}") };
                assert!(s.pipe < s.bore && s.bore < s.collar && s.collar < s.pitch - s.hole / 2.0, "PN {pn} DN {dn}: {s:?}");
                assert!(s.pitch + s.hole < s.outside && s.collar_thickness > 0.0, "PN {pn} DN {dn}: {s:?}");
            }
        }
    }

    /// The sizes as the tables print them; DIN DN 65 has 4 holes; the threaded parts' threads.
    #[test]
    fn known_sizes_and_designations() {
        let w = weld_neck(16, 50, Standard::En).map(|s| (s.outside, s.pitch, s.holes, s.thickness, s.length));
        assert_eq!(w, Some((165.0, 125.0, 4, 18.0, 45.0)));
        assert_eq!(weld_neck(40, 100, Standard::En).map(|s| s.outside), Some(235.0));
        assert_eq!(weld_neck(16, 65, Standard::Din).map(|s| s.holes), Some(4));
        assert_eq!(loose(16, 200, Standard::En).map(|s| (s.holes, s.bore, s.thickness)), Some((12, 226.0, 26.0)));
        assert_eq!(loose(16, 25, Standard::En), loose(10, 25, Standard::En));
        assert_eq!(pipe("R1/2").map(|p| (p.outside, p.wall)), Some((21.3, 2.65)));
        assert_eq!(socket("Rp 1 1/2").map(|s| s.outside), Some(56.0));
        assert_eq!(socket("1-1/2"), socket("1 1/2"));
        let d = designation("11 PN16 DN50 DIN");
        assert_eq!((d.kind.as_str(), d.pn, d.dn, d.standard), ("11", Some(16), Some(50), Standard::Din));
        let d = designation("1 1/4 half");
        assert_eq!((d.kind.as_str(), d.thread), ("half", Some("1 1/4")));
        // every thread size is one the thread maker knows, as R and Rp
        for s in pipe_sizes().into_iter().chain(socket_sizes()) {
            assert!(crate::thread_spec::parse(&thread_name("R", s)).is_some(), "R{s}");
            assert!(crate::thread_spec::parse(&thread_name("Rp", s)).is_some(), "Rp{s}");
        }
        for s in socket_sizes() {
            let (t, k) = (crate::thread_spec::parse(&thread_name("Rp", s)).map_or(0.0, |t| t.major), socket(s).map_or(0.0, |k| k.outside));
            assert!(k > t + 2.0, "a {s} socket's wall: {k} over {t}");
        }
        for s in pipe_sizes() {
            let (t, p) = (crate::thread_spec::parse(&thread_name("R", s)).map_or(0.0, |t| t.major), pipe(s).map_or(0.0, |p| p.outside));
            assert!(p >= t && p - t < 2.0, "a {s} pipe takes its thread: {p} vs {t}");
        }
    }
}
