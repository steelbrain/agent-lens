//! HTML-to-markdown conversion pipeline.
//!
//! Extracts metadata, strips non-content elements, converts to markdown,
//! and prepends YAML frontmatter. Original URLs are preserved as-is.

use scraper::{Html, Selector};

/// Tags to remove entirely (tag and contents).
const STRIP_TAGS: &[&str] = &[
    "script", "noscript", "style", "link", "meta", "nav", "header", "footer", "input", "select",
    "textarea", "button", "form", "svg", "canvas",
];

/// Metadata extracted from an HTML page.
#[derive(Debug, Clone, Default)]
pub struct PageMetadata {
    /// The source URL of the page.
    pub source: String,
    /// The page title from `<title>`.
    pub title: Option<String>,
    /// The description from `<meta name="description">`.
    pub description: Option<String>,
    /// The language from `<html lang="...">`.
    pub language: Option<String>,
}

impl PageMetadata {
    /// Renders the metadata as a YAML frontmatter block.
    pub fn to_frontmatter(&self) -> String {
        let mut lines = vec!["---".to_string()];
        lines.push(format!("source: {}", self.source));
        if let Some(ref title) = self.title {
            lines.push(format!("title: {title}"));
        }
        if let Some(ref desc) = self.description {
            lines.push(format!("description: {desc}"));
        }
        if let Some(ref lang) = self.language {
            lines.push(format!("language: {lang}"));
        }
        lines.push("---".to_string());
        lines.join("\n")
    }
}

