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
/// - `rm_handlers::home::router(state)` — `/` (cross-resource overview)
/// - `rm_auth::router(auth_cfg)` — `/login`, `/logout`, `/me`
/// - `rm_handlers::issues::router(state)` — `/issues`, `/issues/:id` (W1)
pub fn build_router_with(store: Store, auth_cfg: AuthConfig) -> Router {
    let state = AppState { store };
    Router::new()
        .route("/healthz", get(healthz))
        // ── W* width tracks — keep merge calls alphabetised on the URL
        //    path so parallel branches don't conflict on this file. The
        //    home overview owns `/`, so it sorts first. ──
        .merge(rm_handlers::home::router(state.clone())) // Home: / (overview)
        .merge(rm_handlers::issues::router(state.clone())) // W1: /issues
        .merge(rm_handlers::issues_form::router(state.clone())) // D1: /issues/new + POST /issues
        .merge(rm_handlers::news::router(state.clone())) // W6a: /news
        .merge(rm_handlers::news_form::router(state.clone())) // D1: /news/new + POST /news
        .merge(rm_handlers::projects::router(state.clone())) // W2: /projects
        .merge(rm_handlers::projects_form::router(state.clone())) // D1: /projects/new + POST /projects
        .merge(rm_handlers::queries::router(state.clone())) // W8a: /queries
        .merge(rm_handlers::relations::router(state.clone())) // W8b: /relations
        .merge(rm_handlers::roles::router(state.clone())) // W4b: /roles
        .merge(rm_handlers::scm::router(state.clone())) // W7: /repositories + /changesets
        .merge(rm_handlers::taxonomy::router(state.clone())) // W5: /issue_statuses + /trackers + /enumerations/issue_priorities
        .merge(rm_handlers::time_entries::router(state.clone())) // W3: /time_entries
        .merge(rm_handlers::users::router(state.clone())) // W4a: /users
        .merge(rm_handlers::wiki_pages::router(state.clone())) // W6b: /wiki
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
    // POC autohydrate: seed the demo corpus on first boot if the store is
    // empty AND `RM_SEED` isn't set to a disable value. Idempotent — a
    // second boot in the same process is a no-op. See rm_store::seed.
    let seeded = rm_store::seed::hydrate_demo_data_on_boot(&store)
        .await
        .map_err(|e| std::io::Error::other(format!("seed: {e}")))?;
    if seeded > 0 {
        tracing::info!(rows = seeded, "demo data hydrated on boot");
    }
    let auth_cfg = AuthConfig::key_from_env().unwrap_or_else(AuthConfig::with_random_key);

    let app = build_router_with(store, auth_cfg);
    // Honour $PORT for PaaS deploys (Railway, Heroku, Cloud Run, Fly route
    // their public 443/80 edge to $PORT internally; the container MUST bind
    // 0.0.0.0:$PORT or the proxy can't reach it). Fall back to the
    // configured host:port for local / non-PaaS deploys.
    let addr = resolve_bind_addr(std::env::var("PORT").ok(), config.bind)?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(bind = %addr, "rm-server listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
}

/// Resolve the bind address from `$PORT` (when set) or the configured
/// fallback. PaaS proxies route their public edge to `$PORT` and require
/// the app to bind `0.0.0.0:$PORT`; non-PaaS keeps the configured
/// `host:port`. Whitespace-only `$PORT` values are treated as unset.
///
/// Pure helper — the env read happens at the [`serve`] call site so the
/// parse logic is testable without touching process env (the crate is
/// `#![forbid(unsafe_code)]`; `std::env::set_var` is unsafe in recent
/// Rust). Tests cover set / unset / whitespace / malformed input.
///
/// Mirrors `op-server`'s same-named helper (op-nexgen PR #58 — both ports
/// land the lance-graph `CONSUMER_SCAN_TODO.md §B1` PaaS pattern).
fn resolve_bind_addr(
    env_port: Option<String>,
    fallback: SocketAddr,
) -> std::io::Result<SocketAddr> {
    match env_port.as_deref().map(str::trim) {
        Some(p) if !p.is_empty() => format!("0.0.0.0:{p}")
            .parse()
            .map_err(|e| std::io::Error::other(format!("invalid PORT env var `{p}`: {e}"))),
        _ => Ok(fallback),
    }
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

    // ── PaaS deploy: $PORT bind (mirrors op-server PR #58 §B1) ──────

    fn fallback() -> SocketAddr {
        ServerConfig::default().bind
    }

    #[test]
    fn resolve_bind_addr_uses_port_env_when_set() {
        let addr = resolve_bind_addr(Some("3000".into()), fallback()).unwrap();
        assert_eq!(addr, "0.0.0.0:3000".parse().unwrap());
    }

    #[test]
    fn resolve_bind_addr_falls_back_when_port_env_is_unset() {
        let addr = resolve_bind_addr(None, fallback()).unwrap();
        assert_eq!(addr, fallback());
    }

    #[test]
    fn resolve_bind_addr_treats_empty_or_whitespace_port_as_unset() {
        for empty in ["", " ", "\t", " \n "] {
            let addr = resolve_bind_addr(Some(empty.into()), fallback()).unwrap();
            assert_eq!(addr, fallback(), "{empty:?} should fall back");
        }
    }

    #[test]
    fn resolve_bind_addr_rejects_malformed_port_with_diagnostic() {
        for bad in ["abc", "70000", "-1", "8080:extra"] {
            let err = resolve_bind_addr(Some(bad.into()), fallback()).unwrap_err();
            let msg = err.to_string();
            assert!(
                msg.contains("invalid PORT") && msg.contains(bad),
                "input {bad:?} should yield a diagnostic naming the value; got {msg}",
            );
        }
    }
}
