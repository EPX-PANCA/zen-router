# ============================================================
# Stage 1: Builder
# ============================================================
FROM rust:alpine AS builder

# Install build dependencies
RUN apk add --no-cache \
    musl-dev \
    pkgconfig \
    openssl-dev \
    openssl-libs-static \
    build-base \
    sqlite-dev \
    sqlite-static \
    git \
    perl

WORKDIR /build

# Copy dependency files first for better cache
COPY Cargo.toml Cargo.lock* build.rs* ./

# Create a dummy main.rs to compile dependencies
RUN mkdir -p src && echo "fn main() {}" > src/main.rs
RUN cargo build --release 2>&1 || true

# Remove the dummy build artifacts
RUN rm -f target/release/zen-router target/release/deps/zen_router*

# Copy the actual source and git history for commit info
COPY src ./src
COPY static ./static
COPY build.rs ./
COPY .git ./.git

# Build the actual binary
ENV SQLX_OFFLINE=true
RUN cargo build --release

# ============================================================
# Stage 2: Runtime
# ============================================================
FROM alpine:3.20

RUN apk add --no-cache \
    ca-certificates \
    libgcc \
    sqlite-libs \
    tini

# Create non-root user
RUN adduser -D -u 1000 zenrouter

# Create data directory
RUN mkdir -p /app/data && chown -R zenrouter:zenrouter /app

WORKDIR /app

COPY --from=builder /build/target/release/zen-router /app/zen-router
COPY --from=builder /build/static /app/static

RUN chmod +x /app/zen-router && chown -R zenrouter:zenrouter /app

USER zenrouter

EXPOSE 20128

HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD wget -q -O- http://127.0.0.1:20128/health || exit 1

ENTRYPOINT ["/sbin/tini", "--"]
CMD ["/app/zen-router"]
