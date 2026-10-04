// HDDT (electronic invoice) integration with hoadondientu.gdt.gov.vn portal.
//
// Scoped around `HddtClient` (struct + impl) rather than free functions, so the
// follow-up operations (profile, catalogs, invoice lookups, …) share the same
// session/flow.
//
// Status (09/2026): the login protocol was reverse-engineered from the portal's
// Next.js bundle and verified with real Chrome + real API calls. The captcha is
// solvable fully OFFLINE via glyph template matching
// (`hddt_captcha::GlyphTemplateSolver`) — measured on the live portal: 8/8
// successful logins, leave-one-out 222/222.
//
// Solver captcha nằm trong gói riêng `./packages/hddt-captcha`; các re-export
// bên dưới giữ nguyên đường dẫn `crate::hddt::*` cho phần còn lại của crate.
pub(crate) mod client;
pub(crate) mod mock_portal;
pub(crate) mod password;
pub(crate) mod session;
pub(crate) mod sync;
pub(crate) mod xml;

pub(crate) use client::{
    HddtClient, InvoiceDirection, InvoiceKind, InvoiceList, InvoiceQuery, LoginError, LoginSession,
    MAX_PAGE_SIZE,
};
pub(crate) use hddt_captcha::{classify_svg, Captcha, CaptchaSolver, GlyphTemplateSolver, CHARSET};
pub(crate) use password::generate_password;
pub(crate) use session::{days_left_until, now_unix, rfc3339_to_unix, PortalSession};
pub(crate) use sync::CachedInvoice;
pub(crate) use xml::{
    export_zip, fill_missing_after_scan, fill_missing_before_import, list_saved, save_from_zip,
    save_manual, xml_dir_under, FillOutcome, SavedXml, SavedXmlRow, XmlInvoiceKey,
};

#[cfg(test)]
mod live;
