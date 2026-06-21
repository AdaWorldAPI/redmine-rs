//! `rm-server` — Redmine-RS HTTP server scaffold.
//!
//! W0.1 of the [Redmine Integration Plan][plan]. The single sequential
//! gate before everything else fans out parallel — wires the axum
//! router, middleware stack, and an end-to-end proof that the
//! askama-driven render kit reaches the browser.
//!
//! [plan]: https://github.com/AdaWorldAPI/OGAR/blob/main/docs/integration/REDMINE-INTEGRATION-PLAN.md
//!
//! # Shape
//!
//! ```text
//!   GET /                  → render an empty HtmlListView for
//!                            `project_work_item` (the headline concept;
//!                            proves the askama bridge works)
//!   GET /healthz           → 200 OK; for load-balancer probes
//!   GET /assets/*          → static-file fallback (tower-http::fs)
//! ```
//!
//! Width tracks (`W1..W8`) add their resource routes via
//! [`AppRouter::merge_resource`]; each owns one file under
//! `rm-handlers/src/<resource>.rs` plus one merge call here. The plan
//! §8 lays out the file-ownership contract so no two parallel tracks
//! write the same file.
//!
//! # What this is NOT
//!
//! - Not a data layer — `rm-store` (W0.2) lands SurrealDB integration.
//! - Not an auth surface — `rm-auth` (W0.3) lands session middleware
//!   and login. This crate sets up [`tower_cookies::CookieManagerLayer`]
//!   so W0.3 has the slot to wire its session layer through.
//! - Not the rendered layout chrome — `G1` lands the `base.askama`
//!   master template + nav. Today the hello-world handler emits a
//!   minimal HTML doc inline so the round-trip is end-to-end-visible.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod error;
pub mod handlers;
pub mod router;

pub use error::AppError;
pub use router::{build_router, serve, ServerConfig};
