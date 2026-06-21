//! Router + middleware assembly.
//!
//! Per the Integration Plan, the router stays small here — width
//! tracks add their routes via merge calls (the convention: each W*
//! track owns one alphabetised block in this file, so merge conflicts
//! on parallel tracks are trivial).

use std::net::SocketAddr;

use axum::routing::get;
use axum::Router;
use rm_auth::Config as AuthConfig;
use rm_handlers::AppState;
use rm_store::Store;
use tower_cookies::CookieManagerLayer;
use tower_http::compression::CompressionLayer;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use crate::handlers::{healthz, index};

/// Process-wide config for the server. Today the only knob is the
/// bind address; the auth-track (W0.3) will extend this with a
/// session-key field, and the store-track (W0.2) with a SurrealDB DSN.
///
/// Constructed by callers; not loaded from env automatically — the
/// `main` binary handles env parsing so this crate stays library-shaped.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// `0.0.0.0:3000` by default.
    pub bind: SocketAddr,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind: SocketAddr::from(([0, 0, 0, 0], 3000)),
        }
    }
}

/// Build the axum router with the default middleware stack wired.
///
/// The shape is intentionally narrow today: a single proof-of-shape
/// `index` route + a health probe. Width tracks (W1..W8) merge their
/// resource routers in here as they land — the merge calls add
/// alphabetically so parallel branches don't conflict.
///
/// Middleware stack (outer → inner):
///   - `TraceLayer`: structured per-request log lines.
///   - `CompressionLayer`: gzip on the way out.
///   - `CorsLayer`: permissive for now; D3 (RBAC) tightens it when
///     auth lands.
///   - `CookieManagerLayer`: the slot W0.3 (rm-auth) layers its
///     session middleware on top of.
pub fn build_router() -> Router {
    Router::new()
        .route("/", get(index))
        .route("/healthz", get(healthz))
        .layer(CookieManagerLayer::new())
        .layer(CorsLayer::permissive())
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
}

/// Build the full router with all Phase-1 resource handlers + the
/// auth surface mounted.
///
/// Per Plan §8 file ownership, **this** is the merge file the W*
/// tracks add to. The convention: each W track adds its `.merge(...)`
/// call in alphabetical order on `path` so parallel branches don't
/// conflict.
///
/// Today's mounts:
/// - `rm_auth::router(auth_cfg)` — `/login`, `/logout`, `/me`
/// - `rm_handlers::issues::router(state)` — `/issues`, `/issues/:id` (W1)
pub fn build_router_with(store: Store, auth_cfg: AuthConfig) -> Router {
    let state = AppState { store };
    Router::new()
        .route("/", get(index))
        .route("/healthz", get(healthz))
        // ── W* width tracks — keep merge calls alphabetised on the URL
        //    path so parallel branches don't conflict on this file. ──
        .merge(rm_handlers::issues::router(state.clone())) // W1: /issues
        .merge(rm_handlers::projects::router(state.clone())) // W2: /projects
        .merge(rm_handlers::time_entries::router(state.clone())) // W3: /time_entries
        // ── Phase-0 auxiliary surfaces ──
        .merge(rm_auth::router(auth_cfg)) //               /login, /logout, /me
        .layer(CookieManagerLayer::new())
        .layer(CorsLayer::permissive())
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
}

/// Run the server until shutdown signal. The single entry point the
/// `main` binary calls after env parsing.
///
/// Listens on `config.bind`. Stops on `SIGINT` / `SIGTERM`.
///
/// # Errors
///
/// Surfaces the underlying `tokio` / `axum` error if binding fails,
/// the listener errors, or graceful-shutdown plumbing breaks.
pub async fn serve(config: ServerConfig) -> std::io::Result<()> {
    // Boot the Phase-0 substrate the resource handlers depend on.
    let store = Store::open()
        .await
        .map_err(|e| std::io::Error::other(format!("store open: {e}")))?;
    let auth_cfg = AuthConfig::key_from_env().unwrap_or_else(AuthConfig::with_random_key);

    let app = build_router_with(store, auth_cfg);
    let listener = tokio::net::TcpListener::bind(config.bind).await?;
    tracing::info!(bind = %config.bind, "rm-server listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
}

/// Wait for SIGINT (Ctrl-C) or SIGTERM. Used by the bin to trigger
/// `axum::serve`'s graceful-shutdown branch.
async fn shutdown_signal() {
    use tokio::signal;

    let ctrl_c = async {
        signal::ctrl_c().await.ok();
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .ok()
            .map(|mut s| async move { s.recv().await })
            .unwrap()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!("rm-server shutdown signal received");
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    #[tokio::test]
    async fn router_serves_index() {
        let app = build_router();
        let response = app
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let s = std::str::from_utf8(&body).unwrap();
        assert!(s.contains("<!doctype html>"));
        assert!(s.contains("data-class-id=\"0x0102\""));
    }

    #[tokio::test]
    async fn router_serves_healthz() {
        let app = build_router();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/healthz")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&body[..], b"ok");
    }

    #[tokio::test]
    async fn router_returns_404_for_unknown_route() {
        let app = build_router();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/not-a-real-route")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 404);
    }

    #[test]
    fn default_server_config_binds_to_0_0_0_0_3000() {
        let cfg = ServerConfig::default();
        assert_eq!(cfg.bind.port(), 3000);
        assert!(cfg.bind.ip().is_unspecified());
    }
}
