//! Session tokens, sign-in codes and the cookie. Postgres stores hashes
//! only: a copied database yields no usable token, and sign-out deletes the
//! row so a copied cookie is dead.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rand::RngCore;
use sha2::{Digest, Sha256};

/// The cookie name.
pub const SESSION_COOKIE: &str = "skyspace_session";

/// Lowercase hex of `bytes`, for log fields and `ETag`s.
#[must_use]
pub fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut out, b| {
            let _ = write!(out, "{b:02x}");
            out
        })
}

/// A freshly minted session token: the value for the cookie and the hash
/// for the database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionToken {
    /// 32 random bytes, base64url without padding. Goes in the cookie.
    pub cookie_value: String,
    /// SHA-256 of the raw bytes. Goes in `sessions.token_sha256`.
    pub sha256: [u8; 32],
}

/// Mint a token from the OS random source.
#[must_use]
pub fn mint_session_token() -> SessionToken {
    let mut raw = [0u8; 32];
    rand::rng().fill_bytes(&mut raw);
    SessionToken {
        cookie_value: URL_SAFE_NO_PAD.encode(raw),
        sha256: Sha256::digest(raw).into(),
    }
}

/// The hash of a cookie value, or `None` when the value is not a token we minted.
#[must_use]
pub fn hash_cookie_value(value: &str) -> Option<[u8; 32]> {
    let raw = URL_SAFE_NO_PAD.decode(value).ok()?;
    if raw.len() != 32 {
        return None;
    }
    Some(Sha256::digest(&raw).into())
}

/// A six-digit sign-in code, zero-padded.
#[must_use]
pub fn mint_login_code() -> String {
    let n = rand::rng().next_u32() % 1_000_000;
    format!("{n:06}")
}

/// SHA-256 of a code as the student typed it, digits only.
#[must_use]
pub fn hash_login_code(email: &str, code: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(email.trim().to_ascii_lowercase().as_bytes());
    hasher.update(b":");
    hasher.update(code.trim().as_bytes());
    hasher.finalize().into()
}

/// The `Set-Cookie` value for a session: `HttpOnly`, `Secure`, `SameSite=Lax`, `Path=/`.
#[must_use]
pub fn session_cookie(value: &str, max_age_seconds: i64, secure: bool) -> String {
    let secure = if secure { "; Secure" } else { "" };
    format!(
        "{SESSION_COOKIE}={value}; Max-Age={max_age_seconds}; Path=/; HttpOnly{secure}; SameSite=Lax"
    )
}

/// The `Set-Cookie` value that clears the session.
#[must_use]
pub fn clear_session_cookie(secure: bool) -> String {
    session_cookie("", 0, secure)
}

/// The session cookie's value from a `Cookie` header, if present.
#[must_use]
pub fn cookie_from_header(header: &str) -> Option<&str> {
    header.split(';').find_map(|pair| {
        let (name, value) = pair.trim().split_once('=')?;
        (name == SESSION_COOKIE && !value.is_empty()).then_some(value)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_round_trip_through_the_cookie() {
        let token = mint_session_token();
        assert_eq!(hash_cookie_value(&token.cookie_value), Some(token.sha256));
        assert_ne!(mint_session_token().cookie_value, token.cookie_value);
        assert_eq!(hash_cookie_value("short"), None);
    }

    #[test]
    fn hex_is_lowercase_and_padded() {
        assert_eq!(hex(&[0, 15, 255]), "000fff");
    }

    #[test]
    fn login_codes_are_six_digits() {
        let code = mint_login_code();
        assert_eq!(code.len(), 6);
        assert!(code.bytes().all(|b| b.is_ascii_digit()));
        assert_eq!(
            hash_login_code("Owl@Rice.edu", "123456"),
            hash_login_code("owl@rice.edu ", " 123456")
        );
    }

    #[test]
    fn cookie_header_is_read_and_written() {
        let set = session_cookie("abc", 60, true);
        assert!(set.starts_with(
            "skyspace_session=abc; Max-Age=60; Path=/; HttpOnly; Secure; SameSite=Lax"
        ));
        assert_eq!(
            cookie_from_header("a=1; skyspace_session=abc; b=2"),
            Some("abc")
        );
        assert_eq!(cookie_from_header("skyspace_session="), None);
        assert!(clear_session_cookie(false).contains("Max-Age=0"));
    }
}
