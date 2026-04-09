//! MCP (Model Context Protocol) stdio server.
//!
//! Exposes a `fetch` tool that retrieves web pages and returns their content
//! as markdown, using the same conversion pipeline as the HTTP proxy.

use std::sync::Arc;

use axum::http::{HeaderMap, Method};
use rmcp::{
    ErrorData as McpError, ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolResult, Content, Implementation, ProtocolVersion, ServerCapabilities, ServerInfo,
    },
    schemars, tool, tool_handler, tool_router,
};

use crate::browser::BrowserRenderer;
use crate::html::fetch_upstream;
use crate::markdown::html_to_markdown;

/// Default character limit for responses (40 000 chars ≈ 10–13 K tokens).
const DEFAULT_LIMIT: u32 = 40_000;

/// Request parameters for the `fetch` tool.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct FetchRequest {
    /// The full URL to fetch (e.g. `https://example.com/page`).
    url: String,

    /// Character offset to start reading from. Defaults to 0.
    #[serde(default)]
    offset: Option<u32>,

    /// Maximum number of characters to return. Defaults to 40 000 (~10 K tokens).
    #[serde(default)]
    limit: Option<u32>,
}

/// MCP server exposing agent-lense tools via stdio.
#[derive(Clone)]
pub struct McpServer {
    client: reqwest::Client,
    browser: Option<Arc<BrowserRenderer>>,
    timeout: u64,
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl McpServer {
    /// Creates a new MCP server instance.
    pub fn new(
        client: reqwest::Client,
        browser: Option<Arc<BrowserRenderer>>,
        timeout: u64,
    ) -> Self {
        Self { client, browser, timeout, tool_router: Self::tool_router() }
    }

    /// Fetch a web page and return its content as markdown.
    #[tool(
        description = "Fetch a web page and return its content as markdown. HTML is cleaned (scripts, styles, nav removed) and converted to markdown with YAML frontmatter. Supports pagination via `offset` (default 0) and `limit` (default 40000 chars). The response includes total character count and whether more content is available, so you can paginate with subsequent calls."
    )]
    async fn fetch(
        &self,
        Parameters(request): Parameters<FetchRequest>,
    ) -> Result<CallToolResult, McpError> {
        let url = &request.url;
        let offset = request.offset.unwrap_or(0) as usize;
        let limit = request.limit.unwrap_or(DEFAULT_LIMIT) as usize;

        // Validate URL
        match url::Url::parse(url) {
            Ok(parsed) if parsed.scheme() == "http" || parsed.scheme() == "https" => {}
            _ => {
                return Ok(CallToolResult::error(vec![Content::text(
                    "Invalid URL: expected http:// or https:// URL",
                )]));
            }
        }

        // Fetch upstream
        let upstream =
            match fetch_upstream(&self.client, url, Method::GET, HeaderMap::new(), None).await {
                Ok(resp) => resp,
                Err(e) => {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "Failed to fetch {url}: {e}"
                    ))]));
                }
            };

        let content = if upstream.is_html() {
            let html_body = if let Some(ref browser) = self.browser {
                match browser.render(url, self.timeout).await {
                    Ok(rendered) => rendered.into_bytes(),
                    Err(e) => {
                        tracing::warn!("browser render failed, falling back to raw HTML: {e}");
                        upstream.body
                    }
                }
            } else {
                upstream.body
            };

            let html = String::from_utf8_lossy(&html_body);
            html_to_markdown(&html, url, true)
        } else {
            let body_len = upstream.body.len();
            match String::from_utf8(upstream.body) {
                Ok(text) => text,
                Err(_) => {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "Response is binary content ({body_len} bytes), not convertible to text"
                    ))]));
                }
            }
        };

        Ok(paginate_content(&content, offset, limit))
    }
}

#[tool_handler]
impl ServerHandler for McpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("agent-lense", env!("CARGO_PKG_VERSION")))
            .with_protocol_version(ProtocolVersion::V_2024_11_05)
            .with_instructions(
                "Agent Lense — fetch any web page as clean markdown. \
                 Use the `fetch` tool with a URL to retrieve and convert web pages."
                    .to_string(),
            )
    }
}

/// Slices content by character offset/limit and prepends a pagination header.
fn paginate_content(content: &str, offset: usize, limit: usize) -> CallToolResult {
    let total = content.len();

    // Clamp offset to content length
    let start = offset.min(total);

    // Find char-safe boundaries (don't split a multi-byte char)
    let start = content.char_indices().map(|(i, _)| i).find(|&i| i >= start).unwrap_or(total);

    let end_target = start.saturating_add(limit).min(total);
    let end = content.char_indices().map(|(i, _)| i).find(|&i| i >= end_target).unwrap_or(total);

    let slice = &content[start..end];
    let remaining = total.saturating_sub(end);

    let header = format!(
        "[showing {start}–{end} of {total} chars{}]\n\n",
        if remaining > 0 {
            format!(", {remaining} remaining — call again with offset={end} to continue")
        } else {
            String::new()
        }
    );

    CallToolResult::success(vec![Content::text(format!("{header}{slice}"))])
}

/// Runs the MCP stdio server.
///
/// Reads JSON-RPC messages from stdin, writes responses to stdout.
/// Blocks until the client disconnects.
///
/// # Errors
///
/// Returns an error if the MCP transport or service fails.
pub async fn run_mcp_server(
    client: reqwest::Client,
    browser: Option<Arc<BrowserRenderer>>,
    timeout: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let server = McpServer::new(client, browser, timeout);
    let transport = (tokio::io::stdin(), tokio::io::stdout());
    let service = server.serve(transport).await?;
    service.waiting().await?;
    Ok(())
}
