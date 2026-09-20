#![allow(dead_code)]

// ─── HDDT captcha solver via glyph template matching ───
//
// Verified live (09/2026): query bitmaps and reference bitmaps are both produced
// by `glyph::render_glyph` (templates exported with that exact algorithm — see
// `tools/hddt-captcha/export_templates.py`), so they match exactly.
//
// Reference data `templates.bin`: repeated [label 1 byte][bitmap 288 bytes],
// currently 222 glyphs across 29 characters `2-9A-Z` excluding the ambiguous
// ones (`0 1 I L O P U`). Measured: leave-one-out 222/222 and 8/8 real logins.

use crate::hddt::captcha::{Captcha, CaptchaSolver};
use crate::hddt::glyph::{glyph_min_x, render_glyph, Mask, MASK_BYTES};
use regex::Regex;
use std::sync::OnceLock;

/// The captcha's actual character set (29 characters).
pub(crate) const CHARSET: &str = "23456789ABCDEFGHJKMNQRSTVWXYZ";

const TEMPLATE_BYTES: &[u8] = include_bytes!("templates.bin");
const ENTRY_BYTES: usize = 1 + MASK_BYTES;

/// Decode `templates.bin` once → list of (label, bitmap).
fn templates() -> &'static [(u8, Mask)] {
    static T: OnceLock<Vec<(u8, Mask)>> = OnceLock::new();
    T.get_or_init(|| {
        let mut v = Vec::new();
        let mut i = 0;
        while i + ENTRY_BYTES <= TEMPLATE_BYTES.len() {
            let mut m = [0u8; MASK_BYTES];
            m.copy_from_slice(&TEMPLATE_BYTES[i + 1..i + 1 + MASK_BYTES]);
            v.push((TEMPLATE_BYTES[i], m));
            i += ENTRY_BYTES;
        }
        v
    })
}

/// Extract the glyphs (color-filled paths) from the captcha SVG, sorted left → right.
fn extract_glyphs(svg: &str) -> Vec<String> {
    static TAG_RE: OnceLock<Regex> = OnceLock::new();
    static D_RE: OnceLock<Regex> = OnceLock::new();
    let tag_re = TAG_RE.get_or_init(|| Regex::new(r"<path[^>]*>").unwrap());
    let d_re = D_RE.get_or_init(|| Regex::new(r#"d="([^"]*)""#).unwrap());

    let mut found: Vec<(f64, String)> = Vec::new();
    for tag in tag_re.find_iter(svg).map(|m| m.as_str()) {
        // Drop noise paths: `stroke="#..." fill="none"` has no `fill="#..."`.
        if !tag.contains("fill=\"#") {
            continue;
        }
        if let Some(c) = d_re.captures(tag) {
            let d = c[1].to_string();
            found.push((glyph_min_x(&d), d));
        }
    }
    found.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    found.into_iter().map(|(_, d)| d).collect()
}

/// IoU between two 48×48 bitmaps.
fn iou(a: &Mask, b: &Mask) -> f32 {
    let mut inter = 0u32;
    let mut union = 0u32;
    for i in 0..MASK_BYTES {
        inter += (a[i] & b[i]).count_ones();
        union += (a[i] | b[i]).count_ones();
    }
    if union == 0 {
        0.0
    } else {
        inter as f32 / union as f32
    }
}

/// Solve a captcha SVG → character string (left → right). Errors when the SVG
/// has no glyph.
pub(crate) fn classify_svg(svg: &str) -> Result<String, String> {
    let glyphs = extract_glyphs(svg);
    if glyphs.is_empty() {
        return Err("Captcha HĐĐT không có glyph nào để giải.".into());
    }
    let tpl = templates();
    let mut out = String::with_capacity(glyphs.len());
    for d in &glyphs {
        let m = render_glyph(d);
        let mut best_score = f32::MIN;
        let mut best = b'?';
        for (label, tm) in tpl {
            let s = iou(&m, tm);
            if s > best_score {
                best_score = s;
                best = *label;
            }
        }
        out.push(best as char);
    }
    Ok(out)
}

/// Production solver: offline template matching, no external binary required.
pub(crate) struct GlyphTemplateSolver;

impl CaptchaSolver for GlyphTemplateSolver {
    fn solve(&self, captcha: &Captcha) -> Result<String, String> {
        classify_svg(&captcha.content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates_cover_full_charset() {
        let t = templates();
        assert_eq!(t.len(), 222, "template count must match the exported data");
        let mut seen = String::new();
        for (label, _) in t {
            let ch = *label as char;
            assert!(
                CHARSET.contains(ch),
                "unexpected label outside charset: {ch:?}"
            );
            if !seen.contains(ch) {
                seen.push(ch);
            }
        }
        assert_eq!(
            seen.len(),
            CHARSET.len(),
            "every charset character must have a template"
        );
    }

    #[test]
    fn iou_self_is_one() {
        let t = templates();
        assert!((iou(&t[0].1, &t[0].1) - 1.0).abs() < 1e-6);
    }

    /// Each fixture glyph must be classified correctly through the full pipeline.
    #[test]
    fn classify_fixture_glyphs() {
        let fixture = include_str!("parity.txt");
        let mut n = 0;
        for line in fixture.lines().filter(|l| !l.trim().is_empty()) {
            let mut parts = line.split('\t');
            let ch = parts.next().unwrap();
            let path = parts.next().unwrap();
            let svg = format!(
                "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"200\" height=\"40\">\
                 <path d=\"M4 16 C90 39,120 20,189 2\" stroke=\"#888\" fill=\"none\"/>\
                 <path fill=\"#333\" d=\"{path}\"/>\
                 <path d=\"M6 3 C92 11,93 7,183 11\" stroke=\"#777\" fill=\"none\"/></svg>"
            );
            let got = classify_svg(&svg).unwrap();
            assert_eq!(got, ch, "wrong character (expected {ch}, got {got})");
            n += 1;
        }
        assert_eq!(n, 29);
    }

    #[test]
    fn extract_skips_noise_and_sorts_left_to_right() {
        let svg = "<svg>\
            <path d=\"M40 5L50 25\" stroke=\"#888\" fill=\"none\"/>\
            <path fill=\"#222\" d=\"M40 5L50 25L42 25Z\"/>\
            <path fill=\"#111\" d=\"M10 5L20 25L12 25Z\"/>\
            <path d=\"M1 1 C2 2,3 3,4 4\" fill=\"none\" stroke=\"#777\"/></svg>";
        let g = extract_glyphs(svg);
        assert_eq!(g.len(), 2, "only color-filled paths should be kept");
        assert!(g[0].starts_with("M10"), "leftmost glyph must come first");
        assert!(g[1].starts_with("M40"));
    }
}
