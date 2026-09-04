# Sovalune — Архитектура и организация репозиториев

## 1. Структура организации GitHub

Корневая организация: **Sovalune**.

```
Sovalune/
├── sovalune-core            # Rust backend (главный оркестратор), содержит остальное как git submodules
├── sovalune-storage-schema  # SQL миграции, схема БД, pgvector индексы (submodule)
├── sovalune-vector-memory   # Rust-крейт: логика векторной памяти + Context Weaver (submodule)
├── sovalune-self-learning   # Rust-крейт: цикл самообучения, оркестрация верификации (submodule)
├── sovalune-model-runtime   # C++/CUDA инференс-движок + биндинги (submodule)
├── sovalune-training        # Python: дообучение, датасеты, оценка (submodule)
├── sovalune-research        # Исследовательский код, ноутбуки, эксперименты (submodule)
├── sovalune-frontend        # Next.js приложение (submodule)
├── sovalune-instruction-sdk # Общая схема Instruction Tools (JSON Schema/Protobuf) (submodule)
├── sovalune-infra           # IaC, docker-compose для локальной разработки, CI конфиги (submodule)
├── sovalune-storage         # Fork Supabase Storage — S3-совместимое хранилище объектов (кастомизация под Sovalune)
├── sovalune-vector          # Fork pgvector — расширение для векторного поиска в PostgreSQL
└── sovalune-studio          # Fork Supabase Studio — дашборд для управления БД (кастомизация: фиолетовая палитра)
```

### Почему так, а не монорепо

Ты явно просил модульность на уровне GitHub при плоской структуре
локально. Решение:

- **На GitHub** — каждый домен это независимый репозиторий. Причины:
  research-код меняется хаотично и часто ломается — он не должен
  тянуть за собой CI всего core; model-runtime собирается через CMake
  и требует CUDA toolchain — незачем требовать это от фронтенд-разработчика;
  у каждого репозитория свой релизный цикл и своя команда контрибьюторов
  в перспективе.
- **Локально** — `sovalune-core` содержит все нужные для разработки
  сабмодули через `git submodule`, и `Cargo.toml` workspace объединяет
  Rust-крейты (`sovalune-vector-memory`, `sovalune-self-learning`) в
  единую сборку через path-зависимости на сабмодули. C++/Python и
  frontend остаются отдельными процессами (см. раздел 4 "Локальная
  разработка"), но поднимаются одной командой через `sovalune-infra`.

### Правило добавления нового сабмодуля

Прежде чем добавлять код в существующий сабмодуль, проверить: если
код можно тестировать/деплоить/версионировать независимо — это новый
сабмодуль, а не пакет внутри существующего.

## 2. Модули верхнего уровня и их ответственность

```
┌─────────────────────────────────────────────────────────────────┐
│                         Frontend (Next.js)                       │
│  Chat UI, Monaco (просмотр/редактирование памяти и кода),        │
│  дашборд памяти агента, визуализация Self-Learning Loop          │
└───────────────────────────┬───────────────────────────────────────┘
                             │ REST + WebSocket (см. 08-api-contracts.md)
┌───────────────────────────▼───────────────────────────────────────┐
│                    Sovalune Core (Rust / Axum)                    │
│                                                                     │
│  ┌──────────────┐  ┌──────────────────┐  ┌──────────────────┐     │
│  │ API Gateway  │  │  Vector Memory    │  │  Self-Learning    │     │
│  │ (Axum)       │◄─┤  (Context Weaver) │◄─┤  Orchestrator      │     │
│  └──────┬───────┘  └────────┬──────────┘  └─────────┬──────────┘     │
│         │                    │                        │              │
│         │           ┌────────▼────────┐               │              │
│         └──────────►│  NATS bus       │◄──────────────┘              │
│                      └────────┬────────┘                              │
└───────────────────────────────┼───────────────────────────────────────┘
                                 │ NATS subjects (task queue, events)
┌────────────────────────────────▼───────────────────────────────────┐
│                       Model Runtime (C++/CUDA)                     │
│   Inference engine, tool-calling loop, вызывает Instruction Tools   │
│   которые проксируются обратно в Sovalune Core через NATS/gRPC      │
└────────────────────────────────┬───────────────────────────────────┘
                                  │ periodic export / training jobs
┌────────────────────────────────▼───────────────────────────────────┐
│                    Training Pipeline (Python)                       │
│   Offline: fine-tuning, оценка на verified корпусе self-learning     │
└──────────────────────────────────────────────────────────────────┘

                     Sovalune Storage (Supabase Postgres + pgvector)
        Единая точка правды. Доступ — ТОЛЬКО через Sovalune Core (SQLx).
        Форк: sovalune-storage (объектное хранилище), sovalune-vector (pgvector),
        sovalune-studio (дашборд с фиолетовой палитрой).
```

## 3. Потоки данных верхнего уровня

### 3.1 Обычный запрос пользователя (инференс с памятью)

1. Frontend отправляет запрос через REST/WS → Sovalune Core.
2. Sovalune Core → Vector Memory: получить релевантный контекст по
   запросу (semantic search в pgvector + метаданные).
3. Context Weaver формирует финальный промпт с учётом токен-бюджета.
4. Sovalune Core публикует задачу в NATS (`inference.request.*`).
5. Model Runtime подписан на этот subject, забирает задачу, гоняет
   инференс. Если модель вызывает Instruction Tool — публикует запрос
   инструмента обратно в NATS (`tools.call.*`), Core обрабатывает и
   отвечает (`tools.result.*`).
6. Финальный ответ модели публикуется в `inference.response.*`, Core
   пересылает во Frontend по WebSocket, и сохраняет релевантные факты
   обратно в Vector Memory (если応 применимо — см. `03-vector-memory.md`).

### 3.2 Self-Learning Loop (кратко, детали в 04)

1. Model Runtime помечает ответ как потенциально ошибочный (либо это
   обнаруживается post-hoc через тест/фидбек).
2. Self-Learning Orchestrator (Rust) инициирует цикл: поиск источников
   → верификация → тренировочный прогон → тест → только после
   прохождения — попытка повторного решения реальной задачи.
3. Весь цикл логируется в Sovalune Storage как аудируемая цепочка
   (для прозрачности и возможности отката).

## 4. Локальная разработка

Через `sovalune-infra/docker-compose.yml` одной командой поднимаются:

- Sovalune Storage (Supabase Postgres + pgvector + Storage API, миграции из
  `sovalune-storage-schema`)
- NATS сервер (с JetStream)
- Sovalune Core (Rust, `cargo run` в watch-режиме)
- Model Runtime (C++ бинарник, локально либо CPU fallback для
  разработки без GPU — см. `05-model-runtime.md`, раздел "Dev mode")
- Frontend (`next dev`)
- Sovalune Studio (дашборд для управления БД, фиолетовая палитра)

Детали — в `09-roadmap.md`, Этап 0.

## 5. Границы ответственности (что НЕЛЬЗЯ делать)

- Frontend **никогда** не обращается к Storage или NATS напрямую.
- Model Runtime **никогда** не пишет в Storage напрямую — только через
  Instruction Tool вызовы, проксируемые Core.
- Python Training Pipeline читает данные для обучения только через
  экспортированные Core датасеты (файлы/S3-подобное хранилище), не
  через прямое подключение к продовой БД.
- Research submodule не имеет зависимостей "внутрь" от core — это
  улица с односторонним движением: research может использовать
  публичные крейты core, но core никогда не зависит от research.
