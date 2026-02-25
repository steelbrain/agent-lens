//! Integration tests for the proxy server.
//!
//! All tests use wiremock for deterministic HTTP mocking and axum-test
//! for in-process request handling. No real network requests are made.

use std::sync::Arc;

use axum_test::TestServer;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use agent_lense::html::build_client;
use agent_lense::server::build_router;
use agent_lense::{AppState, Config};

/// Creates a test server with a reqwest client for upstream requests.
fn test_server() -> TestServer {
    let client = build_client(5).expect("build client");
    let config = Arc::new(Config { port: 3001, bind: "127.0.0.1".to_string(), timeout: 5 });
    let state = AppState { client, config, browser: None, cdp_handle: None };
    let router = build_router(state);
    TestServer::new(router).expect("test server")
}

/// Helper to build the proxy path for a wiremock URL.
fn proxy_path(mock: &MockServer, upstream_path: &str) -> String {
    format!("/{}{upstream_path}", mock.uri())
}

#[tokio::test]
async fn root_returns_usage_hint() {
    let server = test_server();

    let response = server.get("/").await;

    response.assert_status_ok();
    response.assert_text_contains("agent-lense");
    response.assert_text_contains("Usage:");
}

#[tokio::test]
async fn html_converted_to_markdown_with_frontmatter() {
    let mock = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/page"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            "<html><head><title>Test Page</title></head>\
            <body><h1>Hello World</h1><p>Content here</p></body></html>",
            "text/html; charset=utf-8",
        ))
        .mount(&mock)
        .await;

    let server = test_server();
    let response = server.get(&proxy_path(&mock, "/page")).await;

    response.assert_status_ok();

    let body = response.text();
    assert!(body.contains("---"), "should have frontmatter");
    assert!(body.contains("source:"), "should have source in frontmatter");
    assert!(body.contains("title: Test Page"), "should have title");
    assert!(body.contains("# Hello World"), "should have h1");
    assert!(body.contains("Content here"), "should have content");

    let header = response.header("content-type");
    let content_type = header.to_str().expect("content-type header");
    assert_eq!(content_type, "text/markdown; charset=utf-8");
}

#[tokio::test]
async fn json_passed_through_unchanged() {
    let mock = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/data"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(r#"{"key": "value"}"#, "application/json"),
        )
        .mount(&mock)
        .await;

    let server = test_server();
    let response = server.get(&proxy_path(&mock, "/api/data")).await;

    response.assert_status_ok();
    response.assert_text(r#"{"key": "value"}"#);

    let header = response.header("content-type");
    let content_type = header.to_str().expect("content-type header");
    assert!(content_type.contains("application/json"));
}

#[tokio::test]
async fn original_urls_preserved_in_markdown_output() {
    let mock = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/page"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"<html><body>
            <a href="https://example.com/other">Link</a>
            <img src="image.png" alt="pic" />
            </body></html>"#,
            "text/html",
        ))
        .mount(&mock)
        .await;

    let server = test_server();
    let response = server.get(&proxy_path(&mock, "/page")).await;

    let body = response.text();
    assert!(body.contains("https://example.com/other"), "original URL should be preserved: {body}");
    assert!(!body.contains("/https://example.com/other"), "URL should not be rewritten: {body}");
}

#[tokio::test]
async fn redirect_forwarded_with_rewritten_location() {
    let mock = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/old"))
        .respond_with(
            ResponseTemplate::new(301).insert_header("location", "https://example.com/new-page"),
        )
        .mount(&mock)
        .await;

    let server = test_server();
    let response = server.get(&proxy_path(&mock, "/old")).await;

    response.assert_status(axum::http::StatusCode::MOVED_PERMANENTLY);

    let header = response.header("location");
    let location = header.to_str().expect("location header");
    assert_eq!(location, "/https://example.com/new-page");
}

#[tokio::test]
async fn relative_redirect_resolved_against_upstream() {
    let mock = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/docs/old"))
        .respond_with(ResponseTemplate::new(302).insert_header("location", "/docs/new-page"))
        .mount(&mock)
        .await;

    let server = test_server();
    let response = server.get(&proxy_path(&mock, "/docs/old")).await;

    response.assert_status(axum::http::StatusCode::FOUND);

    let header = response.header("location");
    let location = header.to_str().expect("location header");
    // Relative "/docs/new-page" should be resolved against the upstream origin
    let expected = format!("/{}/docs/new-page", mock.uri());
    assert_eq!(location, expected);
}

