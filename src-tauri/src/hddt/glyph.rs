#![allow(dead_code)]

// ─── Captcha glyph rasterizer (SVG path → 48×48 bitmap) ───
//
// Straight 1-1 port of `tools/hddt-captcha/glyph_algo.py` (the verified source of
// truth):
//   - The portal's captcha paths only use M / L / Q / Z (already flattened
//     server-side); default fill-rule is nonzero → scanline: collect crossings,
//     sort, then fill alternating spans.
//   - Normalization: bbox → scale longest side to 40 → pad 1 → render → center
//     in a 48×48 frame.
//
// `templates.bin` is exported with THIS exact algorithm, so query bitmaps and
// reference bitmaps share the same rendering path → exact match. The `parity`
// unit test compares each bitmap against the Python-generated fixture
// (`parity.txt`) to guarantee the two implementations never diverge.

/// 48×48 pixels = 2304 bits = 288 bytes, row-major, MSB-first.
pub(crate) const MASK_BYTES: usize = 288;
pub(crate) type Mask = [u8; MASK_BYTES];

#[derive(Clone, Copy)]
enum Tok {
    Cmd(u8),
    Num(f64),
}

/// Tokenize into M/L/Q/Z commands plus numbers. Any other character (including
/// letters outside the command set) is skipped — matching the Python
/// `re.findall` behavior.
fn tokenize(s: &str) -> Vec<Tok> {
    let b = s.as_bytes();
    let mut i = 0;
    let mut out = Vec::new();
    while i < b.len() {
        let c = b[i];
        if matches!(c, b'M' | b'L' | b'Q' | b'Z') {
            out.push(Tok::Cmd(c));
            i += 1;
        } else if c.is_ascii_digit() || c == b'.' || c == b'-' || c == b'+' {
            let start = i;
            i += 1;
            while i < b.len() && (b[i].is_ascii_digit() || b[i] == b'.') {
                i += 1;
            }
            if let Ok(v) = s[start..i].parse::<f64>() {
                out.push(Tok::Num(v));
            }
        } else {
            i += 1;
        }
    }
    out
}

/// Flat coordinate list (x0,y0,x1,y1,…) — used for bbox / min-x.
fn flat_numbers(s: &str) -> Vec<f64> {
    tokenize(s)
        .into_iter()
        .filter_map(|t| match t {
            Tok::Num(v) => Some(v),
            Tok::Cmd(_) => None,
        })
        .collect()
}

/// Horizontal position of the glyph inside the captcha (for left → right order).
pub(crate) fn glyph_min_x(d: &str) -> f64 {
    let n = flat_numbers(d);
    n.iter().step_by(2).copied().fold(f64::INFINITY, f64::min)
}

/// Path → contours (polygons with Q flattened, n=10). `Z` closes back to start.
fn parse_contours(d: &str) -> Vec<Vec<(f64, f64)>> {
    let toks = tokenize(d);
    let mut contours: Vec<Vec<(f64, f64)>> = Vec::new();
    let mut start = (0.0f64, 0.0f64);
    let mut i = 0usize;

    macro_rules! num {
        () => {{
            let v = match toks.get(i) {
                Some(Tok::Num(v)) => *v,
                _ => return contours,
            };
            i += 1;
            v
        }};
    }

    while i < toks.len() {
        let c = match toks[i] {
            Tok::Cmd(c) => c,
            Tok::Num(_) => break,
        };
        i += 1;
        match c {
            b'M' => {
                let x = num!();
                let y = num!();
                start = (x, y);
                contours.push(vec![(x, y)]);
            }
            b'L' => {
                while i + 1 < toks.len() && !matches!(toks[i], Tok::Cmd(_)) {
                    let x = num!();
                    let y = num!();
                    if let Some(cur) = contours.last_mut() {
                        cur.push((x, y));
                    }
                }
            }
            b'Q' => {
                while i + 3 < toks.len() && !matches!(toks[i], Tok::Cmd(_)) {
                    let cx = num!();
                    let cy = num!();
                    let x = num!();
                    let y = num!();
                    let p0 = match contours.last().and_then(|c| c.last()) {
                        Some(p) => *p,
                        None => return contours,
                    };
                    if let Some(cur) = contours.last_mut() {
                        for k in 1..=10 {
                            let u = k as f64 / 10.0;
                            let one = 1.0 - u;
                            cur.push((
                                one * one * p0.0 + 2.0 * one * u * cx + u * u * x,
                                one * one * p0.1 + 2.0 * one * u * cy + u * u * y,
                            ));
                        }
                    }
                }
            }
            _ => {
                // b'Z'
                if let Some(cur) = contours.last_mut() {
                    if cur.len() > 1 {
                        cur.push(start);
                    }
                }
            }
        }
    }
    contours.retain(|c| c.len() > 1);
    contours
}

