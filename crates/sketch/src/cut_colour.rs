//! The colours a cutting drawing gives its contours: LightBurn's layer palette (00 to 29), so a
//! DXF opened there lands every contour on the layer of its own colour. A DXF holds each one as
//! its exact colour (true colour) and as the nearest AutoCAD colour index for older readers.

/// The palette: its colours in LightBurn's layer order, by an English name (the first seventeen)
/// or their number.
pub const PALETTE: &[(&str, [u8; 3])] = &[
    ("black", [0, 0, 0]),
    ("blue", [0, 0, 255]),
    ("red", [255, 0, 0]),
    ("green", [0, 224, 0]),
    ("yellow", [208, 208, 0]),
    ("orange", [255, 128, 0]),
    ("cyan", [0, 224, 224]),
    ("magenta", [255, 0, 255]),
    ("light_gray", [180, 180, 180]),
    ("dark_blue", [0, 0, 160]),
    ("dark_red", [160, 0, 0]),
    ("dark_green", [0, 160, 0]),
    ("olive", [160, 160, 0]),
    ("brown", [192, 128, 0]),
    ("sky_blue", [0, 160, 255]),
    ("purple", [160, 0, 160]),
    ("gray", [128, 128, 128]),
    ("c17", [125, 135, 185]),
    ("c18", [187, 119, 132]),
    ("c19", [74, 111, 227]),
    ("c20", [211, 63, 106]),
    ("c21", [140, 215, 140]),
    ("c22", [240, 185, 141]),
    ("c23", [246, 196, 225]),
    ("c24", [250, 158, 212]),
    ("c25", [80, 10, 120]),
    ("c26", [180, 90, 0]),
    ("c27", [0, 71, 84]),
    ("c28", [134, 250, 136]),
    ("c29", [255, 219, 102]),
];

/// The colours a cutting drawing uses when none is chosen: part edges red, holes blue, open
/// curves yellow.
pub const OUTER: u8 = 2;
pub const INNER: u8 = 1;
pub const OPEN: u8 = 4;

/// A colour's RGB, by its palette index.
pub fn rgb(index: u8) -> Option<[u8; 3]> {
    PALETTE.get(usize::from(index)).map(|(_, c)| *c)
}

/// A colour's name, by its palette index.
pub fn name(index: u8) -> Option<&'static str> {
    PALETTE.get(usize::from(index)).map(|(n, _)| *n)
}

/// A colour by name, or by its number ("05", "5"), as a palette index.
pub fn parse(text: &str) -> Option<u8> {
    let t = text.trim().to_ascii_lowercase().replace([' ', '-'], "_");
    if let Some(i) = PALETTE.iter().position(|(n, _)| *n == t) {
        return u8::try_from(i).ok();
    }
    t.trim_start_matches('c').parse::<u8>().ok().filter(|i| usize::from(*i) < PALETTE.len())
}

/// The colour as a DXF true colour (group 420): 0xRRGGBB.
pub fn true_colour(index: u8) -> Option<u32> {
    rgb(index).map(|[r, g, b]| (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b))
}

/// AutoCAD's colour of an index (1 to 255): the nine basic ones, the 240 of the hue wheel (24
/// hues in 15° steps, five brightnesses, full and half saturation) and six greys.
pub fn aci_rgb(aci: u8) -> [u8; 3] {
    match aci {
        1 => [255, 0, 0],
        2 => [255, 255, 0],
        3 => [0, 255, 0],
        4 => [0, 255, 255],
        5 => [0, 0, 255],
        6 => [255, 0, 255],
        7 => [255, 255, 255],
        8 => [128, 128, 128],
        9 => [192, 192, 192],
        10..=249 => {
            let hue = f64::from(aci / 10 - 1) * 15.0;
            let k = aci % 10;
            let value = [1.0, 0.8, 0.6, 0.5, 0.3][usize::from(k / 2)];
            let pale = k % 2 == 1;
            let h = hue / 60.0;
            let x = 1.0 - ((h % 2.0) - 1.0).abs();
            let (r, g, b) = match h as u32 {
                0 => (1.0, x, 0.0),
                1 => (x, 1.0, 0.0),
                2 => (0.0, 1.0, x),
                3 => (0.0, x, 1.0),
                4 => (x, 0.0, 1.0),
                _ => (1.0, 0.0, x),
            };
            let ch = |c: f64| {
                let c = if pale { c + (1.0 - c) * 0.5 } else { c };
                (c * value * 255.0).round().clamp(0.0, 255.0) as u8
            };
            [ch(r), ch(g), ch(b)]
        }
        250 => [51, 51, 51],
        251 => [80, 80, 80],
        252 => [105, 105, 105],
        253 => [130, 130, 130],
        254 => [190, 190, 190],
        _ => [255, 255, 255],
    }
}

fn dist2(a: [u8; 3], b: [u8; 3]) -> i32 {
    a.iter().zip(b).map(|(x, y)| (i32::from(*x) - i32::from(y)).pow(2)).sum()
}

/// The AutoCAD colour index nearest a palette colour, for readers without true colour: black is
/// 7 (drawn black on a white page), the rest the nearest of the others.
pub fn aci(index: u8) -> i16 {
    let Some(c) = rgb(index) else { return 7 };
    if c == [0, 0, 0] {
        return 7;
    }
    (1..=255u8).filter(|a| *a != 7).min_by_key(|a| dist2(aci_rgb(*a), c)).map_or(7, i16::from)
}

/// The palette colour nearest an RGB colour (how LightBurn reads a colour it doesn't know).
pub fn nearest(c: [u8; 3]) -> u8 {
    PALETTE.iter().enumerate().min_by_key(|(_, (_, p))| dist2(*p, c)).and_then(|(i, _)| u8::try_from(i).ok()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_numbers_read_back() {
        assert_eq!(PALETTE.len(), 30, "LightBurn has 30 layer colours");
        for i in 0..30u8 {
            assert_eq!(parse(name(i).unwrap_or_default()), Some(i));
            assert_eq!(parse(&format!("{i:02}")), Some(i));
        }
        assert_eq!(parse("Light Gray"), Some(8));
        assert_eq!(parse("30"), None);
        assert_eq!(parse("pink"), None);
        assert_eq!(true_colour(3), Some(0x00E000));
    }

    #[test]
    fn the_autocad_wheel_is_autocad_s() {
        // AutoCAD's colour table, to a unit (it rounds 127.5 down).
        assert_eq!(aci_rgb(10), [255, 0, 0]);
        assert_eq!(aci_rgb(11), [255, 128, 128]);
        assert_eq!(aci_rgb(12), [204, 0, 0]);
        assert_eq!(aci_rgb(30), [255, 128, 0]);
        assert_eq!(aci_rgb(40), [255, 191, 0]);
        assert_eq!(aci_rgb(150), [0, 128, 255]);
    }

    /// A reader that only knows the colour index still lands the basic colours on their own
    /// LightBurn layer.
    #[test]
    fn the_index_of_each_basic_colour_reads_back_as_it() {
        for i in 1..17u8 {
            assert_eq!(nearest(aci_rgb(aci(i).clamp(1, 255) as u8)), i, "{:?} → ACI {}", name(i), aci(i));
        }
        assert_eq!(aci(0), 7);
    }
}
