# Sovalune Core — детальное ТЗ (Rust)

## 1. Cargo workspace структура

```
sovalune-core/
├── Cargo.toml                     # [workspace]
├── crates/
│   ├── sovalune-api/               # Axum HTTP/WS сервер — тонкий слой
│   ├── sovalune-domain/            # доменные типы, общие для всех крейтов
│   ├── sovalune-vector-memory/     # git submodule (см. 03-vector-memory.md)
│   ├── sovalune-self-learning/     # git submodule (см. 04-self-learning.md)
│   ├── sovalune-storage-client/    # SQLx-обёртка над Sovalune Storage
│   ├── sovalune-bus/               # обёртка над NATS клиентом, subjects как типы
│   ├── sovalune-instruction-sdk/   # git submodule — общая схема Instruction Tools
│   └── sovalune-config/            # загрузка конфигурации, env, секреты
└── bin/
    └── sovalune-server/            # main.rs, собирает всё воедино
```

Правило: **`sovalune-api` не содержит бизнес-логики.** Она только
парсит HTTP/WS, валидирует форму запроса и делегирует в domain-слой.
Это чтобы бизнес-логика была тестируема без поднятия HTTP-сервера.

## 2. Зависимости (базовый набор)

```toml
[workspace.dependencies]
tokio = { version = "1", features = ["full"] }
axum = { version = "0.8", features = ["ws", "macros"] }
sqlx = { version = "0.8", features = ["postgres", "runtime-tokio", "uuid", "chrono", "json"] }
async-nats = "0.38"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
uuid = { version = "1", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
thiserror = "2"
anyhow = "1"
```

Версии — ориентир на момент написания ТЗ; агент-реализатор обязан
проверить актуальные major-версии перед `cargo add`, но не должен
менять выбор библиотек (Axum/Tokio/SQLx/NATS) без явного запроса.

## 3. `sovalune-api` — контракт слоя API

### 3.1 Требования

- Axum роутер строится из отдельных модулей по доменам:
  `routes/chat.rs`, `routes/memory.rs`, `routes/tools.rs`,
  `routes/self_learning.rs`, `routes/health.rs`.
- Аутентификация — middleware, извлекающий и валидирующий JWT (провайдер
  auth не фиксируется в этом документе — см. `08-api-contracts.md`,
  раздел Auth).
- Все ошибки домена конвертируются в `IntoResponse` через единый
  `AppError` enum (`thiserror`), с консистентным JSON форматом ошибки:

```json
{
  "error": {
    "code": "MEMORY_NOT_FOUND",
    "message": "человекочитаемое сообщение",
    "request_id": "uuid"
  }
}
```

- WebSocket-эндпоинт для чата: `/ws/chat` — протокол сообщений описан
  в `08-api-contracts.md`, раздел 2.
- Обязательный middleware: `request_id` (генерируется, прокидывается
  в tracing span и в NATS сообщения для сквозной трассировки).

### 3.2 Health/Readiness

- `GET /health/live` — процесс жив.
- `GET /health/ready` — проверяет соединение с Storage и NATS,
  возвращает 503 если что-то недоступно. Обязателен для k8s/docker
  healthcheck.

## 4. `sovalune-storage-client` — контракт слоя доступа к данным

- Единственный крейт, который держит `sqlx::PgPool`.
- Все запросы — через `sqlx::query!`/`query_as!` (compile-time проверка
  типов против реальной схемы БД, схема — из `sovalune-storage-schema`
  submodule).
- Паттерн: репозитории по агрегатам (`MemoryRepository`,
  `SessionRepository`, `LearningCycleRepository`), никаких raw SQL за
  пределами этого крейта.
- Миграции запускаются через `sqlx::migrate!` при старте в dev-режиме;
  в проде — отдельным CI-шагом до деплоя новой версии.
- Пул соединений: конфигурируемый `max_connections`, обязательный
  `acquire_timeout`, иначе под нагрузкой сервис будет тихо зависать
  вместо явной ошибки.
- **Supabase:**底层使用 Supabase Postgres (форк `sovalune-storage`),
  включая Storage API для объектного хранилища и pgvector (форк `sovalune-vector`)
  для векторного поиска.

## 5. `sovalune-bus` — контракт слоя NATS

- Обёртка типизирует subjects через enum + `Display`, чтобы нельзя
  было опечататься в строке подписки:

```rust
pub enum Subject {
    InferenceRequest,
    InferenceResponse,
    ToolCall,
    ToolResult,
    LearningCycleStarted,
    LearningCycleStageCompleted,
    LearningCycleFinished,
}
```

- Полный список subjects, форматов payload и семантики
  delivery-guarantees — в `08-api-contracts.md`, раздел 3.
- Используется JetStream (не голый core NATS) для subjects, где нужна
  персистентность и at-least-once доставка (`inference.request.*`,
  `tools.call.*`, всё, что относится к Self-Learning Loop). Голый
  pub/sub допустим только для чисто эфемерных событий (например,
  стриминг токенов ответа во Frontend).

## 6. Логирование и наблюдаемость

- `tracing` + `tracing-subscriber` с JSON-выводом в проде,
  human-readable в dev (переключается через env `SOVALUNE_LOG_FORMAT`).
- Каждый запрос получает `request_id`, который прокидывается через все
  слои — API → domain → NATS payload → Model Runtime → обратно.
  Это критично для дебага Self-Learning Loop, где один цикл может
  растянуться на несколько инференс-вызовов.
- Метрики — Prometheus-совместимый `/metrics` эндпоинт (через
  `axum-prometheus` либо аналог), минимум: latency гистограммы по
  роутам, счётчик активных WS-соединений, счётчик активных
  self-learning циклов.

## 7. Конфигурация

- Единый `sovalune-config` крейт, конфиг собирается из (по приоритету
  возрастания): значения по умолчанию → файл `config.toml` →
  переменные окружения с префиксом `SOVALUNE_`.
- Секреты (DB URL, NATS creds, JWT secret) — только через env, никогда
  в файле конфигурации, который может попасть в git.

## 8. Тестирование

- Domain-логика — unit-тесты без внешних зависимостей.
- Интеграционные тесты — через `testcontainers` (поднимают реальный
  Postgres+pgvector и NATS в Docker на время теста), лежат в
  `tests/` каждого крейта.
- Обязательный CI-гейт: `cargo test --workspace` и `cargo clippy --
  -D warnings` перед мержем.
