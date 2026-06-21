//! `CurrentUser` axum extractor — `Some(username)` if the request
//! carries a valid signed session cookie, `None` otherwise.
//!
//! Handlers that GATE behind auth use the `Option`-free
//! [`RequireUser`] wrapper (rejects unauthenticated requests with
//! 401); handlers that show different UI based on signed-in vs
//! anonymous (e.g. nav bar) use `CurrentUser`.

use async_trait::async_trait;
use axum::extract::{FromRequestParts, State};
use axum::http::request::Parts;
use axum::http::StatusCode;
use tower_cookies::Cookies;

use crate::session::Session;
use crate::Config;

/// `Some(username)` if the request carries a valid signed session
/// cookie; `None` otherwise. Extracts in O(1) — just reads the
/// existing cookie jar.
#[derive(Debug, Clone)]
pub struct CurrentUser(pub Option<String>);

#[async_trait]
impl<S> FromRequestParts<S> for CurrentUser
where
    S: Send + Sync,
    Config: axum::extract::FromRef<S>,
{
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        // Cookies layer must be installed upstream (rm-server wires
        // CookieManagerLayer in build_router).
        let cookies = Cookies::from_request_parts(parts, state)
            .await
            .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "cookies layer missing"))?;
        let State(cfg): State<Config> = State::from_request_parts(parts, state)
            .await
            .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "auth config missing"))?;
        let session = Session::from_cookies(&cookies, &cfg.key);
        Ok(CurrentUser(session.username))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lightweight unit on CurrentUser's `Some` / `None` semantics
    /// — the full extractor flow is exercised in the handler tests
    /// (which exercise the end-to-end cookie round trip via axum
    /// oneshot).
    #[test]
    fn current_user_some_pattern_matches() {
        let cu = CurrentUser(Some("jsmith".to_string()));
        match cu.0 {
            Some(name) => assert_eq!(name, "jsmith"),
            None => panic!("expected Some"),
        }
    }

    #[test]
    fn current_user_none_pattern_matches() {
        let cu = CurrentUser(None);
        assert!(cu.0.is_none());
    }

    #[test]
    fn config_clone_keeps_the_key_in_sync() {
        // The cloned Config must HMAC-verify cookies signed with the
        // original — that's what `Key` being Arc-shared guarantees,
        // and what the State<Config> path depends on.
        let cfg = Config::with_random_key();
        let cfg2 = cfg.clone();
        let cookies = Cookies::default();
        Session::set(&cookies, &cfg.key, "jsmith");
        let s = Session::from_cookies(&cookies, &cfg2.key);
        assert_eq!(s.username.as_deref(), Some("jsmith"));
    }
}
