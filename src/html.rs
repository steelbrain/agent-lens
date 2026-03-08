//! Fetching remote HTML pages via reqwest.
//!
//! Handles building the HTTP client, forwarding requests to upstream servers,
//! and enforcing response size limits.

use std::time::Duration;

use axum::http::{HeaderMap, Method};
use reqwest::Client;

use crate::{MAX_RESPONSE_SIZE, ProxyError};

/// Response received from an upstream server.
#[derive(Debug)]
pub struct UpstreamResponse {
    /// HTTP status code from upstream.
    pub status: u16,
    /// Response headers from upstream.
    pub headers: HeaderMap,
    /// Response body bytes.
    pub body: Vec<u8>,
    /// Content-Type header value, if present.
    pub content_type: Option<String>,
}

impl UpstreamResponse {
    /// Returns `true` if the response content type indicates HTML.
    pub fn is_html(&self) -> bool {
        self.content_type.as_ref().is_some_and(|ct| ct.contains("text/html"))
    }
}

/// Builds a reqwest HTTP client with the given timeout.
///
/// The client does not follow redirects (the proxy forwards them to the client)
/// and uses rustls for TLS.
///
/// # Errors
///
/// Returns `ProxyError::Internal` if the client cannot be built.
pub fn build_client(timeout_secs: u64) -> Result<Client, ProxyError> {
    Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(timeout_secs))
        .connect_timeout(Duration::from_secs(5))
        .pool_max_idle_per_host(10)
        .pool_idle_timeout(Duration::from_secs(90))
        .build()
        .map_err(|e| ProxyError::Internal(format!("failed to build HTTP client: {e}")))
}

/// Fetches a URL from the upstream server, forwarding the method, headers, and body.
///
/// Enforces a response size limit of [`MAX_RESPONSE_SIZE`] bytes by reading
/// the body in chunks.
///
/// # Errors
///
/// Returns `ProxyError::Timeout` if the upstream request times out,
/// `ProxyError::UpstreamUnreachable` if the upstream cannot be reached,
/// or `ProxyError::ResponseTooLarge` if the response exceeds the size limit.
pub async fn fetch_upstream(
    client: &Client,
    url: &str,
    method: Method,
    headers: HeaderMap,
    body: Option<Vec<u8>>,
) -> Result<UpstreamResponse, ProxyError> {
    let reqwest_method = reqwest::Method::from_bytes(method.as_str().as_bytes())
        .map_err(|e| ProxyError::Internal(format!("invalid method: {e}")))?;

    let mut request = client.request(reqwest_method, url);

    // Forward headers, skipping Host (reqwest sets it from the URL)
    // and Accept-Encoding (reqwest handles compression negotiation)
    for (name, value) in &headers {
        if name.as_str().eq_ignore_ascii_case("host")
            || name.as_str().eq_ignore_ascii_case("accept-encoding")
        {
            continue;
        }
        request = request.header(name.clone(), value.clone());
    }

    if let Some(b) = body {
        request = request.body(b);
    }

    let response = request.send().await.map_err(|e| classify_reqwest_error(&e))?;

    let status = response.status().as_u16();
    let response_headers = convert_headers(response.headers());
    let content_type =
        response.headers().get("content-type").and_then(|v| v.to_str().ok()).map(String::from);

    // Read body with size limit enforcement
    let body = read_body_with_limit(response).await?;

    Ok(UpstreamResponse { status, headers: response_headers, body, content_type })
}

/// Reads the response body, enforcing the maximum size limit.
async fn read_body_with_limit(response: reqwest::Response) -> Result<Vec<u8>, ProxyError> {
    // Check Content-Length header first for early rejection
    if let Some(len) = response.content_length()
        && len > MAX_RESPONSE_SIZE
    {
        return Err(ProxyError::ResponseTooLarge);
    }

    let mut body = Vec::new();
    let mut stream = response;

    while let Some(chunk) = stream.chunk().await.map_err(|e| classify_reqwest_error(&e))? {
        body.extend_from_slice(&chunk);
        if body.len() as u64 > MAX_RESPONSE_SIZE {
            return Err(ProxyError::ResponseTooLarge);
        }
    }

    Ok(body)
}

/// Converts reqwest headers to axum `HeaderMap`.
fn convert_headers(headers: &reqwest::header::HeaderMap) -> HeaderMap {
    let mut map = HeaderMap::new();
    for (name, value) in headers {
        if let Ok(name) = axum::http::HeaderName::from_bytes(name.as_str().as_bytes())
            && let Ok(value) = axum::http::HeaderValue::from_bytes(value.as_bytes())
        {
            map.insert(name, value);
        }
    }
    map
}

/// Maps a reqwest error to the appropriate `ProxyError` variant.
fn classify_reqwest_error(err: &reqwest::Error) -> ProxyError {
    if err.is_timeout() {
        ProxyError::Timeout
    } else {
        ProxyError::UpstreamUnreachable(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_html_true_for_text_html() {
        let resp = UpstreamResponse {
            status: 200,
            headers: HeaderMap::new(),
            body: vec![],
            content_type: Some("text/html; charset=utf-8".to_string()),
        };
        assert!(resp.is_html());
    }

    #[test]
    fn is_html_false_for_json() {
        let resp = UpstreamResponse {
            status: 200,
            headers: HeaderMap::new(),
            body: vec![],
            content_type: Some("application/json".to_string()),
        };
        assert!(!resp.is_html());
    }

    #[test]
    fn is_html_false_for_none() {
        let resp = UpstreamResponse {
            status: 200,
            headers: HeaderMap::new(),
            body: vec![],
            content_type: None,
        };
        assert!(!resp.is_html());
    }
}
