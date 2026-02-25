# agent-lense

An HTTP proxy that serves the web with HTML converted to markdown — built for LLMs and agents.

## Usage

```
cargo run -- --port 3001
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

## Docker

```
docker compose up
```

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
