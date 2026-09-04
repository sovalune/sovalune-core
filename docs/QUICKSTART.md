# Sovalune — Быстрый старт

## Предварительные требования

- Docker Desktop
- Git
- Node.js 20+ (для frontend)
- Rust toolchain (для core)

## Запуск инфраструктуры

```bash
# Клонировать core (содержит все submodules)
git clone --recurse-submodules https://github.com/sovalune/sovalune-core.git
cd sovalune-core

# Запустить PostgreSQL + NATS + Studio
docker compose -f ../sovalune-infra/docker-compose.yml up -d

# Проверить статус
docker compose -f ../sovalune-infra/docker-compose.yml ps
```

## Сервисы по умолчанию

| Сервис | URL | Описание |
|--------|-----|----------|
| PostgreSQL | localhost:5432 | Supabase Postgres + pgvector |
| NATS | localhost:4222 | Message bus |
| NATS Monitor | localhost:8222 | Web UI для NATS |
| Studio | localhost:3000 | Supabase Dashboard |
| API Gateway | localhost:8000 | PostgREST |
| Storage API | localhost:5000 | Object Storage |
| Kong | localhost:8080 | API Gateway |

## Запуск Core сервера

```bash
# Установить зависимости
cargo build

# Запустить сервер
cargo run --bin sovalune-server
```

Сервер запустится на `http://localhost:8080`.

### Endpoints

- `GET /health/live` — Health check
- `GET /health/ready` — Readiness check
- `GET /api/v1/projects` — Список проектов
- `GET /api/v1/sessions` — Список сессий
- `GET /api/v1/memory` — Память
- `GET /api/v1/learning-cycles` — Циклы обучения
- `WS /ws/chat` — WebSocket для чата

## Запуск Frontend

```bash
cd ../sovalune-frontend
npm install
npm run dev
```

Frontend запустится на `http://localhost:3001`.

## Тестирование WebSocket чата

Откройте `http://localhost:3001` в браузере и начните чат. Сообщения будут отправляться через WebSocket на core сервер, который пересылает их в NATS для инференса.

## Переменные окружения

Создайте `.env` файл в корне core:

```env
SOVALUNE_STORAGE_URL=postgres://sovalune:sovalune_dev_password@localhost:5432/sovalune
SOVALUNE_NATS_URL=nats://localhost:4222
SOVALUNE_SERVER_HOST=0.0.0.0
SOVALUNE_SERVER_PORT=8080
```

## Структура проекта

```
sovalune/
├── sovalune-core/           # Rust backend (workspace)
│   ├── crates/
│   │   ├── sovalune-api/           # HTTP/WS handlers
│   │   ├── sovalune-bus/           # NATS client
│   │   ├── sovalune-config/        # Configuration
│   │   ├── sovalune-domain/        # Domain types
│   │   ├── sovalune-storage-client/# PostgreSQL client
│   │   ├── sovalune-vector-memory/ # Vector memory (submodule)
│   │   └── sovalune-self-learning/ # Self-learning (submodule)
│   └── bin/sovalune-server/        # Server binary
├── sovalune-frontend/       # Next.js frontend
├── sovalune-infra/          # Docker-compose, Kong, migrations
├── sovalune-storage-schema/ # SQL migrations
├── sovalune-studio/         # Supabase Studio (purple theme)
├── sovalune-storage/        # Supabase Storage
├── sovalune-vector/         # pgvector
├── sovalune-model-runtime/  # C++/CUDA inference
├── sovalune-training/       # Python training
├── sovalune-research/       # Research code
└── sovalune-instruction-sdk/# JSON Schema for tools
```
