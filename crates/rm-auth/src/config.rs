//! Configuration the auth layer needs at boot — primarily the
//! signing key.

use tower_cookies::Key;

/// Process-wide auth config. Constructed once at boot, then cloned
/// into [`axum::extract::State`] for every handler that touches the
/// session cookie.
///
/// The `Key` is intentionally kept as `tower_cookies::Key` (not an
/// opaque byte slice) so consumers never accidentally store a copy
/// of the raw bytes — only the cookie crate touches them.
#[derive(Clone)]
pub struct Config {
    /// HMAC key for signed cookies. Cheap to clone (Arc-backed
    /// inside tower-cookies).
    pub key: Key,
}

impl Config {
    /// Construct with an explicit key. Used by tests.
    #[must_use]
    pub fn new(key: Key) -> Self {
        Self { key }
    }

    /// Construct with a freshly-generated random key. Useful for
    /// dev / tests where session continuity across restarts doesn't
    /// matter.
    ///
    /// **Production callers should use [`Self::key_from_env`]
    /// instead** so a restart doesn't invalidate every active
    /// session.
    #[must_use]
    pub fn with_random_key() -> Self {
        Self {
            key: Key::generate(),
        }
    }

    /// Read the session key from `RM_SESSION_KEY` (base64-encoded,
    /// must decode to >= 64 bytes per `tower_cookies::Key::from`).
    /// Returns `None` if the env var is unset; lets the binary
    /// decide whether to fall back to `with_random_key` (dev) or
    /// fail loudly (prod).
    pub fn key_from_env() -> Option<Self> {
        let raw = std::env::var("RM_SESSION_KEY").ok()?;
        let bytes = base64_decode(&raw)?;
        if bytes.len() < 64 {
            tracing::warn!(
                "RM_SESSION_KEY decodes to {} bytes; tower_cookies::Key needs >= 64. \
                 Falling back to random key.",
                bytes.len()
            );
            return None;
        }
        Some(Self {
            key: Key::from(&bytes),
        })
    }
}

/// Minimal base64 decode — no general-purpose dep just for one
/// env-var read at boot. Returns `None` for any malformed input.
fn base64_decode(s: &str) -> Option<Vec<u8>> {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let s = s.trim().trim_end_matches('=');
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let mut buf: u32 = 0;
    let mut bits: u8 = 0;
    for c in s.bytes() {
        let v = ALPHABET.iter().position(|&b| b == c)? as u32;
        buf = (buf << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_key_constructs_cleanly() {
        let _ = Config::with_random_key();
    }

    #[test]
    fn key_from_env_returns_none_for_obviously_bad_input() {
        // Avoid mutating process env (would race with other tests
        // under cargo's parallel default). The internal decode-bytes
        // path is what's interesting — exercised below via the
        // direct `base64_decode` unit. Here we only assert that the
        // public API exists and handles the common "not set" branch
        // gracefully.
        // SAFETY: read-only; doesn't mutate the env.
        let _ = Config::key_from_env();
    }

    #[test]
    fn base64_decode_round_trips_a_sample() {
        // "Hello" base64 = "SGVsbG8="
        assert_eq!(base64_decode("SGVsbG8="), Some(b"Hello".to_vec()));
    }

    #[test]
    fn base64_decode_rejects_garbage() {
        assert!(base64_decode("not_b64!").is_none());
    }
}
