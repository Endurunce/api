# ── Stage 1: build ───────────────────────────────────────────────────────────
FROM rust:latest AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Cache dependencies separately from source
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo 'fn main() {}' > src/main.rs
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build --release --locked 2>/dev/null; \
    rm -f target/release/endurance

# Build real binary
COPY src ./src
COPY migrations ./migrations
COPY .sqlx ./.sqlx
RUN touch src/main.rs

ENV SQLX_OFFLINE=true

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build --release --locked && \
    cp target/release/endurance /app/endurance

# ── Stage 2: runtime ─────────────────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

RUN useradd -ms /bin/bash appuser
USER appuser
WORKDIR /app

COPY --from=builder /app/endurance ./endurance
COPY --from=builder /app/migrations ./migrations

EXPOSE 3000
ENTRYPOINT ["./endurance"]
