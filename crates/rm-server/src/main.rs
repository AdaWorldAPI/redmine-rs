//! `rm-server` binary entry — wires env parsing, tracing, and
//! [`rm_server::serve`].
//!
//! Today the env surface is minimal (`RM_BIND` overrides the bind
//! address). Width / depth tracks add their env knobs by extending
//! [`ServerConfig`].

use std::net::SocketAddr;

use rm_server::{serve, ServerConfig};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    // `RUST_LOG=info` by default; callers override via env.
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    // Distinguish unset from malformed (codex P1 on PR #7). Silently
    // falling back to the default for a typo'd RM_BIND can make a
    // bad deployment listen on the wrong interface or port.
    let bind: SocketAddr = match std::env::var("RM_BIND") {
        Err(std::env::VarError::NotPresent) => ServerConfig::default().bind,
        Err(e) => {
            return Err(std::io::Error::other(format!("RM_BIND env read: {e}")));
        }
        Ok(raw) => raw.parse().map_err(|e| {
            std::io::Error::other(format!("RM_BIND='{raw}' is not a valid SocketAddr: {e}"))
        })?,
    };

    serve(ServerConfig { bind }).await
}
