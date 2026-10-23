//! A vector PDF of one page (PDF 1.4, from the public specification): lines with widths and
//! dash patterns, filled polygons and single lines of text in the standard Helvetica font
//! (WinAnsi, with ő ű Ő Ű added through the font's Differences), in millimetres from the page's
//! lower left corner. Nothing is embedded, so the files are small and print at any size.

use solidcraft_geom::Vec2;

/// Where a text sits against its point.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PdfAlign {
    Left,
    Center,
    Right,
}

/// What a page holds (mm).
#[derive(Clone, Debug, PartialEq)]
pub enum PdfItem {
    /// A straight line `width` mm wide, dashed by `dash` (on/off lengths, mm) when it isn't empty.
    Line { a: Vec2, b: Vec2, width: f64, dash: Vec<f64> },
    /// A filled polygon.
    Fill { points: Vec<Vec2> },
    /// One line of text on its baseline, `height` mm tall (cap height about 0.72 of it),
    /// turned `angle_deg` counter-clockwise.
    Text { at: Vec2, height: f64, angle_deg: f64, text: String, align: PdfAlign },
}

const MM: f64 = 72.0 / 25.4;

/// A character's WinAnsi code (with ő ű Ő Ű at the codes of õ û Õ Û), or `?`.
fn win_ansi(c: char) -> u8 {
    match c {
        ' '..='~' => c as u8,
        'ő' => 0xF5,
        'ű' => 0xFB,
        'Ő' => 0xD5,
        'Ű' => 0xDB,
        '–' => 0x96,
        '—' => 0x97,
        '€' => 0x80,
        '‘' => 0x91,
        '’' => 0x92,
        '“' => 0x93,
        '”' => 0x94,
        '•' => 0x95,
        '⌀' => 0xD8,
        c if ('\u{a0}'..='\u{ff}').contains(&c) => c as u32 as u8,
        _ => b'?',
    }
}

/// A character's width in Helvetica (thousandths of the font size), near enough for placing text.
fn glyph_width(c: u8) -> f64 {
    match c {
        b' ' | b'!' | b',' | b'.' | b'/' | b':' | b';' | b'I' | b'[' | b']' | b'\\' | b'|' => 278.0,
        b'\'' | b'i' | b'j' | b'l' => 222.0,
        b'(' | b')' | b'-' | b'`' | b'f' | b't' | b'r' => 333.0,
        b'0'..=b'9' | b'$' | b'#' | b'_' => 556.0,
        b'*' | b'^' | b'"' => 389.0,
        b'+' | b'<' | b'=' | b'>' | b'~' | 0xB1 | 0xD7 => 584.0,
        b'm' => 833.0,
        b'w' => 722.0,
        b'M' => 833.0,
        b'W' => 944.0,
        b'@' => 1015.0,
        b'%' => 889.0,
        b'A'..=b'Z' | 0xC0..=0xDE => 680.0,
        b'a'..=b'z' | 0xDF..=0xFF => 550.0,
        0xB0 => 400.0,
        _ => 556.0,
    }
}

/// How wide a text is in Helvetica at `height` mm.
pub fn text_width(text: &str, height: f64) -> f64 {
    text.chars().map(|c| glyph_width(win_ansi(c))).sum::<f64>() / 1000.0 * height
}

fn n(v: f64) -> String {
    let v = if v.is_finite() { v } else { 0.0 };
    let t = format!("{:.3}", v);
    let t = t.trim_end_matches('0').trim_end_matches('.');
    if t == "-0" || t.is_empty() { "0".into() } else { t.to_string() }
}

fn escape(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() + 2);
    for &b in bytes {
        match b {
            b'(' | b')' | b'\\' => {
                s.push('\\');
                s.push(b as char);
            }
            0x20..=0x7e => s.push(b as char),
            _ => s.push_str(&format!("\\{:03o}", b)),
        }
    }
    s
}

