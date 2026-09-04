FROM rust:1.82-slim AS builder

WORKDIR /app

RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./

# Copy all crate manifests first for dependency caching
COPY crates/sovalune-storage-schema/Cargo.toml crates/sovalune-storage-schema/Cargo.toml
COPY crates/sovalune-storage-schema/migrations crates/sovalune-storage-schema/migrations
COPY crates/sovalune-storage-schema/src crates/sovalune-storage-schema/src

COPY crates/sovalune-domain/Cargo.toml crates/sovalune-domain/Cargo.toml
RUN mkdir -p crates/sovalune-domain/src && touch crates/sovalune-domain/src/lib.rs

COPY crates/sovalune-config/Cargo.toml crates/sovalune-config/Cargo.toml
RUN mkdir -p crates/sovalune-config/src && touch crates/sovalune-config/src/lib.rs

COPY crates/sovalune-storage-client/Cargo.toml crates/sovalune-storage-client/Cargo.toml
RUN mkdir -p crates/sovalune-storage-client/src && touch crates/sovalune-storage-client/src/lib.rs

COPY crates/sovalune-bus/Cargo.toml crates/sovalune-bus/Cargo.toml
RUN mkdir -p crates/sovalune-bus/src && touch crates/sovalune-bus/src/lib.rs

COPY crates/sovalune-vector-memory/Cargo.toml crates/sovalune-vector-memory/Cargo.toml
RUN mkdir -p crates/sovalune-vector-memory/src && touch crates/sovalune-vector-memory/src/lib.rs

COPY crates/sovalune-self-learning/Cargo.toml crates/sovalune-self-learning/Cargo.toml
RUN mkdir -p crates/sovalune-self-learning/src && touch crates/sovalune-self-learning/src/lib.rs

COPY crates/sovalune-instruction-sdk/Cargo.toml crates/sovalune-instruction-sdk/Cargo.toml
RUN mkdir -p crates/sovalune-instruction-sdk/src && touch crates/sovalune-instruction-sdk/src/lib.rs

COPY crates/sovalune-model-runtime/Cargo.toml crates/sovalune-model-runtime/Cargo.toml
RUN mkdir -p crates/sovalune-model-runtime/src && touch crates/sovalune-model-runtime/src/lib.rs

COPY crates/sovalune-api/Cargo.toml crates/sovalune-api/Cargo.toml
RUN mkdir -p crates/sovalune-api/src && touch crates/sovalune-api/src/lib.rs

COPY bin/sovalune-server/Cargo.toml bin/sovalune-server/Cargo.toml
RUN mkdir -p bin/sovalune-server/src && touch bin/sovalune-server/src/main.rs

RUN cargo build --release || true

# Copy actual source code
COPY crates crates
COPY bin bin

RUN touch crates/sovalune-storage-schema/src/lib.rs \
    && touch crates/sovalune-domain/src/lib.rs \
    && touch crates/sovalune-config/src/lib.rs \
    && touch crates/sovalune-storage-client/src/lib.rs \
    && touch crates/sovalune-bus/src/lib.rs \
    && touch crates/sovalune-vector-memory/src/lib.rs \
    && touch crates/sovalune-self-learning/src/lib.rs \
    && touch crates/sovalune-instruction-sdk/src/lib.rs \
    && touch crates/sovalune-model-runtime/src/lib.rs \
    && touch crates/sovalune-api/src/lib.rs \
    && touch bin/sovalune-server/src/main.rs

RUN cargo build --release --bin sovalune-server

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    curl \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /app/target/release/sovalune-server /app/
COPY --from=builder /app/crates/sovalune-storage-schema/migrations /app/migrations/

EXPOSE 8090 8091

CMD ["./sovalune-server"]
