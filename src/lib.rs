//! Agent Lense — library for fetching and converting web pages to markdown.

pub mod browser;
pub mod html;
pub mod markdown;
pub mod mcp;
pub mod server;

use std::sync::Arc;

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use tokio::task::JoinHandle;

use crate::browser::BrowserRenderer;

/// Maximum allowed response body size from upstream (10 MB).
pub const MAX_RESPONSE_SIZE: u64 = 10 * 1024 * 1024;

/// Hop-by-hop headers that should be stripped from upstream responses.
pub const HOP_BY_HOP_HEADERS: &[&str] = &[
    "transfer-encoding",
    "connection",
    "keep-alive",
    "upgrade",
    "te",
    "trailer",
    "proxy-authenticate",
    "proxy-authorization",
];

/// Errors that can occur while proxying a request.
#[derive(Debug, thiserror::Error)]
pub enum ProxyError {
    /// The client supplied an invalid or missing target URL.
    #[error(
        "400 Bad Request: invalid target URL — expected /https://example.com/ or /http://example.com/"
    )]
    BadTargetUrl,

    /// The upstream server could not be reached.
    #[error("502 Bad Gateway: failed to fetch upstream — {0}")]
    UpstreamUnreachable(String),

    /// The upstream request timed out.
    #[error("504 Gateway Timeout: upstream request timed out")]
    Timeout,

    /// The upstream response exceeded the size limit.
    #[error("413 Content Too Large: response exceeded the {MAX_RESPONSE_SIZE} byte limit")]
    ResponseTooLarge,

    /// An internal proxy error.
    #[error("500 Internal Server Error: {0}")]
    Internal(String),
}

impl ProxyError {
    /// Returns the HTTP status code for this error.
    pub const fn status_code(&self) -> StatusCode {
        match self {
            Self::BadTargetUrl => StatusCode::BAD_REQUEST,
            Self::UpstreamUnreachable(_) => StatusCode::BAD_GATEWAY,
            Self::Timeout => StatusCode::GATEWAY_TIMEOUT,
            Self::ResponseTooLarge => StatusCode::PAYLOAD_TOO_LARGE,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl IntoResponse for ProxyError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let body = self.to_string();
        (status, [("content-type", "text/plain; charset=utf-8")], body).into_response()
    }
}

/// Server configuration.
#[derive(Debug, Clone)]
pub struct Config {
    /// Port to listen on.
    pub port: u16,
    /// Address to bind to.
    pub bind: String,
    /// Timeout in seconds for upstream requests.
    pub timeout: u64,
}

/// Shared application state.
#[derive(Debug, Clone)]
pub struct AppState {
    /// HTTP client for upstream requests.
    pub client: reqwest::Client,
    /// Server configuration.
    pub config: Arc<Config>,
    /// Optional headless browser for rendering JS-heavy pages.
    pub browser: Option<Arc<BrowserRenderer>>,
    /// CDP event-loop handle — kept alive for the lifetime of the browser session.
    pub cdp_handle: Option<Arc<JoinHandle<()>>>,
}
