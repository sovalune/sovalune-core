# Sovalune Core

Rust backend orchestrator for the Sovalune AI Agent Platform.

## Overview

Sovalune Core is the central service that orchestrates:
- HTTP/WS API (Axum)
- Vector Memory (pgvector)
- NATS message bus
- Self-Learning Loop

## Structure

```
sovalune-core/
├── Cargo.toml                     # [workspace]
├── crates/
│   ├── sovalune-api/               # Axum HTTP/WS сервер
│   ├── sovalune-domain/            # доменные типы
│   ├── sovalune-storage-client/    # SQLx-обёртка над Sovalune Storage
│   ├── sovalune-bus/               # обёртка над NATS клиентом
│   └── sovalune-config/            # загрузка конфигурации
├── bin/
│   └── sovalune-server/            # main.rs
└── .gitmodules                     # git submodules
```

## Development

### Prerequisites

- Rust 1.75+
- PostgreSQL with pgvector (via Docker)
- NATS server (via Docker)

### Quick Start

```bash
# Start infrastructure
cd ../sovalune-infra
docker compose up -d

# Run server
cargo run --bin sovalune-server
```

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `SOVALUNE_STORAGE_URL` | `postgres://sovalune:sovalune_dev_password@localhost:5432/sovalune` | PostgreSQL connection URL |
| `SOVALUNE_NATS_URL` | `nats://localhost:4222` | NATS connection URL |
| `SOVALUNE_SERVER_HOST` | `0.0.0.0` | Server bind host |
| `SOVALUNE_SERVER_PORT` | `8080` | Server bind port |

## API Endpoints

| Method | Path | Description |
|--------|------|-------------|
| GET | `/health/live` | Liveness check |
| GET | `/health/ready` | Readiness check |
| GET | `/api/v1/projects` | List projects |
| GET | `/api/v1/sessions` | List sessions |
| GET | `/api/v1/memory` | List memory entries |
| GET | `/api/v1/learning-cycles` | List learning cycles |
