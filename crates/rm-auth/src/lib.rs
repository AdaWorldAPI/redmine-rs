//! `rm-auth` — signed-cookie session middleware + login flow for
//! Redmine-RS.
//!
//! W0.3 of the [Redmine Integration Plan][plan]. Layers on top of
//! `rm-server`'s [`tower_cookies::CookieManagerLayer`] (already wired)
//! to add:
//!
//! - `POST /login` — verify credentials, set a **signed** session
//!   cookie carrying the username
//! - `POST /logout` — clear the session cookie
//! - `GET /me` — return the current username (200) or 401
//! - [`CurrentUser`] axum extractor for downstream handlers
//!
//! [plan]: https://github.com/AdaWorldAPI/OGAR/blob/main/docs/integration/REDMINE-INTEGRATION-PLAN.md
//!
//! # Out of scope (today)
//!
//! - **DB-backed users** — W4 (Plan §3 width track, User / Member /
//!   Role / MemberRole admin pages) lands users persisted in the
//!   `project_actor` table with argon2 password hashes via a
//!   sibling `rm_credentials` table.
//! - **RBAC enforcement** — D3 (Plan §4 depth track) uses the
//!   `CurrentUser` extractor + a `RequirePermission<P>` wrapper to
//!   gate routes.
//! - **OAuth / SAML / SSO** — out of MVP; a sibling crate later.
//!
//! Today's seed-user table is in-code (see [`users::SEED_USERS`]) so
//! the login flow is wireable end-to-end without depending on W0.2's
//! store. When W4 lands DB-backed users, [`users::verify_seed`] gets
//! a sibling `users::verify_via_store` and `SEED_USERS` becomes a
//! dev-only feature flag.
//!
//! # Signed vs encrypted cookies
//!
//! tower-cookies' `signed` cookies use HMAC: the client can READ the
//! cookie body (it's just the username) but can't FORGE it without
//! the server's key. Encrypted cookies would hide the body but the
//! username isn't a secret (the same user trivially sees it in
//! `/me`). Signed is the right primitive here.
//!
//! Forcing the key to be process-stable + secret matters: a
//! restart shouldn't invalidate every session, but the key
//! shouldn't be checked into the repo. [`Config::key_from_env`]
//! reads `RM_SESSION_KEY` (base64-decoded, 32+ bytes) and falls back
//! to a fresh random key for dev. Production must set the env var.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod config;
mod extractor;
mod handlers;
mod session;
mod users;

pub use config::Config;
pub use extractor::CurrentUser;
pub use handlers::router;
pub use session::{Session, SESSION_COOKIE_NAME};
pub use users::SeedUser;
