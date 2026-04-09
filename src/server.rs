//! HTTP server, routing, and request handling.
//!
//! Builds the axum router and handles proxy requests: extracting the target URL,
//! forwarding the request upstream, and converting HTML responses to markdown.

use axum::Router;
use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;

use crate::html::fetch_upstream;
use crate::markdown::html_to_markdown;
use crate::{AppState, HOP_BY_HOP_HEADERS, ProxyError};

/// Builds the axum router with shared application state.
pub fn build_router(state: AppState) -> Router {
    Router::new().route("/", get(root_handler)).fallback(proxy_handler).with_state(state)
}

/// Root endpoint — returns a usage hint and serves as a health check.
async fn root_handler() -> impl IntoResponse {
    (
        StatusCode::OK,
        [("content-type", "text/plain; charset=utf-8")],
        format!(
            "agent-lense v{version}\n\n\
             Usage: append a full URL to the path.\n\n\
             \x20 curl http://localhost:3001/https://example.com/\n\n\
             Documentation: https://github.com/steelbrain/agent-lens\n",
            version = env!("CARGO_PKG_VERSION"),
        ),
    )
}

/// HTTP methods the proxy accepts (safe / idempotent only).
const ALLOWED_METHODS: &[Method] = &[Method::GET, Method::HEAD, Method::OPTIONS];

/// Main proxy handler — fetches upstream, converts HTML to markdown, passes through non-HTML.
async fn proxy_handler(
    State(state): State<AppState>,
    request: Request,
) -> Result<Response, ProxyError> {
    let method = request.method().clone();

    if !ALLOWED_METHODS.contains(&method) {
        return Ok((
            StatusCode::METHOD_NOT_ALLOWED,
            [("allow", "GET, HEAD, OPTIONS")],
            "405 Method Not Allowed: only GET, HEAD, and OPTIONS are supported",
        )
            .into_response());
    }

    let path = request.uri().path();
    let query = request.uri().query();

    let target_url = extract_target_url(path, query)?;

    let headers = request.headers().clone();

    let upstream = fetch_upstream(&state.client, &target_url, method, headers, None).await?;

    let status = StatusCode::from_u16(upstream.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);

    // Check for redirects — rewrite the Location header
    if status.is_redirection() {
        return build_redirect_response(status, &upstream.headers, &target_url);
    }

    if upstream.is_html() {
        let html_body = if let Some(ref browser) = state.browser {
            match browser.render(&target_url, state.config.timeout).await {
                Ok(rendered) => rendered.into_bytes(),
                Err(e) => {
                    tracing::warn!("browser render failed, falling back to raw HTML: {e}");
                    upstream.body
                }
            }
        } else {
            upstream.body
        };
        build_markdown_response(status, &upstream.headers, &html_body, &target_url)
    } else {
        build_passthrough_response(status, &upstream.headers, upstream.body)
    }
}

/// Extracts the target URL from the proxy request path.
///
/// Expects paths like `/https://example.com/path?q=1`.
fn extract_target_url(path: &str, query: Option<&str>) -> Result<String, ProxyError> {
    // Strip leading `/`
    let raw = path.strip_prefix('/').unwrap_or(path);

    if raw.is_empty() {
        return Err(ProxyError::BadTargetUrl);
    }

    let mut url = raw.to_string();
    if let Some(q) = query {
        url.push('?');
        url.push_str(q);
    }

    // Validate it's a proper http/https URL
    match url::Url::parse(&url) {
        Ok(parsed) if parsed.scheme() == "http" || parsed.scheme() == "https" => Ok(url),
        _ => Err(ProxyError::BadTargetUrl),
    }
}

/// Builds a redirect response with the Location header rewritten to a proxy-relative path.
fn build_redirect_response(
    status: StatusCode,
    upstream_headers: &axum::http::HeaderMap,
    target_url: &str,
) -> Result<Response, ProxyError> {
    let mut builder = Response::builder().status(status);

    // Copy upstream headers, stripping hop-by-hop
    for (name, value) in upstream_headers {
        if is_hop_by_hop(name.as_str()) {
            continue;
        }
        if name.as_str().eq_ignore_ascii_case("location")
            && let Ok(location) = value.to_str()
        {
            let rewritten = rewrite_location(location, target_url);
            builder = builder.header(name, &rewritten);
            continue;
        }
        builder = builder.header(name, value);
    }

    builder
        .body(Body::empty())
        .map_err(|e| ProxyError::Internal(format!("failed to build redirect response: {e}")))
}