#[tokio::test]
async fn bad_target_url_returns_400() {
    let server = test_server();

    let response = server.get("/not-a-url").await;

    response.assert_status(axum::http::StatusCode::BAD_REQUEST);
    let body = response.text();
    assert!(body.contains("400"), "should indicate 400: {body}");
}

#[tokio::test]
async fn upstream_404_forwarded_as_markdown() {
    let mock = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/missing"))
        .respond_with(ResponseTemplate::new(404).set_body_raw(
            "<html><head><title>Not Found</title></head>\
            <body><h1>404 Not Found</h1></body></html>",
            "text/html",
        ))
        .mount(&mock)
        .await;

    let server = test_server();
    let response = server.get(&proxy_path(&mock, "/missing")).await;

    response.assert_status(axum::http::StatusCode::NOT_FOUND);

    let body = response.text();
    assert!(body.contains("Not Found"), "should have 404 content: {body}");

    let header = response.header("content-type");
    let content_type = header.to_str().expect("content-type header");
    assert_eq!(content_type, "text/markdown; charset=utf-8");
}

#[tokio::test]
async fn client_headers_forwarded_to_upstream() {
    let mock = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/auth"))
        .and(wiremock::matchers::header("authorization", "Bearer tok_123"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw("<html><body>OK</body></html>", "text/html"),
        )
        .mount(&mock)
        .await;

    let server = test_server();
    let response = server
        .get(&proxy_path(&mock, "/auth"))
        .add_header(
            axum::http::HeaderName::from_static("authorization"),
            axum::http::HeaderValue::from_static("Bearer tok_123"),
        )
        .await;

    response.assert_status_ok();
}

#[tokio::test]
async fn hop_by_hop_headers_stripped() {
    let mock = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/page"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(r#"{"ok": true}"#, "application/json")
                .insert_header("connection", "keep-alive")
                .insert_header("transfer-encoding", "chunked")
                .insert_header("x-custom", "preserved"),
        )
        .mount(&mock)
        .await;

    let server = test_server();
    let response = server.get(&proxy_path(&mock, "/page")).await;

    response.assert_status_ok();

    // Hop-by-hop headers should be stripped
    assert!(response.maybe_header("connection").is_none(), "connection header should be stripped");
    assert!(
        response.maybe_header("transfer-encoding").is_none(),
        "transfer-encoding header should be stripped"
    );

    // Custom headers should be preserved
    let header = response.header("x-custom");
    let custom = header.to_str().expect("x-custom header");
    assert_eq!(custom, "preserved");
}

#[tokio::test]
async fn non_idempotent_method_returns_405() {
    let mock = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/submit"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(r#"{"status": "ok"}"#, "application/json"),
        )
        .mount(&mock)
        .await;

    let server = test_server();
    let response = server.post(&proxy_path(&mock, "/submit")).await;

    response.assert_status(axum::http::StatusCode::METHOD_NOT_ALLOWED);

    let allow = response.header("allow");
    let allow_str = allow.to_str().expect("allow header");
    assert_eq!(allow_str, "GET, HEAD, OPTIONS");
}

#[tokio::test]
async fn script_tags_stripped_from_output() {
    let mock = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/page"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            "<html><body>\
            <p>Visible content</p>\
            <script>alert('xss')</script>\
            <style>.hidden { display: none }</style>\
            </body></html>",
            "text/html",
        ))
        .mount(&mock)
        .await;

    let server = test_server();
    let response = server.get(&proxy_path(&mock, "/page")).await;

    let body = response.text();
    assert!(body.contains("Visible content"), "content preserved");
    assert!(!body.contains("alert"), "script content stripped");
    assert!(!body.contains(".hidden"), "style content stripped");
}

#[tokio::test]
async fn oversized_response_returns_413() {
    let mock = MockServer::start().await;

    // Create a body larger than 10MB
    let large_body = "x".repeat(11 * 1024 * 1024);

    Mock::given(method("GET"))
        .and(path("/huge"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(large_body, "text/html"))
        .mount(&mock)
        .await;

    let server = test_server();
    let response = server.get(&proxy_path(&mock, "/huge")).await;

    response.assert_status(axum::http::StatusCode::PAYLOAD_TOO_LARGE);
}