/// Extracts metadata from raw HTML before stripping.
///
/// Must be called before [`strip_html`] since `<title>` and `<meta>` are
/// removed during stripping.
pub fn extract_metadata(html: &str, source_url: &str) -> PageMetadata {
    let document = Html::parse_document(html);

    let title = Selector::parse("title")
        .ok()
        .and_then(|sel| document.select(&sel).next())
        .map(|el| el.text().collect::<String>().trim().to_string())
        .filter(|t| !t.is_empty());

    let description = Selector::parse(r#"meta[name="description"]"#)
        .ok()
        .and_then(|sel| document.select(&sel).next())
        .and_then(|el| el.value().attr("content").map(|s| s.trim().to_string()))
        .filter(|d| !d.is_empty());

    let language = Selector::parse("html")
        .ok()
        .and_then(|sel| document.select(&sel).next())
        .and_then(|el| el.value().attr("lang").map(|s| s.trim().to_string()))
        .filter(|l| !l.is_empty());

    PageMetadata { source: source_url.to_string(), title, description, language }
}

/// Removes non-content elements from HTML.
///
/// Strips tags like `script`, `style`, `nav`, `footer`, etc., elements with `aria-hidden="true"`,
/// and elements with inline `display:none` style.
pub fn strip_html(html: &str) -> String {
    let document = Html::parse_document(html);

    // Collect node IDs to remove
    let mut remove_ids = Vec::new();

    for tag in STRIP_TAGS {
        if let Ok(selector) = Selector::parse(tag) {
            for el in document.select(&selector) {
                remove_ids.push(el.id());
            }
        }
    }

    // aria-hidden="true"
    if let Ok(selector) = Selector::parse(r#"[aria-hidden="true"]"#) {
        for el in document.select(&selector) {
            remove_ids.push(el.id());
        }
    }

    // display:none in inline style
    if let Ok(selector) = Selector::parse("[style]") {
        for el in document.select(&selector) {
            if let Some(style) = el.value().attr("style") {
                let normalized = style.replace(' ', "").to_ascii_lowercase();
                if normalized.contains("display:none") {
                    remove_ids.push(el.id());
                }
            }
        }
    }

    // Build the output by traversing the tree and skipping removed subtrees
    let mut output = String::new();
    let mut skip_depth: Option<usize> = None;
    let mut depth: usize = 0;

    for edge in document.tree.root().traverse() {
        match edge {
            ego_tree::iter::Edge::Open(node) => {
                if skip_depth.is_some() {
                    depth += 1;
                    continue;
                }
                if remove_ids.contains(&node.id()) {
                    skip_depth = Some(depth);
                    depth += 1;
                    continue;
                }
                match node.value() {
                    scraper::Node::Text(text) => output.push_str(text.as_ref()),
                    scraper::Node::Element(el) => {
                        output.push('<');
                        output.push_str(&el.name.local);
                        for attr in &el.attrs {
                            output.push(' ');
                            output.push_str(&attr.0.local);
                            output.push_str("=\"");
                            // Simple attribute escaping
                            let escaped = attr
                                .1
                                .replace('&', "&amp;")
                                .replace('"', "&quot;")
                                .replace('<', "&lt;")
                                .replace('>', "&gt;");
                            output.push_str(&escaped);
                            output.push('"');
                        }
                        if is_void_element(&el.name.local) {
                            output.push_str(" />");
                        } else {
                            output.push('>');
                        }
                    }
                    scraper::Node::Doctype(dt) => {
                        output.push_str("<!DOCTYPE ");
                        output.push_str(&dt.name);
                        output.push('>');
                    }
                    _ => {}
                }
                depth += 1;
            }
            ego_tree::iter::Edge::Close(node) => {
                depth -= 1;
                if let Some(sd) = skip_depth {
                    if depth == sd {
                        skip_depth = None;
                    }
                    continue;
                }
                if let scraper::Node::Element(el) = node.value() {
                    if !is_void_element(&el.name.local) {
                        output.push_str("</");
                        output.push_str(&el.name.local);
                        output.push('>');
                    }
                }
            }
        }
    }

    output
}

/// Returns `true` for HTML void elements (self-closing tags).
fn is_void_element(tag: &str) -> bool {
    matches!(
        tag,
        "area"
            | "base"
            | "br"
            | "col"
            | "embed"
            | "hr"
            | "img"
            | "input"
            | "link"
            | "meta"
            | "param"
            | "source"
            | "track"
            | "wbr"
    )
}

/// Converts HTML to markdown with YAML frontmatter.
///
/// Pipeline:
/// 1. Extract metadata (title, description, language) — before stripping
/// 2. Strip non-content elements
/// 3. Convert to markdown via htmd
/// 4. Prepend YAML frontmatter
#[allow(clippy::missing_panics_doc)]
pub fn html_to_markdown(html: &str, source_url: &str) -> String {
    // 1. Extract metadata before stripping (title and meta get stripped)
    let metadata = extract_metadata(html, source_url);

    // 2. Strip non-content elements
    let stripped = strip_html(html);

    // 3. Convert to markdown
    let markdown = htmd::convert(&stripped).unwrap_or_default();

    // 4. Prepend frontmatter and usage hint
    let frontmatter = metadata.to_frontmatter();
    format!(
        "{frontmatter}\n\n\
         > All URLs in this document are original. \
         To fetch any URL as markdown, send a GET request to this same server \
         with the full URL as the path, e.g. `GET /https://example.com/page`.\n\n\
         ---\n\n\
         {}",
        markdown.trim()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontmatter_all_fields() {
        let meta = PageMetadata {
            source: "https://example.com/".to_string(),
            title: Some("Example".to_string()),
            description: Some("A test page".to_string()),
            language: Some("en".to_string()),
        };
        let fm = meta.to_frontmatter();
        assert!(fm.starts_with("---\n"));
        assert!(fm.ends_with("\n---"));
        assert!(fm.contains("source: https://example.com/"));
        assert!(fm.contains("title: Example"));
        assert!(fm.contains("description: A test page"));
        assert!(fm.contains("language: en"));
    }

    #[test]
    fn frontmatter_minimal() {
        let meta =
            PageMetadata { source: "https://example.com/".to_string(), ..Default::default() };
        let fm = meta.to_frontmatter();
        assert_eq!(fm, "---\nsource: https://example.com/\n---");
    }

    #[test]
    fn extract_title() {
        let html = "<html><head><title>Hello World</title></head><body></body></html>";
        let meta = extract_metadata(html, "https://example.com/");
        assert_eq!(meta.title, Some("Hello World".to_string()));
    }

    #[test]
    fn extract_description() {
        let html = r#"<html><head><meta name="description" content="A test page"></head><body></body></html>"#;
        let meta = extract_metadata(html, "https://example.com/");
        assert_eq!(meta.description, Some("A test page".to_string()));
    }

    #[test]
    fn extract_language() {
        let html = r#"<html lang="en"><head></head><body></body></html>"#;
        let meta = extract_metadata(html, "https://example.com/");
        assert_eq!(meta.language, Some("en".to_string()));
    }

    #[test]
    fn strip_script_tags() {
        let html = "<html><body><p>Hello</p><script>alert('xss')</script></body></html>";
        let stripped = strip_html(html);
        assert!(!stripped.contains("script"));
        assert!(!stripped.contains("alert"));
        assert!(stripped.contains("Hello"));
    }

    #[test]
    fn strip_style_tags() {
        let html = "<html><body><p>Hello</p><style>body{color:red}</style></body></html>";
        let stripped = strip_html(html);
        assert!(!stripped.contains("style"));
        assert!(!stripped.contains("color:red"));
    }

    #[test]
    fn strip_aria_hidden() {
        let html =
            r#"<html><body><p>Visible</p><div aria-hidden="true">Hidden</div></body></html>"#;
        let stripped = strip_html(html);
        assert!(stripped.contains("Visible"));
        assert!(!stripped.contains("Hidden"));
    }

    #[test]
    fn strip_display_none() {
        let html =
            r#"<html><body><p>Visible</p><div style="display: none">Hidden</div></body></html>"#;
        let stripped = strip_html(html);
        assert!(stripped.contains("Visible"));
        assert!(!stripped.contains("Hidden"));
    }

    #[test]
    fn full_pipeline() {
        let html = r#"<html lang="en">
            <head>
                <title>Test Page</title>
                <meta name="description" content="A test">
            </head>
            <body>
                <h1>Hello</h1>
                <p>World</p>
                <script>evil()</script>
            </body>
        </html>"#;

        let md = html_to_markdown(html, "https://example.com/");

        assert!(md.starts_with("---\n"));
        assert!(md.contains("source: https://example.com/"));
        assert!(md.contains("title: Test Page"));
        assert!(md.contains("# Hello"));
        assert!(md.contains("World"));
        assert!(!md.contains("evil"));
    }
}
