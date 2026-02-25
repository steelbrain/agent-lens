# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2025-02-25

### Added

- HTTP proxy that converts web pages to markdown
- HTML-to-markdown conversion via htmd with metadata extraction (title, description, language)
- HTML stripping of non-content elements (scripts, styles, nav, footer, ads, etc.)
- Headless Chrome rendering for JavaScript-heavy pages (automatic when Chrome is available)
- JSON passthrough — JSON responses are returned as-is
- YAML frontmatter with source URL, title, and description
- Configurable via CLI flags and environment variables (`AGENT_LENSE_` prefix)
- Response size limit (10 MB) to prevent abuse
- Hop-by-hop header stripping for correct proxying
- Docker support with multi-stage build
- GitHub Actions CI pipeline
