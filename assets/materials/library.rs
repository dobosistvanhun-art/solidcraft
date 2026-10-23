//! Built-in physical materials: densities in g/cm³ and sRGB colours.
pub const DEFAULT_DENSITY: f64 = 1.29;

/// Physical materials: name and density (g/cm³).
/// The generic ones first (older designs name them), then grades by their standard name.
pub const MATERIALS: [(&str, f64); 31] = [
    ("Default", DEFAULT_DENSITY),
    ("Steel", 7.85),
    ("Stainless Steel", 8.0),
    ("Aluminum", 2.70),
    ("Brass", 8.50),
    ("Copper", 8.96),
    ("Titanium", 4.43),
    ("Cast Iron", 7.20),
    ("ABS Plastic", 1.06),
    ("PLA", 1.24),
    ("Nylon", 1.14),
    ("Polycarbonate", 1.20),
    ("Oak", 0.75),
    ("Glass", 2.50),
    ("Steel S235JR", 7.85),
    ("Steel S355J2", 7.85),
    ("Steel C45", 7.85),
    ("Steel DC01", 7.85),
    ("Galvanised Steel DX51D", 7.85),
    ("Tool Steel 1.2842", 7.85),
    ("Stainless 1.4301 (304)", 7.93),
    ("Stainless 1.4404 (316L)", 8.0),
    ("Stainless 1.4016 (430)", 7.7),
    ("Aluminium Al99.5 (1050)", 2.705),
    ("Aluminium AlMg3 (5754)", 2.67),
    ("Aluminium AlMg4.5Mn (5083)", 2.66),
    ("Aluminium AlMgSi1 (6082)", 2.70),
    ("Copper Cu-ETP", 8.93),
    ("Brass CuZn37", 8.44),
    ("Bronze CuSn8", 8.80),
    ("Bronze CuSn12", 8.70),
];

/// How a physical material looks (sRGB); the default material has no colour of its own.
pub fn material_color(name: &str) -> Option<[u8; 3]> {
    Some(match name {
        "Steel" => [158, 164, 172],
        "Stainless Steel" => [186, 191, 198],
        "Aluminum" => [204, 208, 214],
        "Brass" => [208, 172, 88],
        "Copper" => [204, 122, 80],
        "Titanium" => [150, 148, 158],
        "Cast Iron" => [104, 104, 110],
        "ABS Plastic" => [222, 222, 216],
        "PLA" => [110, 160, 222],
        "Nylon" => [236, 230, 214],
        "Polycarbonate" => [196, 216, 232],
        "Oak" => [190, 146, 92],
        "Glass" => [170, 208, 218],
        // The grades take their family's colour.
        n if n.starts_with("Steel ") || n.starts_with("Galvanised Steel") || n.starts_with("Tool Steel") => return material_color("Steel"),
        n if n.starts_with("Stainless ") => return material_color("Stainless Steel"),
        n if n.starts_with("Aluminium ") => return material_color("Aluminum"),
        n if n.starts_with("Copper ") => return material_color("Copper"),
        n if n.starts_with("Brass ") => return material_color("Brass"),
        n if n.starts_with("Bronze ") => [184, 132, 72],
        _ => return None,
    })
}