/// Rewrites a `Location` header value to a proxy-relative path.
///
/// Absolute URLs are prefixed with `/`.  Relative URLs are resolved against
/// the original target URL first, then prefixed.
fn rewrite_location(location: &str, target_url: &str) -> String {
    if location.starts_with("http://") || location.starts_with("https://") {
        return format!("/{location}");
    }

    // Relative location — resolve against the original target URL
    if let Ok(base) = url::Url::parse(target_url)
        && let Ok(resolved) = base.join(location)
    {
        return format!("/{resolved}");
    }

    // Fallback: return as-is (shouldn't happen with valid URLs)
    location.to_string()
}

/// Builds a response with the HTML body converted to markdown.
fn build_markdown_response(
    status: StatusCode,
    upstream_headers: &axum::http::HeaderMap,
    body: &[u8],
    source_url: &str,
) -> Result<Response, ProxyError> {
    let html = String::from_utf8_lossy(body);
    let markdown = html_to_markdown(&html, source_url, false);
    let md_bytes = markdown.into_bytes();

    let mut builder = Response::builder().status(status);

    // Copy upstream headers, overriding content-related ones
    for (name, value) in upstream_headers {
        let name_str = name.as_str();
        if is_hop_by_hop(name_str) {
            continue;
        }
        // Skip content headers we'll override
        if name_str.eq_ignore_ascii_case("content-type")
            || name_str.eq_ignore_ascii_case("content-length")
            || name_str.eq_ignore_ascii_case("content-encoding")
        {
            continue;
        }
        builder = builder.header(name, value);
    }

    builder = builder.header("content-type", "text/markdown; charset=utf-8");
    builder = builder.header("content-length", md_bytes.len().to_string());

    builder
        .body(Body::from(md_bytes))
        .map_err(|e| ProxyError::Internal(format!("failed to build markdown response: {e}")))
}

/// Builds a passthrough response for non-HTML content.
fn build_passthrough_response(
    status: StatusCode,
    upstream_headers: &axum::http::HeaderMap,
    body: Vec<u8>,
) -> Result<Response, ProxyError> {
    let mut builder = Response::builder().status(status);

    for (name, value) in upstream_headers {
        if is_hop_by_hop(name.as_str()) {
            continue;
        }
        builder = builder.header(name.clone(), value.clone());
    }

    builder
        .body(Body::from(body))
        .map_err(|e| ProxyError::Internal(format!("failed to build passthrough response: {e}")))
}

/// Returns `true` if the header name is a hop-by-hop header.
fn is_hop_by_hop(name: &str) -> bool {
    HOP_BY_HOP_HEADERS.iter().any(|h| name.eq_ignore_ascii_case(h))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_valid_https_url() {
        let result = extract_target_url("/https://example.com/path", None);
        assert_eq!(result.unwrap(), "https://example.com/path");
    }

    #[test]
    fn extract_valid_http_url() {
        let result = extract_target_url("/http://example.com/", None);
        assert_eq!(result.unwrap(), "http://example.com/");
    }

    #[test]
    fn extract_url_with_query() {
        let result = extract_target_url("/https://example.com/search", Some("q=rust&page=1"));
        assert_eq!(result.unwrap(), "https://example.com/search?q=rust&page=1");
    }

    #[test]
    fn extract_invalid_url() {
        let result = extract_target_url("/not-a-url", None);
        assert!(result.is_err());
    }

    #[test]
    fn extract_empty_path() {
        let result = extract_target_url("/", None);
        assert!(result.is_err());
    }

    #[test]
    fn hop_by_hop_detected() {
        assert!(is_hop_by_hop("Transfer-Encoding"));
        assert!(is_hop_by_hop("connection"));
        assert!(is_hop_by_hop("Keep-Alive"));
    }

    #[test]
    fn non_hop_by_hop_passes() {
        assert!(!is_hop_by_hop("content-type"));
        assert!(!is_hop_by_hop("x-custom-header"));
    }

    #[test]
    fn rewrite_location_absolute() {
        let result = rewrite_location("https://example.com/new", "https://example.com/old");
        assert_eq!(result, "/https://example.com/new");
    }

    #[test]
    fn rewrite_location_relative_path() {
        let result = rewrite_location("/docs/new", "https://example.com/docs/old");
        assert_eq!(result, "/https://example.com/docs/new");
    }

    #[test]
    fn rewrite_location_relative_sibling() {
        let result = rewrite_location("sibling", "https://example.com/docs/old");
        assert_eq!(result, "/https://example.com/docs/sibling");
    }
}