/// The PDF of a page `width` × `height` mm holding `items`; `title` goes in its properties.
pub fn write_pdf(width: f64, height: f64, items: &[PdfItem], title: &str) -> Vec<u8> {
    let mut c = String::new();
    c.push_str(&format!("{MM:.6} 0 0 {MM:.6} 0 0 cm\n1 J 1 j 0 G 0 g\n"));
    let mut last: Option<(String, String)> = None;
    for it in items {
        match it {
            PdfItem::Line { a, b, width, dash } => {
                if !(a.is_finite() && b.is_finite()) {
                    continue;
                }
                let w = n(width.clamp(0.0, 10.0));
                let d = if dash.is_empty() {
                    "[] 0 d".to_string()
                } else {
                    format!("[{}] 0 d", dash.iter().map(|x| n(*x)).collect::<Vec<_>>().join(" "))
                };
                if last.as_ref() != Some(&(w.clone(), d.clone())) {
                    c.push_str(&format!("{w} w {d}\n"));
                    last = Some((w, d));
                }
                c.push_str(&format!("{} {} m {} {} l S\n", n(a.x), n(a.y), n(b.x), n(b.y)));
            }
            PdfItem::Fill { points } => {
                if points.len() < 3 || !points.iter().all(|p| p.is_finite()) {
                    continue;
                }
                for (i, p) in points.iter().enumerate() {
                    c.push_str(&format!("{} {} {}\n", n(p.x), n(p.y), if i == 0 { "m" } else { "l" }));
                }
                c.push_str("h f\n");
            }
            PdfItem::Text { at, height, angle_deg, text, align } => {
                if !(at.is_finite() && height.is_finite() && *height > 0.0) || text.is_empty() {
                    continue;
                }
                let bytes: Vec<u8> = text.chars().take(500).map(win_ansi).collect();
                let w = bytes.iter().map(|b| glyph_width(*b)).sum::<f64>() / 1000.0 * height;
                let shift = match align {
                    PdfAlign::Left => 0.0,
                    PdfAlign::Center => -w / 2.0,
                    PdfAlign::Right => -w,
                };
                let (s, co) = angle_deg.to_radians().sin_cos();
                let (x, y) = (at.x + co * shift, at.y + s * shift);
                // A 1 mm font scaled to the height: the matrix carries the size and the turn.
                c.push_str(&format!(
                    "BT /F1 1 Tf {} {} {} {} {} {} Tm ({}) Tj ET\n",
                    n(co * height),
                    n(s * height),
                    n(-s * height),
                    n(co * height),
                    n(x),
                    n(y),
                    escape(&bytes)
                ));
            }
        }
    }
    let (pw, ph) = (width * MM, height * MM);
    let mut objs: Vec<String> = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
        format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>", n(pw), n(ph)),
        format!("<< /Length {} >>\nstream\n{}endstream", c.len(), c),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding << /Type /Encoding /BaseEncoding /WinAnsiEncoding \
         /Differences [213 /Ohungarumlaut 219 /Uhungarumlaut 245 /ohungarumlaut 251 /uhungarumlaut] >> >>"
            .into(),
    ];
    let title_bytes: Vec<u8> = title.chars().take(200).map(win_ansi).collect();
    objs.push(format!("<< /Title ({}) /Producer (SolidCraft) >>", escape(&title_bytes)));
    let mut out: Vec<u8> = b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n".to_vec();
    let mut offsets = Vec::with_capacity(objs.len());
    for (i, o) in objs.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{}\nendobj\n", i + 1, o).as_bytes());
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offsets {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!("trailer\n<< /Size {} /Root 1 0 R /Info {} 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1, objs.len()).as_bytes(),
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_is_a_wellformed_pdf() {
        let items = vec![
            PdfItem::Line { a: Vec2::new(10.0, 10.0), b: Vec2::new(100.0, 10.0), width: 0.5, dash: vec![] },
            PdfItem::Line { a: Vec2::new(10.0, 20.0), b: Vec2::new(100.0, 20.0), width: 0.25, dash: vec![3.0, 1.5] },
            PdfItem::Fill { points: vec![Vec2::new(0.0, 0.0), Vec2::new(3.0, 0.5), Vec2::new(3.0, -0.5)] },
            PdfItem::Text { at: Vec2::new(50.0, 50.0), height: 3.5, angle_deg: 90.0, text: "Ø20 ±0,1 (ő ű)".into(), align: PdfAlign::Center },
            PdfItem::Line { a: Vec2::new(f64::NAN, 0.0), b: Vec2::new(1.0, 1.0), width: 0.5, dash: vec![] },
        ];
        let pdf = write_pdf(297.0, 210.0, &items, "Próba");
        let text = String::from_utf8_lossy(&pdf);
        assert!(text.starts_with("%PDF-1.4"));
        assert!(text.trim_end().ends_with("%%EOF"));
        assert!(text.contains("/MediaBox [0 0 841.89 595.276]"), "A4 landscape in points");
        assert!(text.contains("[3 1.5] 0 d"));
        assert!(text.contains("\\330") && text.contains("\\365") && text.contains("\\373"), "Ø ő ű in WinAnsi");
        // The cross-reference table points at each object.
        let xref = text.rfind("startxref\n").map(|i| &text[i + 10..]).and_then(|t| t.lines().next()).and_then(|l| l.parse::<usize>().ok()).unwrap();
        assert!(pdf[xref..].starts_with(b"xref"));
        let table = String::from_utf8_lossy(&pdf[xref..]).to_string();
        for i in 1..=6 {
            let line = table.lines().nth(2 + i).unwrap();
            let off: usize = line[..10].parse().unwrap();
            assert!(pdf[off..].starts_with(format!("{i} 0 obj").as_bytes()), "object {i}");
        }
        assert!(!text.contains("NaN"));
        assert!((text_width("MW", 10.0) - 17.77).abs() < 0.01);
    }
}
