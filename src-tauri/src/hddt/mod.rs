// HDDT (electronic invoice) integration with hoadondientu.gdt.gov.vn portal.
//
// Scoped around `HddtClient` (struct + impl) rather than free functions, so the
// follow-up operations (profile, catalogs, invoice lookups, …) share the same
// session/flow.
//
// Status (09/2026): the login protocol was reverse-engineered from the portal's
// Next.js bundle and verified with real Chrome + real API calls. The captcha is
// solvable fully OFFLINE via glyph template matching
// (`solver::GlyphTemplateSolver`) — measured on the live portal: 8/8 successful
// logins, leave-one-out 222/222.
pub(crate) mod captcha;
pub(crate) mod client;
pub(crate) mod glyph;
pub(crate) mod solver;

pub(crate) use captcha::{Captcha, CaptchaSolver};
pub(crate) use client::{HddtClient, LoginSession};
pub(crate) use solver::{classify_svg, GlyphTemplateSolver, CHARSET};
