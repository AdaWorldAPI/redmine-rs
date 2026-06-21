//! Login / logout / `/me` handlers + the `router()` builder
//! `rm-server` merges in.

use axum::extract::{Form, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::Router;
use serde::Deserialize;
use tower_cookies::Cookies;

use crate::session::Session;
use crate::users::verify_seed;
use crate::{Config, CurrentUser};

/// Form body for `POST /login`. Comes in as
/// `application/x-www-form-urlencoded`.
#[derive(Debug, Deserialize)]
pub struct LoginForm {
    /// Lowercase login.
    pub login: String,
    /// Plaintext password.
    pub password: String,
}

/// `POST /login` — verify credentials against the seed table, set
/// the signed session cookie on success.
///
/// Returns:
/// - 200 + `{username}` body on success
/// - 401 + `"invalid credentials"` body on failure
///
/// W4 swaps `verify_seed` for `verify_via_store`; the handler shape
/// stays.
pub async fn login(
    cookies: Cookies,
    State(cfg): State<Config>,
    Form(form): Form<LoginForm>,
) -> impl IntoResponse {
    match verify_seed(&form.login, &form.password) {
        Some(user) => {
            Session::set(&cookies, &cfg.key, user.login);
            tracing::info!(login = %user.login, "user logged in");
            (StatusCode::OK, user.login.to_string())
        }
        None => {
            tracing::warn!(login = %form.login, "login failed");
            (StatusCode::UNAUTHORIZED, "invalid credentials".to_string())
        }
    }
}

/// `POST /logout` — clear the session cookie. Returns 200 unconditionally
/// (idempotent — calling /logout when already logged out is fine).
pub async fn logout(cookies: Cookies, State(cfg): State<Config>) -> impl IntoResponse {
    Session::clear(&cookies, &cfg.key);
    (StatusCode::OK, "logged out")
}

/// `GET /me` — return the current username (200 + plain-text body)
/// or 401 + `"unauthenticated"` if there's no valid session.
pub async fn me(CurrentUser(user): CurrentUser) -> impl IntoResponse {
    match user {
        Some(name) => (StatusCode::OK, name),
        None => (StatusCode::UNAUTHORIZED, "unauthenticated".to_string()),
    }
}

/// Build the auth router. `rm-server` merges this into its top-level
/// router with `app.merge(rm_auth::router(cfg))`; the `Config` is
/// shared via `State`.
pub fn router(cfg: Config) -> Router {
    Router::new()
        .route("/login", post(login))
        .route("/logout", post(logout))
        .route("/me", get(me))
        .with_state(cfg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{header, Request};
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    use tower_cookies::CookieManagerLayer;

    fn app() -> Router {
        Router::new()
            .merge(router(Config::with_random_key()))
            .layer(CookieManagerLayer::new())
    }

    fn form_body(login: &str, password: &str) -> Body {
        Body::from(format!("login={login}&password={password}"))
    }

    #[tokio::test]
    async fn login_with_seed_credentials_sets_session_cookie() {
        let app = app();
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/login")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(form_body("jsmith", "jsmith"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let set_cookie = response
            .headers()
            .get(header::SET_COOKIE)
            .expect("login must set a cookie");
        let s = set_cookie.to_str().unwrap();
        assert!(s.starts_with("rm_session="), "got: {s}");
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&body[..], b"jsmith");
    }

    #[tokio::test]
    async fn login_with_bad_credentials_returns_401() {
        let app = app();
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/login")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(form_body("jsmith", "WRONG"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        // No cookie on failure.
        assert!(response.headers().get(header::SET_COOKIE).is_none());
    }

    #[tokio::test]
    async fn me_returns_401_when_unauthenticated() {
        let app = app();
        let response = app
            .oneshot(Request::builder().uri("/me").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn login_then_me_round_trips_the_username() {
        // The W0.3 headline DoD: POST /login -> set cookie -> GET
        // /me with the cookie -> 200 + username.
        let cfg = Config::with_random_key();
        let app = Router::new()
            .merge(router(cfg.clone()))
            .layer(CookieManagerLayer::new());

        // 1) login
        let login_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/login")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(form_body("admin", "admin"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(login_res.status(), StatusCode::OK);
        let cookie = login_res
            .headers()
            .get(header::SET_COOKIE)
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        // tower-cookies' Set-Cookie carries attributes after `; ` — for
        // the Cookie request header we only need the name=value
        // before the first `;`.
        let cookie_header = cookie.split(';').next().unwrap().to_string();

        // 2) /me with the cookie
        let me_res = app
            .oneshot(
                Request::builder()
                    .uri("/me")
                    .header(header::COOKIE, cookie_header)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(me_res.status(), StatusCode::OK);
        let body = me_res.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&body[..], b"admin");
    }

    #[tokio::test]
    async fn logout_clears_the_session() {
        let cfg = Config::with_random_key();
        let app = Router::new()
            .merge(router(cfg.clone()))
            .layer(CookieManagerLayer::new());

        // Login first to get a valid cookie.
        let login_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/login")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(form_body("admin", "admin"))
                    .unwrap(),
            )
            .await
            .unwrap();
        let cookie = login_res
            .headers()
            .get(header::SET_COOKIE)
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        let cookie_header = cookie.split(';').next().unwrap().to_string();

        // Logout — should set a cookie clearing rm_session.
        let logout_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/logout")
                    .header(header::COOKIE, &cookie_header)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(logout_res.status(), StatusCode::OK);
        let clear_cookie = logout_res
            .headers()
            .get(header::SET_COOKIE)
            .expect("logout sets a removal Set-Cookie")
            .to_str()
            .unwrap();
        assert!(
            clear_cookie.contains("rm_session=") && clear_cookie.contains("Max-Age=0"),
            "logout cookie should be a removal: {clear_cookie}"
        );
    }
}
