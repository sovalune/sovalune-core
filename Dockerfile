FROM rust:1.77-slim as builder

WORKDIR /app

# Install dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy workspace files
COPY Cargo.toml Cargo.lock ./

# Create empty src files for dependency caching
RUN mkdir -p crates/sovalune-api/src crates/sovalune-bus/src crates/sovalune-storage-client/src crates/sovalune-vector-memory/src crates/sovalune-self-learning/src crates/sovalune-instruction-sdk/src crates/sovalune-ml-runtime/src crates/sovalune-training/src crates/sovalune-storage-schema/src && \
    touch crates/sovalune-api/src/lib.rs crates/sovalune-bus/src/lib.rs crates/sovalune-storage-client/src/lib.rs crates/sovalune-vector-memory/src/lib.rs crates/sovalune-self-learning/src/lib.rs crates/sovalune-instruction-sdk/src/lib.rs crates/sovalune-ml-runtime/src/lib.rs crates/sovalune-training/src/lib.rs crates/sovalune-storage-schema/src/lib.rs

# Build dependencies
RUN cargo build --release || true

# Copy source code
COPY crates ./crates

# Build application
RUN cargo build --release --bin sovalune-server

# Runtime stage
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /app/target/release/sovalune-server /app/

EXPOSE 8090 8091

CMD ["./sovalune-server"]
