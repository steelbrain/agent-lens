FROM rust:1.85-slim AS builder

WORKDIR /app

# Cache dependencies by building a dummy project first
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && \
    echo '//! stub' > src/lib.rs && \
    echo 'fn main() {}' > src/main.rs && \
    cargo build --release && \
    rm -rf src

# Build the real project
COPY src/ src/
RUN touch src/main.rs src/lib.rs && cargo build --release

# Runtime image
FROM debian:bookworm-slim

RUN apt-get update && \
    apt-get install -y --no-install-recommends \
        ca-certificates \
        chromium \
        fonts-liberation \
    && rm -rf /var/lib/apt/lists/*

RUN useradd -m -s /bin/bash agentlense

COPY --from=builder /app/target/release/agent-lense /usr/local/bin/agent-lense

USER agentlense

ENV AGENT_LENSE_CHROME_NO_SANDBOX=true

EXPOSE 3001

ENTRYPOINT ["agent-lense"]