/// Nonzero scanline fill: per row, collect crossing x positions, sort, then fill
/// alternating spans.
fn rasterize(contours: &[Vec<(f64, f64)>], gw: usize, gh: usize) -> Vec<u8> {
    let mut acc = vec![0u8; gw * gh];
    let mut edges: Vec<(f64, f64, f64, f64)> = Vec::new();
    for c in contours {
        for w in c.windows(2) {
            let (x0, y0) = w[0];
            let (x1, y1) = w[1];
            if (y1 - y0).abs() > 1e-12 {
                edges.push((x0, y0, x1, y1));
            }
        }
    }
    for y in 0..gh {
        let yc = y as f64 + 0.5;
        let mut xs: Vec<f64> = Vec::new();
        for &(x0, y0, x1, y1) in &edges {
            let (ylo, yhi) = if y0 < y1 { (y0, y1) } else { (y1, y0) };
            if ylo <= yc && yc < yhi {
                xs.push(x0 + (yc - y0) / (y1 - y0) * (x1 - x0));
            }
        }
        if xs.len() < 2 {
            continue;
        }
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let mut k = 0;
        while k + 1 < xs.len() {
            let xa = (xs[k] - 0.5).ceil() as i64;
            let xb = (xs[k + 1] - 0.5).floor() as i64;
            if xb >= xa {
                let a = xa.max(0);
                let b = xb.min(gw as i64 - 1);
                if b >= a {
                    for x in a..=b {
                        acc[y * gw + x as usize] = 1;
                    }
                }
            }
            k += 2;
        }
    }
    acc
}

/// Normalize + render one glyph path into a 48×48 bitmap (matches Python
/// `render_glyph`).
pub(crate) fn render_glyph(d: &str) -> Mask {
    let nums = flat_numbers(d);
    if nums.len() < 2 {
        return [0u8; MASK_BYTES];
    }
    let xs: Vec<f64> = nums.iter().step_by(2).copied().collect();
    let ys: Vec<f64> = nums.iter().skip(1).step_by(2).copied().collect();
    let (x0b, x1) = (
        xs.iter().copied().fold(f64::INFINITY, f64::min),
        xs.iter().copied().fold(f64::NEG_INFINITY, f64::max),
    );
    let (y0b, y1) = (
        ys.iter().copied().fold(f64::INFINITY, f64::min),
        ys.iter().copied().fold(f64::NEG_INFINITY, f64::max),
    );
    let (w, h) = (x1 - x0b, y1 - y0b);
    let s = 40.0 / w.max(h);
    let wpx = ((w * s) as i64).max(2) as usize + 2;
    let hpx = ((h * s) as i64).max(2) as usize + 2;
    let sx = wpx as f64 / (w + 2.0);
    let sy = hpx as f64 / (h + 2.0);

    let contours: Vec<Vec<(f64, f64)>> = parse_contours(d)
        .into_iter()
        .map(|c| {
            c.into_iter()
                .map(|(x, y)| ((x - x0b + 1.0) * sx, (y - y0b + 1.0) * sy))
                .collect()
        })
        .collect();
    let mask = rasterize(&contours, wpx, hpx);

    let mut out = [0u8; MASK_BYTES];
    if wpx <= 48 && hpx <= 48 {
        let yy = (48 - hpx) / 2;
        let xx = (48 - wpx) / 2;
        for r in 0..hpx {
            for c in 0..wpx {
                if mask[r * wpx + c] != 0 {
                    let n = (yy + r) * 48 + (xx + c);
                    out[n / 8] |= 0x80 >> (n % 8);
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn to_hex(m: &Mask) -> String {
        m.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// Compare every bitmap against the Python-generated fixture to guarantee
    /// the port stays in sync.
    #[test]
    fn parity_with_python_reference() {
        let fixture = include_str!("parity.txt");
        let mut n = 0;
        for line in fixture.lines().filter(|l| !l.trim().is_empty()) {
            let mut parts = line.split('\t');
            let ch = parts.next().unwrap();
            let path = parts.next().unwrap();
            let expect = parts.next().unwrap();
            let got = to_hex(&render_glyph(path));
            assert_eq!(
                got, expect,
                "glyph '{ch}' rendered differently from Python (port diverged)"
            );
            n += 1;
        }
        assert_eq!(
            n,
            crate::hddt::CHARSET.chars().count(),
            "fixture phải phủ đủ charset"
        );
    }

    #[test]
    fn render_is_not_empty_and_fits() {
        let fixture = include_str!("parity.txt");
        let path = fixture.lines().next().unwrap().split('\t').nth(1).unwrap();
        let m = render_glyph(path);
        let filled: u32 = m.iter().map(|b| b.count_ones()).sum();
        assert!(
            (50..2000).contains(&filled),
            "unexpected number of filled pixels: {filled}"
        );
    }

    #[test]
    fn min_x_orders_glyphs() {
        let a = "M10 5L20 25L12 25Z";
        let b = "M40 5L50 25L42 25Z";
        assert!(glyph_min_x(a) < glyph_min_x(b));
    }
}
