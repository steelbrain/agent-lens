# Agent Lense

HTTP proxy that converts web pages to markdown for LLM/agent consumption.

## Architecture

- `src/main.rs` — Entry point, CLI parsing, server startup
- `src/lib.rs` — Library root, re-exports all modules
- `src/server.rs` — Axum HTTP server, routing, request handling
- `src/html.rs` — Fetching remote HTML via reqwest
- `src/markdown.rs` — HTML-to-markdown conversion via htmd
- `src/browser.rs` — Headless Chrome rendering for JS-heavy pages (chromiumoxide)
- `tests/integration.rs` — Integration tests using axum-test and wiremock

## Commands

```bash
./scripts/fmt.sh             # Auto-format code
./scripts/lint.sh            # Check formatting + clippy
./scripts/test.sh            # Run all tests (forwards extra args to cargo test)
./scripts/check.sh           # Full local CI: fmt + clippy + test + release build
./scripts/run.sh             # Run server locally (debug, RUST_LOG=info)
./scripts/docker-build.sh    # Build Docker image
```

## Standards

- `unsafe` is forbidden (`unsafe_code = "forbid"`)
- `clippy::all` and `clippy::pedantic` are set to `deny` — no warnings allowed
- `clippy::nursery` and `clippy::cargo` are set to `warn`
- `missing_docs` is a warning — every public item should have a doc comment
- All code must pass `cargo fmt --check` before commit
- Integration tests use wiremock for deterministic HTTP mocking — never hit real URLs in tests
- Error types use `thiserror` — no `.unwrap()` in library code
- Original URLs are preserved as-is in markdown output — no link rewriting
