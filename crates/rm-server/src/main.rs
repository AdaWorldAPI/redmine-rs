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

    let bind: SocketAddr = std::env::var("RM_BIND")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| ServerConfig::default().bind);

    serve(ServerConfig { bind }).await
}
