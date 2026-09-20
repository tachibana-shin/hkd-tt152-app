//! Portal session state — the JWT plus its expiry.
//!
//! The portal token lives ~1 day. We keep the session in `AppState` and refresh it
//! before it expires, so the user stays logged in without re-entering the captcha.

use serde::Serialize;

/// Unix time in seconds.
pub(crate) fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Fallback token lifetime when the JWT carries no `exp` claim (observed ~1 day).
const DEFAULT_TTL_SECS: i64 = 24 * 3600;

/// A logged-in HDDT portal session.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct PortalSession {
    pub(crate) username: String,
    pub(crate) token: String,
    /// Token expiry (unix seconds, from the JWT `exp` claim when available).
    pub(crate) expires_at: i64,
    /// Taxpayer profile from `/profile` (kept verbatim for the UI).
    pub(crate) user: Option<serde_json::Value>,
}

impl PortalSession {
    /// Build a session from a fresh token, reading its expiry from the JWT.
    pub(crate) fn new(
        username: impl Into<String>,
        token: impl Into<String>,
        user: Option<serde_json::Value>,
    ) -> Self {
        let token = token.into();
        let expires_at = jwt_exp(&token).unwrap_or_else(|| now_unix() + DEFAULT_TTL_SECS);
        Self {
            username: username.into(),
            token,
            expires_at,
            user,
        }
    }

    /// Seconds until expiry (negative when already expired).
    pub(crate) fn seconds_left(&self) -> i64 {
        self.expires_at - now_unix()
    }

    /// Valid when more than `margin` seconds remain (margin = early re-login).
    pub(crate) fn is_valid(&self, margin: i64) -> bool {
        self.seconds_left() > margin
    }
}

/// Extract `exp` (unix seconds) from a JWT, decoding the payload ourselves so no
/// base64 dependency is needed.
fn jwt_exp(token: &str) -> Option<i64> {
    let payload = token.split('.').nth(1)?;
    let bytes = base64url_decode(payload)?;
    let v: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    v.get("exp").and_then(|e| e.as_i64())
}

/// Minimal base64url (no padding) decoder.
fn base64url_decode(s: &str) -> Option<Vec<u8>> {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let mut acc: u32 = 0;
    let mut bits: u32 = 0;
    for c in s.bytes() {
        if c == b'=' {
            continue;
        }
        let val = ALPHABET.iter().position(|&x| x == c)? as u32;
        acc = (acc << 6) | val;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

/// Convert an RFC 3339 UTC timestamp — e.g. `password_expire` from the portal,
/// `"2026-12-16T05:23:45.570Z"` — to unix seconds. Fractional seconds are
/// truncated; non-UTC offsets are rejected (the portal always sends `Z`).
pub(crate) fn rfc3339_to_unix(s: &str) -> Option<i64> {
    let (date, time) = s.split_once('T')?;
    let time = time.strip_suffix('Z')?;
    let mut it = date.split('-');
    let year: i64 = it.next()?.parse().ok()?;
    let month: u32 = it.next()?.parse().ok()?;
    let day: u32 = it.next()?.parse().ok()?;
    let mut it = time.split(':');
    let hour: u32 = it.next()?.parse().ok()?;
    let minute: u32 = it.next()?.parse().ok()?;
    let second: f64 = it.next()?.parse().ok()?;
    let days = days_from_civil(year, month, day)?;
    Some(days * 86_400 + i64::from(hour) * 3600 + i64::from(minute) * 60 + second as i64)
}

/// Whole days between 1970-01-01 and a civil date (Howard Hinnant's algorithm).
fn days_from_civil(y: i64, m: u32, d: u32) -> Option<i64> {
    if !(1..=12).contains(&m) {
        return None;
    }
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u64; // [0, 399]
    let mp = (u64::from(m) + 9) % 12; // March=0 … February=11
    let doy = (153 * mp + 2) / 5 + (u64::from(d) - 1); // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    Some(era * 146_097 + doe as i64 - 719_468)
}

/// Whole days remaining until `expiry_unix` (negative when already past).
pub(crate) fn days_left_until(expiry_unix: i64) -> i64 {
    (expiry_unix - now_unix()) / 86_400
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base64url_encode(data: &[u8]) -> String {
        const ALPHABET: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
        let mut out = String::new();
        for chunk in data.chunks(3) {
            let b = [
                chunk[0],
                *chunk.get(1).unwrap_or(&0),
                *chunk.get(2).unwrap_or(&0),
            ];
            let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | (b[2] as u32);
            out.push(ALPHABET[((n >> 18) & 63) as usize] as char);
            out.push(ALPHABET[((n >> 12) & 63) as usize] as char);
            if chunk.len() > 1 {
                out.push(ALPHABET[((n >> 6) & 63) as usize] as char);
            }
            if chunk.len() > 2 {
                out.push(ALPHABET[(n & 63) as usize] as char);
            }
        }
        out
    }

    #[test]
    fn decodes_jwt_exp() {
        let encoded = base64url_encode(br#"{"exp":2000000000}"#);
        let token = format!("aaa.{encoded}.sig");
        assert_eq!(jwt_exp(&token), Some(2_000_000_000));
    }

    #[test]
    fn missing_exp_falls_back_to_default_ttl() {
        let s = PortalSession::new("u", "not.a.jwt", None);
        assert!(s.seconds_left() > DEFAULT_TTL_SECS - 60);
    }

    #[test]
    fn expired_token_is_not_valid() {
        let mut s = PortalSession::new("u", "x", None);
        s.expires_at = now_unix() - 10;
        assert!(!s.is_valid(0));
        assert!(s.seconds_left() < 0);
    }

    #[test]
    fn parses_rfc3339_utc_timestamps() {
        assert_eq!(rfc3339_to_unix("1970-01-01T00:00:00.000Z"), Some(0));
        assert_eq!(rfc3339_to_unix("1970-01-02T00:00:00Z"), Some(86_400));
        // 2024-03-01T00:00:00Z — a leap year (fraction gets truncated).
        assert_eq!(
            rfc3339_to_unix("2024-03-01T00:00:00.5Z"),
            Some(1_709_251_200)
        );
    }

    #[test]
    fn rejects_non_utc_or_malformed_timestamps() {
        assert_eq!(rfc3339_to_unix("2026-12-16T05:23:45.570+07:00"), None);
        assert_eq!(rfc3339_to_unix("không phải ngày"), None);
    }

    #[test]
    fn days_left_is_positive_for_a_future_expiry() {
        assert!(days_left_until(rfc3339_to_unix("2026-12-16T05:23:45.570Z").unwrap()) > 0);
    }
}
