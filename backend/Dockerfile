FROM rust:1.96-slim-bookworm AS builder

RUN sed -i 's|deb.debian.org|mirrors.ustc.edu.cn|g' /etc/apt/sources.list.d/debian.sources 2>/dev/null; \
    sed -i 's|deb.debian.org|mirrors.ustc.edu.cn|g' /etc/apt/sources.list 2>/dev/null; \
    true

WORKDIR /app

RUN unset HTTP_PROXY HTTPS_PROXY http_proxy https_proxy && \
    apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY .cargo .cargo/
COPY Cargo.toml ./

ENV NO_PROXY=mirrors.ustc.edu.cn,localhost,127.0.0.1 \
    no_proxy=mirrors.ustc.edu.cn,localhost,127.0.0.1 \
    CARGO_HTTP_MULTIPLEXING=false

RUN mkdir -p src/bin && \
    echo "fn main() {}" > src/bin/main.rs && \
    echo "" > src/lib.rs && \
    unset HTTP_PROXY HTTPS_PROXY http_proxy https_proxy && \
    cargo build --release 2>&1 || true && \
    rm -rf src

COPY src ./src
COPY migrations ./migrations

RUN touch src/bin/main.rs src/lib.rs && \
    unset HTTP_PROXY HTTPS_PROXY http_proxy https_proxy && \
    cargo build --release && \
    mv target/release/moneyrobert /app/moneyrobert

FROM debian:bookworm-slim

RUN sed -i 's|deb.debian.org|mirrors.ustc.edu.cn|g' /etc/apt/sources.list.d/debian.sources 2>/dev/null; \
    sed -i 's|deb.debian.org|mirrors.ustc.edu.cn|g' /etc/apt/sources.list 2>/dev/null; \
    true

WORKDIR /app

RUN unset HTTP_PROXY HTTPS_PROXY http_proxy https_proxy && \
    apt-get update && apt-get install -y --no-install-recommends \
    libssl3 \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/moneyrobert /app/moneyrobert
COPY --from=builder /app/migrations /app/migrations

RUN useradd -m -u 1000 appuser && \
    chown -R appuser:appuser /app

USER appuser

EXPOSE 8001

HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:8001/api/v1/health || exit 1

ENTRYPOINT ["/app/moneyrobert"]
