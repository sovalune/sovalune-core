# Sovalune Core

AI agent platform with long-term memory and self-learning capabilities.

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                     sovalune-core                           │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐     │
│  │   sovalune-  │  │  sovalune-   │  │  sovalune-   │     │
│  │     api      │  │    bus       │  │   storage    │     │
│  │  (REST/WS)   │  │  (NATS)      │  │   (SQLx)     │     │
│  └──────────────┘  └──────────────┘  └──────────────┘     │
│          │               │               │                 │
│          └───────────────┼───────────────┘                 │
│                          │                                 │
│  ┌───────────────────────┼───────────────────────┐        │
│  │                       │                       │        │
│  │  ┌──────────┐  ┌──────────┐  ┌──────────┐   │        │
│  │  │ vector-  │  │  self-   │  │instruct- │   │        │
│  │  │ memory   │  │ learning │  │   ion    │   │        │
│  │  │          │  │          │  │   sdk    │   │        │
│  │  └──────────┘  └──────────┘  └──────────┘   │        │
│  │                       │                       │        │
│  └───────────────────────┼───────────────────────┘        │
│                          │                                 │
└─────────────────────────────────────────────────────────────┘
```

## Crates

| Crate | Description |
|-------|-------------|
| `sovalune-api` | REST API and WebSocket handlers |
| `sovalune-bus` | NATS messaging with JetStream |
| `sovalune-storage-client` | PostgreSQL repositories |
| `sovalune-vector-memory` | Vector search and memory management |
| `sovalune-self-learning` | Learning cycle orchestration |
| `sovalune-instruction-sdk` | Tool definitions and execution |
| `sovalune-ml-runtime` | ML model inference |
| `sovalune-training` | Training pipeline |

## Quick Start

### Prerequisites

- Rust 1.75+
- PostgreSQL 15+ with pgvector
- NATS 2.10+

### Development

```bash
# Clone the repository
git clone https://github.com/sovalune/sovalune-core.git
cd sovalune-core

# Build
cargo build

# Run tests
cargo test

# Check compilation
cargo check
```

### Running

```bash
# Set environment variables
export SOVALUNE_STORAGE_URL=postgres://sovalune:sovalune_dev_password@localhost:5432/sovalune
export SOVALUNE_NATS_URL=nats://localhost:4222

# Run the server
cargo run --bin sovalune-server
```

### Docker

```bash
# Build the image
docker build -t sovalune-core .

# Run with Docker Compose (from sovalune-infra)
cd ../sovalune-infra
docker-compose up -d sovalune-core
```

## API Endpoints

### REST API (port 8090)

| Method | Path | Description |
|--------|------|-------------|
| GET | `/health/live` | Liveness check |
| GET | `/health/ready` | Readiness check |
| GET | `/api/v1/projects` | List projects |
| POST | `/api/v1/projects` | Create project |
| GET | `/api/v1/projects/:id` | Get project |
| GET | `/api/v1/sessions` | List sessions |
| POST | `/api/v1/sessions` | Create session |
| GET | `/api/v1/sessions/:id/messages` | Get messages |
| POST | `/api/v1/sessions/:id/messages` | Add message |
| GET | `/api/v1/memory` | List memory entries |
| GET | `/api/v1/memory/:id` | Get memory entry |
| PATCH | `/api/v1/memory/:id` | Update memory entry |
| DELETE | `/api/v1/memory/:id` | Delete memory entry |
| GET | `/api/v1/learning-cycles` | List learning cycles |
| GET | `/api/v1/learning-cycles/:id` | Get learning cycle |

### WebSocket (port 8091)

Connect to `ws://localhost:8091/ws/chat` for real-time chat.

**Client Messages:**
```json
{
  "type": "user_message",
  "session_id": "uuid",
  "project_id": "uuid",
  "content": "Hello!"
}
```

**Server Messages:**
```json
{
  "type": "token",
  "session_id": "uuid",
  "delta": "Hello"
}
```

## Configuration

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `SOVALUNE_STORAGE_URL` | `postgres://sovalune:sovalune_dev_password@localhost:5432/sovalune` | PostgreSQL URL |
| `SOVALUNE_NATS_URL` | `nats://localhost:4222` | NATS URL |
| `SOVALUNE_SERVER_HOST` | `0.0.0.0` | Server host |
| `SOVALUNE_SERVER_PORT` | `8090` | HTTP port |
| `SOVALUNE_WS_PORT` | `8091` | WebSocket port |

### Memory Tiers

- **raw**: Unprocessed observations
- **consolidated**: Merged and cleaned memories
- **verified**: High-confidence, validated memories

### Learning Cycle Stages

1. **detected**: Issue identified
2. **researching**: Gathering information
3. **verifying**: Validating findings
4. **practicing**: Applying knowledge
5. **testing**: Running tests
6. **applying**: Deploying changes
7. **completed**: Cycle finished

## Development

### Adding a New Crate

1. Create directory: `crates/sovalune-new-crate`
2. Add `Cargo.toml`
3. Add to workspace `Cargo.toml`
4. Run `cargo check`

### Code Style

- Use `rustfmt` for formatting
- Follow Rust API Guidelines
- Add doc comments for public items
- Use `anyhow` for error handling

### Testing

```bash
# Run all tests
cargo test

# Run specific crate tests
cargo test -p sovalune-storage-client

# Run with output
cargo test -- --nocapture
```

## License

MIT
