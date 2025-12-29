# Multi-stage build for Sorcery Server
FROM rust:1.83-alpine AS builder

# Install build dependencies
RUN apk add --no-cache musl-dev git

WORKDIR /build

# Copy only Cargo files first for better layer caching
COPY Cargo.toml Cargo.lock ./
COPY src ./src

# Build the application
RUN cargo build --release

# Runtime stage
FROM alpine:latest

# Install runtime dependencies
RUN apk add --no-cache ca-certificates

# Create app user
RUN addgroup -g 1000 sorcery && \
    adduser -D -u 1000 -G sorcery sorcery

WORKDIR /app

# Copy binary from builder
COPY --from=builder /build/target/release/sorcery-server /app/sorcery-server

# Copy tenant configurations
COPY tenants /app/tenants

# Change ownership
RUN chown -R sorcery:sorcery /app

USER sorcery

# Expose port
EXPOSE 8080

# Set environment variables
ENV PORT=8080
ENV TENANTS_DIR=/app/tenants
ENV RUST_LOG=sorcery_server=info
ENV BASE_DOMAIN=srcuri.com

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
  CMD wget --no-verbose --tries=1 --spider http://127.0.0.1:8080/health || exit 1

CMD ["/app/sorcery-server"]
