// ─── HDDT portal captcha + solver interface ───
//
// Research 09/2026 (decompiled the portal's Next.js bundle + verified with real
// Chrome):
//   - GET {base}/api/captcha → {"key":"<id hex>","content":"<svg 200x40>"}
//     NOTE: the frontend calls "api//captcha" (double slash) which the F5 WAF
//     blocks ("Request Rejected") — use the canonical "api/captcha" path.
//   - The SVG has 6 glyphs (fill #111/#222/#333, TrueType-style quadratic paths,
//     ~32 tall in a 40 viewBox) plus 2 wavy noise strokes (stroke #888/#777,
//     fill=none).
//   - The actual charset is 29 characters `23456789ABCDEFGHJKMNQRSTVWXYZ`
//     (excluding 0 1 I L O P U — the confusable ones). It is solvable fully
//     OFFLINE via glyph template matching built from the captcha's own font:
//     see `solver::GlyphTemplateSolver` (verified 8/8 real logins).
//     `CaptchaSolver` is the plug-in point for alternative solvers.
//
// No external process is ever spawned and no temp files are written: solving is
// pure in-process computation (`solver::GlyphTemplateSolver`).

use serde::Deserialize;

/// Captcha returned by `GET /api/captcha`.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub(crate) struct Captcha {
    /// Captcha key — sent along with the answer (field `ckey` at login).
    pub key: String,
    /// SVG content (6 glyphs + noise) — shown to the user or fed to a solver.
    pub content: String,
}

/// "Solve captcha" interface. Plug OCR/AI/paid solvers in here — the app's
/// default flow is to show the SVG and let the user type the code manually.
pub(crate) trait CaptchaSolver {
    /// Return the captcha string (usually 6 characters) or an error.
    fn solve(&self, captcha: &Captcha) -> Result<String, String>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_captcha() {
        let c: Captcha =
            serde_json::from_str(r#"{"key":"6aaec400fed74d686396617b","content":"<svg>x</svg>"}"#)
                .unwrap();
        assert_eq!(c.key, "6aaec400fed74d686396617b");
    }
}
