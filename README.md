# agent-lense

Give your AI agent clean markdown instead of raw HTML.

Agent Lense fetches web pages, renders JavaScript with headless Chrome, strips away scripts, styles, nav, and other noise, and returns clean markdown with metadata. Works as an [MCP](https://modelcontextprotocol.io/) tool or a standalone HTTP proxy.

## Setup

### Claude Code

```bash
claude mcp add --transport stdio agent-lens -- docker run -i --rm ghcr.io/steelbrain/agent-lens --mcp
```

### Codex

```bash
codex mcp add agent-lens -- docker run -i --rm ghcr.io/steelbrain/agent-lens --mcp
```

### Claude Desktop

Add to your `claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "agent-lense": {
      "command": "docker",
      "args": ["run", "-i", "--rm", "ghcr.io/steelbrain/agent-lens", "--mcp"]
    }
  }
}
```

### Codex config file

Add to `~/.codex/config.toml`:

```toml
[mcp_servers.agent-lens]
command = "docker"
args = ["run", "-i", "--rm", "ghcr.io/steelbrain/agent-lens", "--mcp"]
```

### From a local build

If you prefer building from source instead of Docker:

```bash
cargo build --release

# Then use the binary path directly
claude mcp add --transport stdio agent-lens -- ./target/release/agent-lense --mcp
codex mcp add agent-lens -- ./target/release/agent-lense --mcp
```

## The `fetch` tool

The MCP server exposes a single `fetch` tool:

| Parameter | Type | Default | Description |
|---|---|---|---|
| `url` | string | *(required)* | Full URL to fetch (`https://...` or `http://...`) |
| `offset` | number | `0` | Character offset to start reading from |
| `limit` | number | `40000` | Maximum characters to return (~10K tokens) |

Responses include a pagination header with total character count and remaining content, so your agent can page through large documents with follow-up calls.

## HTTP proxy mode

Agent Lense also runs as a standalone HTTP proxy for use outside MCP — any tool, script, or agent framework can call it over HTTP.

```bash
# Start the proxy
docker run -p 3001:3001 ghcr.io/steelbrain/agent-lens

# Fetch any page as markdown
curl http://localhost:3001/https://example.com/
```

Pass the target URL as the path:

```
GET /https://example.com/page
```

- **HTML** is cleaned and converted to markdown with YAML frontmatter (source URL, title, description, language)
- **Non-HTML** (JSON, images, etc.) is passed through unchanged
- **Redirects** are forwarded with `Location` rewritten to proxy-relative paths
- **Original URLs** in the output are preserved as-is — no link rewriting

## Configuration

All options can be set via CLI flags or environment variables.

| Variable | Flag | Default | Description |
|---|---|---|---|
| `AGENT_LENSE_MCP` | `--mcp` | `false` | Run as MCP stdio server instead of HTTP proxy |
| `AGENT_LENSE_PORT` | `--port` | `3001` | Port to listen on (HTTP proxy mode) |
| `AGENT_LENSE_BIND` | `--bind` | `0.0.0.0` | Address to bind to (HTTP proxy mode) |
| `AGENT_LENSE_TIMEOUT` | `--timeout` | `30` | Upstream request timeout (seconds) |
| `AGENT_LENSE_CHROME_NO_SANDBOX` | `--chrome-no-sandbox` | `false` | Disable Chrome sandbox (set automatically in Docker) |

## Docker

Pre-built images for `linux/amd64` and `linux/arm64` are available on [GitHub Container Registry](https://github.com/steelbrain/agent-lens/pkgs/container/agent-lens).

```bash
# MCP mode
docker run -i --rm ghcr.io/steelbrain/agent-lens --mcp

# HTTP proxy mode
docker run -p 3001:3001 ghcr.io/steelbrain/agent-lens

# With docker compose
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
