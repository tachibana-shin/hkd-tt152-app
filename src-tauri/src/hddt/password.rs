//! New-password generation matching the portal's policy (read from the change-
//! password form validation in the portal bundle, 09/2026):
//!   - length 8..=15;
//!   - must contain at least one lowercase, one uppercase, one digit and one
//!     special character from `!@#$%&*()_+-=[]|,./?><`;
//!   - must differ from the current password (checked by the caller).
//!
//! Entropy comes from a random UUID v4, so no extra RNG dependency is needed.

const LOWERCASE: &[u8] = b"abcdefghijklmnopqrstuvwxyz";
const UPPERCASE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const DIGITS: &[u8] = b"0123456789";
const SPECIALS: &[u8] = b"!@#$%&*()_+-=[]|,./?><";
const ALL: &[u8] =
    b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789!@#$%&*()_+-=[]|,./?><";

const PASSWORD_LEN: usize = 12;

/// Generate a 12-char password that satisfies the portal's policy.
pub(crate) fn generate_password() -> String {
    let bytes = uuid::Uuid::new_v4().into_bytes(); // 16 random bytes
                                                   // Guarantee one character of each required class …
    let mut pw: Vec<u8> = vec![
        LOWERCASE[(bytes[0] % LOWERCASE.len() as u8) as usize],
        UPPERCASE[(bytes[1] % UPPERCASE.len() as u8) as usize],
        DIGITS[(bytes[2] % DIGITS.len() as u8) as usize],
        SPECIALS[(bytes[3] % SPECIALS.len() as u8) as usize],
    ];
    // … then fill the rest from the full alphabet.
    for i in 4..PASSWORD_LEN {
        pw.push(ALL[(bytes[i] % ALL.len() as u8) as usize]);
    }
    // Pseudo-random shuffle seeded by the uuid bytes.
    for i in (1..pw.len()).rev() {
        let j = usize::from(bytes[i % bytes.len()]) % (i + 1);
        pw.swap(i, j);
    }
    String::from_utf8(pw).expect("password is pure ASCII")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The exact policy the portal applies to `new_password`, checked without
    /// regex look-ahead (the Rust `regex` crate rejects `(?=...)`).
    fn matches_portal_policy(pw: &str) -> bool {
        (8..=15).contains(&pw.len())
            && pw.bytes().any(|c| c.is_ascii_lowercase())
            && pw.bytes().any(|c| c.is_ascii_uppercase())
            && pw.bytes().any(|c| c.is_ascii_digit())
            && pw.bytes().any(|c| b"!@#$%&*()_+-=[]|,./?><".contains(&c))
            && pw.bytes().all(|c| {
                b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789!@#$%&*()_+-=[]|,./?><"
                    .contains(&c)
            })
    }

    #[test]
    fn generated_password_matches_portal_policy() {
        for _ in 0..200 {
            let pw = generate_password();
            assert_eq!(pw.len(), PASSWORD_LEN);
            assert!(
                matches_portal_policy(&pw),
                "password {pw:?} fails portal policy"
            );
        }
    }

    #[test]
    fn generated_passwords_differ() {
        let a = generate_password();
        let b = generate_password();
        assert_ne!(a, b);
    }
}
