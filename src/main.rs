//! Agent Lense — an HTTP proxy that serves the web as markdown.

use std::sync::Arc;

use clap::Parser;
use tokio::net::TcpListener;
use tracing::info;

use agent_lense::browser::BrowserRenderer;
use agent_lense::html::build_client;
use agent_lense::server::build_router;
use agent_lense::{AppState, Config};

/// An HTTP proxy that serves the web with HTML converted to markdown.
#[derive(Parser, Debug)]
#[command(version, about)]
struct Cli {
    /// Port to listen on.
    #[arg(short, long, default_value_t = 3001, env = "AGENT_LENSE_PORT")]
    port: u16,

    /// Address to bind to.
    #[arg(short, long, default_value = "0.0.0.0", env = "AGENT_LENSE_BIND")]
    bind: String,

    /// Timeout in seconds for fetching upstream pages.
    #[arg(short, long, default_value_t = 30, env = "AGENT_LENSE_TIMEOUT")]
    timeout: u64,

    /// Disable Chrome's sandbox and /dev/shm usage (required inside containers).
    #[arg(long, default_value_t = false, env = "AGENT_LENSE_CHROME_NO_SANDBOX")]
    chrome_no_sandbox: bool,
}

#[tokio::main]
async fn main() {
    // Initialize tracing with env filter (default: info).
    // Always suppress noisy chromiumoxide CDP deserialization errors.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"))
                .add_directive("chromiumoxide=off".parse().expect("valid directive")),
        )
        .init();

    let cli = Cli::parse();

    let config = Arc::new(Config { port: cli.port, bind: cli.bind.clone(), timeout: cli.timeout });

    let client = build_client(config.timeout).expect("failed to build HTTP client");

    let (browser, cdp_handle) = match BrowserRenderer::new(cli.chrome_no_sandbox).await {
        Ok((renderer, handle)) => {
            info!("headless browser launched successfully");
            (Some(Arc::new(renderer)), Some(Arc::new(handle)))
        }
        Err(e) => {
            tracing::warn!("failed to launch headless browser, running without JS rendering: {e}");
            (None, None)
        }
    };

    let state = AppState { client, config: config.clone(), browser, cdp_handle };

    let router = build_router(state);

    let addr = format!("{}:{}", config.bind, config.port);
    let listener =
        TcpListener::bind(&addr).await.unwrap_or_else(|e| panic!("failed to bind to {addr}: {e}"));

    info!("agent-lense v{}", env!("CARGO_PKG_VERSION"));
    info!("listening on http://{addr}");

    axum::serve(listener, router).await.expect("server error");
}
