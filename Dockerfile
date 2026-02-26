# ── Build stage (Rust backend) ──
FROM rust:1.85-slim AS rust-builder
WORKDIR /app

RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/

RUN cargo build --release

# ── Build stage (React frontend) ──
FROM node:22-slim AS web-builder
WORKDIR /app/web

COPY web/package.json web/package-lock.json ./
RUN npm ci

COPY web/ ./
RUN npm run build

# ── Runtime stage ──
FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y ca-certificates curl && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=rust-builder /app/target/release/fast-photo ./
COPY --from=web-builder /app/web/dist ./web/dist
COPY models/ models/

# Create data directories
RUN mkdir -p data thumbnails

EXPOSE 8080

ENV RUST_LOG=fast_photo=info

CMD ["./fast-photo"]
