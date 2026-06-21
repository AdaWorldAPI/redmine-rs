//! Session cookie helpers — read/set/clear the signed session
//! cookie that carries the current username.
//!
//! The cookie body is just `<username>`; the HMAC signature is
//! managed by tower-cookies' `signed` jar over [`crate::Config::key`].
//! Clients can read the username (it's not a secret) but can't forge
//! it.

use tower_cookies::cookie::SameSite;
use tower_cookies::{Cookie, Cookies, Key};

/// Cookie name. `rm_session` keeps it port-specific so the same
/// browser can hold sessions for OpenProject / Redmine / other
/// AdaWorldAPI ports side-by-side without collisions.
pub const SESSION_COOKIE_NAME: &str = "rm_session";

/// Per-request session view. Constructed by the extractor; handlers
/// receive it via `axum::Extension`.
#[derive(Debug, Clone)]
pub struct Session {
    /// `Some(username)` when the request carries a valid signed
    /// session cookie; `None` otherwise.
    pub username: Option<String>,
}

impl Session {
    /// Read the current session from the request's cookie jar.
    /// Returns a session with `username = None` if the cookie is
    /// absent or its signature doesn't verify.
    pub fn from_cookies(cookies: &Cookies, key: &Key) -> Self {
        let username = cookies
            .signed(key)
            .get(SESSION_COOKIE_NAME)
            .map(|c| c.value().to_string());
        Self { username }
    }

    /// Set the session cookie to the given username (signed). Calls
    /// must already have a valid `Cookies` from
    /// `tower_cookies::CookieManagerLayer`.
    pub fn set(cookies: &Cookies, key: &Key, username: &str) {
        let mut cookie = Cookie::new(SESSION_COOKIE_NAME, username.to_string());
        cookie.set_http_only(true);
        cookie.set_same_site(SameSite::Lax);
        cookie.set_path("/");
        cookies.signed(key).add(cookie);
    }

    /// Clear the session cookie (logout).
    pub fn clear(cookies: &Cookies, key: &Key) {
        // tower_cookies removes by name; the value doesn't matter
        // but Cookie::new requires both.
        cookies
            .signed(key)
            .remove(Cookie::new(SESSION_COOKIE_NAME, ""));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_from_empty_cookies_has_no_user() {
        let cookies = Cookies::default();
        let key = Key::generate();
        let s = Session::from_cookies(&cookies, &key);
        assert!(s.username.is_none());
    }

    #[test]
    fn set_then_read_round_trips_the_username() {
        let cookies = Cookies::default();
        let key = Key::generate();
        Session::set(&cookies, &key, "jsmith");
        let s = Session::from_cookies(&cookies, &key);
        assert_eq!(s.username, Some("jsmith".to_string()));
    }

    #[test]
    fn signature_mismatch_drops_the_session() {
        // A cookie set with one key cannot be read with another —
        // signed cookies enforce this.
        let cookies = Cookies::default();
        let key_a = Key::generate();
        let key_b = Key::generate();
        Session::set(&cookies, &key_a, "jsmith");
        let s = Session::from_cookies(&cookies, &key_b);
        assert!(
            s.username.is_none(),
            "different key must invalidate the signature"
        );
    }

    #[test]
    fn clear_removes_the_session() {
        let cookies = Cookies::default();
        let key = Key::generate();
        Session::set(&cookies, &key, "jsmith");
        Session::clear(&cookies, &key);
        // After clear, the cookie should have a removal marker; the
        // signed jar's get() returns None.
        let s = Session::from_cookies(&cookies, &key);
        assert!(s.username.is_none());
    }
}
