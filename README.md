# agent-lense

An HTTP proxy that converts web pages to clean markdown — built for LLMs and agents.

## The problem

When LLMs fetch a web page, they get raw HTML. That HTML is full of noise: `<script>` tags, inline CSS, SVG blobs, tracking pixels, ad containers, navigation chrome, and deeply nested `<div>` soup. All of that eats tokens and adds zero useful information. Worse, a plain HTTP fetch only gets the initial HTML document — it doesn't execute JavaScript. That means single-page apps, client-rendered dashboards, and any page that hydrates content after load come back mostly empty.

Agent Lense sits between your LLM and the web. It fetches pages, renders JavaScript when needed using headless Chrome, strips away everything that isn't content, and returns clean markdown. Your agent gets the actual text, links, and structure of the page without burning context on markup that was never meant for it.

## Usage

```
cargo run -- --port 3001
```

Or with Docker:

```bash
docker compose up
```

Then visit `http://localhost:3001/https://example.com/` to get a markdown view of any website.

Original URLs are preserved as-is in the output. Agents can follow links by requesting them through the proxy the same way (e.g. `GET /https://example.com/page`).

## Configuration

All options can be set via CLI flags or environment variables.

| Variable | Flag | Default | Description |
|---|---|---|---|
| `AGENT_LENSE_PORT` | `--port` | `3001` | Port to listen on |
| `AGENT_LENSE_BIND` | `--bind` | `0.0.0.0` | Address to bind to |
| `AGENT_LENSE_TIMEOUT` | `--timeout` | `30` | Upstream request timeout in seconds |
| `AGENT_LENSE_CHROME_NO_SANDBOX` | `--chrome-no-sandbox` | `false` | Disable Chrome sandbox and `/dev/shm` usage (set automatically in Docker) |

## Development

```bash
./scripts/run.sh              # Run the server locally (debug mode)
./scripts/fmt.sh              # Auto-format code
./scripts/lint.sh             # Check formatting + clippy
./scripts/test.sh             # Run all tests (extra args forwarded to cargo test)
./scripts/check.sh            # Full local CI: fmt + clippy + test + release build
./scripts/docker-build.sh     # Build the Docker image locally
```

## License

MIT
