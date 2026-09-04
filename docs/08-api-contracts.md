# API контракты между модулями

Этот документ — единственный источник истины по форматам данных на
границах модулей. Реализация любого модуля не должна изобретать
формат самостоятельно.

**Примечание по Supabase:** Sovalune Storage реализован на базе Supabase
Postgres + pgvector (форки: `sovalune-storage`, `sovalune-vector`,
`sovalune-studio`). Все контракты ниже остаются неизменными — Supabase
используется как провайдер БД и объектного хранилища, не меняя
публичный API модулей.

## 1. REST API (Frontend ↔ Sovalune Core)

Базовый путь: `/api/v1`.

### 1.1 Аутентификация

- `Authorization: Bearer <jwt>` на всех эндпоинтах кроме `/health/*`.
- Провайдер выдачи JWT — architectural decision на этапе реализации
  (может быть собственный, может быть внешний auth-провайдер);
  контракт фиксирует только формат заголовка и обязательные claims:
  `sub` (user id), `project_ids` (список доступных проектов), `exp`.

### 1.2 Эндпоинты (ядро)

```
GET    /api/v1/projects
POST   /api/v1/projects
GET    /api/v1/projects/{id}

GET    /api/v1/sessions?project_id=
POST   /api/v1/sessions
GET    /api/v1/sessions/{id}/messages

GET    /api/v1/memory?project_id=&tier=&q=
GET    /api/v1/memory/{id}
PATCH  /api/v1/memory/{id}         # ручная ре-курация: archived, confidence_score
DELETE /api/v1/memory/{id}

GET    /api/v1/learning-cycles?project_id=&status=
GET    /api/v1/learning-cycles/{id}
GET    /api/v1/learning-cycles/{id}/evidence
GET    /api/v1/learning-cycles/{id}/test-results

GET    /api/v1/health/live
GET    /api/v1/health/ready
GET    /api/v1/metrics
```

Формат ошибок — единый, описан в `02-rust-core.md` §3.1.

### 1.3 Пагинация

Все списочные эндпоинты — cursor-based:
`?cursor=<opaque>&limit=<int, default 20, max 100>`, ответ содержит
`next_cursor: string | null`.

## 2. WebSocket протокол (Frontend ↔ Sovalune Core), `/ws/chat`

Сообщения — JSON, каждое с полем `type`.

### 2.1 Клиент → сервер

```json
{ "type": "user_message", "session_id": "uuid", "content": "string" }
{ "type": "stop_generation", "session_id": "uuid" }
```

### 2.2 Сервер → клиент

```json
{ "type": "token", "session_id": "uuid", "delta": "string" }
{ "type": "tool_call_started", "session_id": "uuid", "tool": "memory_search", "arguments": {} }
{ "type": "tool_call_finished", "session_id": "uuid", "tool": "memory_search", "result_summary": "string" }
{ "type": "message_complete", "session_id": "uuid", "message_id": "uuid" }
{ "type": "learning_cycle_update", "cycle_id": "uuid", "status": "researching", "detail": {} }
{ "type": "error", "code": "string", "message": "string" }
```

Правило: любое событие, влияющее на видимый пользователю прогресс
(включая Self-Learning Loop), обязано быть отправлено во Frontend —
никакой "тихой" фоновой работы, о которой пользователь не может узнать
в реальном времени.

## 3. NATS subjects (Sovalune Core ↔ Model Runtime ↔ Training Pipeline)

Соглашение об именовании: `<domain>.<action>.<qualifier?>`.
JetStream используется для всех subjects ниже, кроме явно помеченных
"ephemeral".

### 3.1 Инференс

```
Subject: inference.request.{project_id}
Payload:
{
  "request_id": "uuid",
  "session_id": "uuid",
  "prompt_context": {
     "system": "string",
     "memory_sections": [ { "tier": "verified", "content": "string" } ],
     "history": [ { "role": "user|assistant", "content": "string" } ]
  },
  "generation_config": { "max_tokens": 1024, "temperature": 0.7 }
}
```

```
Subject: inference.response.{request_id}   [ephemeral, для стриминга]
Payload: { "delta": "string" } | { "done": true, "message_id": "uuid" }
```

### 3.2 Instruction Tools

```
Subject: tools.call.{tool_name}
Payload:
{
  "request_id": "uuid",
  "cycle_id": "uuid | null",
  "tool": "memory_search",
  "arguments": { ... }   // схема — в sovalune-instruction-sdk
}
```

```
Subject: tools.result.{request_id}
Payload:
{
  "request_id": "uuid",
  "ok": true,
  "result": { ... },      // либо
  "error": "string"
}
```

### 3.3 Self-Learning Loop

```
Subject: learning.cycle.stage_changed
Payload:
{
  "cycle_id": "uuid",
  "project_id": "uuid",
  "from_status": "researching",
  "to_status": "verifying",
  "detail": { ... }
}
```

```
Subject: learning.cycle.started / learning.cycle.finished
Payload: { "cycle_id": "uuid", "project_id": "uuid" }
```

### 3.4 Training

```
Subject: training.job.request
Payload:
{
  "job_id": "uuid",
  "cycle_id": "uuid | null",
  "job_type": "adapter_tune | eval_only",
  "dataset_uri": "string",
  "base_artifact_uri": "string | null",
  "limits": { "max_steps": 200, "timeout_seconds": 3600 }
}
```

```
Subject: training.job.result
Payload:
{
  "job_id": "uuid",
  "ok": true,
  "artifact_uri": "string | null",
  "metrics": { "eval_pass_rate": 0.94 },
  "error": "string | null"
}
```

### 3.5 Память — фоновые задачи

```
Subject: memory.decay.tick   [публикуется по расписанию из Core]
Payload: { "project_id": "uuid | null" }   // null = все проекты
```

## 4. Instruction Tools — схема (сводно, полная версия в
`sovalune-instruction-sdk`)

Каждый инструмент описывается JSON Schema для `arguments` и для
`result`, используется:
- Model Runtime — для constrained decoding (раздел `05-model-runtime.md` §2.2).
- Rust Core — для валидации входящих `tools.call.*` до исполнения.
- Frontend — для человекочитаемого рендера вызова инструмента в UI.

Схемы версионируются (`schema_version` поле), обратная несовместимость
запрещена без мажорного версионирования всего SDK-сабмодуля.

## 5. Общее правило консистентности

Любое новое поле в любом из контрактов выше добавляется только как
опциональное с default-значением на принимающей стороне — чтобы разные
версии модулей (Core обновился, Model Runtime ещё старой версии) не
ломали друг друга во время rolling deploy.
