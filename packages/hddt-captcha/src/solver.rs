#![allow(dead_code)]

// ─── HDDT captcha solver via glyph template matching ───
//
// Verified live (09/2026): query bitmaps and reference bitmaps are both produced
// by `glyph::render_glyph` (templates exported with that exact algorithm — see
// `tools/hddt-captcha/export_templates.py`), so they match exactly.
//
// Reference data `templates.bin`: repeated [label 1 byte][bitmap 288 bytes].
// 30 characters `2-9A-Z` trừ `0 1 I L O U` (the confusable ones); `P` có thật,
// xác nhận bằng các mẫu cổng chấp nhận. Measured: leave-one-out 756/756.
// Mở rộng bộ mẫu: `cargo test --lib live_harvest_captcha_templates -- --ignored`

use crate::captcha::{Captcha, CaptchaSolver};
use crate::glyph::{glyph_min_x, render_glyph, Mask, MASK_BYTES};
use regex::Regex;
use std::sync::OnceLock;

/// The captcha's actual character set (30 characters). `P` chỉ xuất hiện rất hiếm
/// (1/222 mẫu đầu tiên) nhưng có thật — 7 mẫu chứa P đều được cổng chấp nhận
/// khi đăng nhập (25/09/2026), nên không được loại khỏi charset.
pub const CHARSET: &str = "23456789ABCDEFGHJKMNPQRSTVWXYZ";

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
pub fn classify_svg(svg: &str) -> Result<String, String> {
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

/// Đo từng glyph: (ký tự dự đoán, IoU tối nhất với template khớp nhất). Dùng khi
/// thu thập mẫu mới để biết glyph nào solver chưa vững — ứng viên cần thêm template.
pub fn classify_glyphs_detailed(svg: &str) -> Vec<(char, f32)> {
    let tpl = templates();
    extract_glyphs(svg)
        .iter()
        .map(|d| {
            let m = render_glyph(d);
            let mut best_score = 0.0f32;
            let mut best = b'?';
            for (label, tm) in tpl {
                let s = iou(&m, tm);
                if s > best_score {
                    best_score = s;
                    best = *label;
                }
            }
            (best as char, best_score)
        })
        .collect()
}

/// Các path glyph của captcha, đã loại nhiễu và sắp theo trục x (trái → phải).
/// Dùng để render lại glyph khi gán nhãn cho mẫu mới.
pub fn glyph_paths(svg: &str) -> Vec<String> {
    extract_glyphs(svg)
}

/// Đọc `templates.bin` (bytes thô) → danh sách (label, bitmap).
pub fn parse_templates(bytes: &[u8]) -> Vec<(u8, Mask)> {
    let mut v = Vec::new();
    let mut i = 0;
    while i + ENTRY_BYTES <= bytes.len() {
        let mut m = [0u8; MASK_BYTES];
        m.copy_from_slice(&bytes[i + 1..i + 1 + MASK_BYTES]);
        v.push((bytes[i], m));
        i += ENTRY_BYTES;
    }
    v
}

/// Nối các entry `(label, bitmap)` thành định dạng `templates.bin` và bỏ trùng
/// (trùng mask thì bỏ, kể cả khác nhãn — bitmap đã có thì không thêm lại).
pub fn build_templates(entries: &[(u8, Mask)]) -> Vec<u8> {
    let mut out: Vec<u8> = TEMPLATE_BYTES.to_vec();
    let mut seen: Vec<Mask> = parse_templates(TEMPLATE_BYTES)
        .into_iter()
        .map(|(_, m)| m)
        .collect();
    for (label, mask) in entries {
        if seen.iter().any(|m| m == mask) {
            continue;
        }
        out.push(*label);
        out.extend_from_slice(mask);
        seen.push(*mask);
    }
    out
}

/// Leave-one-out: bỏ từng entry rồi xem các entry còn lại có đoán đúng label không.
/// Báo cáo độ chính xác để quyết định có dùng bộ template mới hay không.
pub fn leave_one_out_accuracy(entries: &[(u8, Mask)]) -> (usize, usize) {
    let mut ok = 0;
    let mut total = 0;
    for (i, (label, mask)) in entries.iter().enumerate() {
        let rest: Vec<(u8, Mask)> = entries
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != i)
            .map(|(_, e)| *e)
            .collect();
        let mut best_score = 0.0f32;
        let mut best = b'?';
        for (l, tm) in &rest {
            let s = iou(mask, tm);
            if s > best_score {
                best_score = s;
                best = *l;
            }
        }
        total += 1;
        if best == *label {
            ok += 1;
        }
    }
    (ok, total)
}

/// Production solver: offline template matching, no external binary required.
pub struct GlyphTemplateSolver;

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
        assert!(
            t.len() >= 222,
            "phải giữ ít nhất số template gốc, thấy {}",
            t.len()
        );
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

    /// Bộ template phải nhận ra được `P` — ký tự hiếm nhưng có thật (các mẫu
    /// chứa P đều được cổng chấp nhận khi đăng nhập).
    #[test]
    fn charset_includes_rare_p() {
        assert!(CHARSET.contains('P'));
        assert!(templates().iter().any(|(label, _)| *label == b'P'));
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
        assert_eq!(n, CHARSET.chars().count(), "fixture phải phủ đủ charset");
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
