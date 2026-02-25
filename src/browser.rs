//! Browser rendering via headless Chrome (chromiumoxide).
//!
//! Launches a persistent headless Chrome instance at startup and renders
//! pages with full JavaScript execution to capture dynamically-generated content.

use std::time::Duration;

use chromiumoxide::browser::{Browser, BrowserConfig};
use futures::StreamExt;
use tokio::task::JoinHandle;

use crate::ProxyError;

/// JavaScript that polls `document.documentElement.innerHTML.length` until it
/// stops changing, then resolves.  This catches SPA hydration, lazy-loaded
/// data, and other post-load rendering.
///
/// - Polls every 300 ms
/// - Requires 3 consecutive stable polls (~900 ms of no DOM changes)
/// - Hard-caps at 10 s to avoid hanging on infinite-poll pages
const WAIT_FOR_RENDER_JS: &str = r"new Promise((resolve) => {
    let prev = 0;
    let same = 0;
    const iv = setInterval(() => {
        const cur = document.documentElement.innerHTML.length;
        if (cur === prev) { same++; } else { same = 0; }
        prev = cur;
        if (same >= 3) { clearInterval(iv); resolve(); }
    }, 300);
    setTimeout(() => { clearInterval(iv); resolve(); }, 10000);
})";

/// A headless Chrome renderer that produces fully-rendered HTML for any URL.
///
/// The browser is launched once and shared across all requests via `Arc`.
/// Each call to [`render`](Self::render) opens a new tab, navigates, captures
/// the DOM, and closes the tab.
pub struct BrowserRenderer {
    browser: Browser,
}

impl std::fmt::Debug for BrowserRenderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BrowserRenderer").finish_non_exhaustive()
    }
}

impl BrowserRenderer {
    /// Launches a headless Chrome browser and returns the renderer plus
    /// the CDP handler task that must remain alive for the browser session.
    ///
    /// # Errors
    ///
    /// Returns `ProxyError::Internal` if the browser cannot be started.
    pub async fn new(no_sandbox: bool) -> Result<(Self, JoinHandle<()>), ProxyError> {
        let mut builder = BrowserConfig::builder();
        if no_sandbox {
            builder = builder.no_sandbox().arg("--disable-dev-shm-usage");
        }
        let config = builder
            .new_headless_mode()
            .build()
            .map_err(|e| ProxyError::Internal(format!("failed to build browser config: {e}")))?;

        let (browser, mut handler) = Browser::launch(config)
            .await
            .map_err(|e| ProxyError::Internal(format!("failed to launch browser: {e}")))?;

        let handle = tokio::task::spawn(async move {
            while let Some(event) = handler.next().await {
                if let Err(e) = event {
                    tracing::debug!("CDP handler event error (non-fatal): {e}");
                }
            }
        });

        Ok((Self { browser }, handle))
    }

    /// Renders a URL in headless Chrome and returns the fully-rendered HTML.
    ///
    /// Opens a new tab, navigates to `url`, waits for the page to load,
    /// polls until the DOM stops changing (SPA hydration finishes), captures
    /// the rendered HTML, and closes the tab.
    ///
    /// # Errors
    ///
    /// Returns `ProxyError::Internal` if navigation, content retrieval, or
    /// the overall timeout fails.
    pub async fn render(&self, url: &str, timeout_secs: u64) -> Result<String, ProxyError> {
        let fut = async {
            let page = self
                .browser
                .new_page(url)
                .await
                .map_err(|e| ProxyError::Internal(format!("browser new_page failed: {e}")))?;

            // Wait for JS frameworks to hydrate / render.
            let _: () = page
                .evaluate(WAIT_FOR_RENDER_JS)
                .await
                .map_err(|e| {
                    tracing::debug!("DOM settle script failed (non-fatal): {e}");
                })
                .and_then(|v| v.into_value().map_err(|_| ()))
                .unwrap_or_default();

            let content = page
                .content()
                .await
                .map_err(|e| ProxyError::Internal(format!("browser content() failed: {e}")))?;

            // close() consumes the page — fire-and-forget errors on close
            let _ = page.close().await;

            Ok(content)
        };

        tokio::time::timeout(Duration::from_secs(timeout_secs), fut)
            .await
            .map_err(|_| ProxyError::Internal("browser render timed out".to_string()))?
    }
}
