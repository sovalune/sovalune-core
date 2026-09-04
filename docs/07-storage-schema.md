# Sovalune Storage — схема данных

Репозиторий: `sovalune-storage-schema` (SQL миграции + `pgvector`
расширение). Реализация — Supabase Postgres (форк `sovalune-storage`),
включает Storage API для объектного хранилища. Детали провайдера не
являются частью публичного контракта этого документа — все
взаимодействия идут через `sqlx` в `sovalune-storage-client`.

Дополнительные репозитории:
- `sovalune-storage` — форк Supabase Storage (S3-совместимое объектное хранилище)
- `sovalune-vector` — форк pgvector (расширение для векторного поиска)
- `sovalune-studio` — форк Supabase Studio (дашборд с фиолетовой палитрой)

## 1. Расширения

```sql
CREATE EXTENSION IF NOT EXISTS vector;
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS pg_trgm; -- для гибридного поиска (полнотекст + вектор)
```

## 2. Основные таблицы (ядро, без второстепенных)

### 2.1 `projects`

```sql
CREATE TABLE projects (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    name TEXT NOT NULL,
    settings JSONB NOT NULL DEFAULT '{}', -- token budgets, decay params, thresholds — см. соотв. документы
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

### 2.2 `memory_entries`

```sql
CREATE TYPE memory_tier AS ENUM ('raw', 'consolidated', 'verified');

CREATE TABLE memory_entries (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    project_id UUID NOT NULL REFERENCES projects(id),
    tier memory_tier NOT NULL DEFAULT 'raw',
    content TEXT NOT NULL,
    embedding VECTOR(768),          -- размерность под выбранную embedding-модель, см. 03-vector-memory.md §5
    metadata JSONB NOT NULL DEFAULT '{}',
    confidence_score REAL NOT NULL DEFAULT 0.5,
    decay_score REAL NOT NULL DEFAULT 1.0,
    archived BOOLEAN NOT NULL DEFAULT false,
    source_entry_ids UUID[] DEFAULT NULL, -- для consolidated: из каких raw-записей собрано
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX memory_entries_embedding_idx
    ON memory_entries USING hnsw (embedding vector_cosine_ops);

CREATE INDEX memory_entries_project_tier_idx
    ON memory_entries (project_id, tier) WHERE NOT archived;
```

> Примечание: `VECTOR(768)` — placeholder под конкретную embedding-модель.
> Финальная размерность фиксируется на этапе реализации согласно
> `03-vector-memory.md` §5 и прописывается в первой миграции —
> изменение размерности после первых продовых данных требует полной
> реиндексации.

### 2.3 `learning_cycles`

```sql
CREATE TYPE learning_cycle_status AS ENUM (
    'detected', 'researching', 'verifying',
    'practicing', 'testing', 'applying',
    'completed', 'failed'
);

CREATE TABLE learning_cycles (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    project_id UUID NOT NULL REFERENCES projects(id),
    status learning_cycle_status NOT NULL DEFAULT 'detected',
    origin_task_id UUID NOT NULL,          -- ссылка на исходное сообщение/задачу
    failure_reason TEXT,                    -- заполняется при status = 'failed'
    retry_count INT NOT NULL DEFAULT 0,
    confidence_score REAL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

### 2.4 `learning_cycle_evidence`

```sql
CREATE TABLE learning_cycle_evidence (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    cycle_id UUID NOT NULL REFERENCES learning_cycles(id) ON DELETE CASCADE,
    source_type TEXT NOT NULL,   -- 'web' | 'docs' | 'memory'
    source_url TEXT,
    excerpt TEXT NOT NULL,
    trust_tier INT NOT NULL,     -- иерархия доверия, см. 04-self-learning.md §2.3
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

### 2.5 `learning_cycle_test_results`

```sql
CREATE TABLE learning_cycle_test_results (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    cycle_id UUID NOT NULL REFERENCES learning_cycles(id) ON DELETE CASCADE,
    stage TEXT NOT NULL,          -- 'practicing' | 'testing'
    passed BOOLEAN NOT NULL,
    detail JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

### 2.6 `sessions` / `messages`

```sql
CREATE TABLE sessions (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    project_id UUID NOT NULL REFERENCES projects(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TYPE message_role AS ENUM ('user', 'assistant', 'tool', 'system');

CREATE TABLE messages (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    session_id UUID NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    role message_role NOT NULL,
    content TEXT NOT NULL,
    tool_call JSONB,             -- если role = 'tool'
    request_id UUID NOT NULL,    -- для сквозной трассировки, см. 02-rust-core.md §6
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

### 2.7 `training_artifacts`

```sql
CREATE TABLE training_artifacts (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    cycle_id UUID REFERENCES learning_cycles(id),
    version INT NOT NULL,
    artifact_uri TEXT NOT NULL,   -- путь в объектном хранилище
    metrics JSONB NOT NULL DEFAULT '{}',
    promoted BOOLEAN NOT NULL DEFAULT false, -- см. auto_promote_adapter в 04-self-learning.md
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

## 3. Миграции — соглашение

- Каждая миграция — файл `NNNN_описание.sql` в `sovalune-storage-schema/migrations/`,
  применяется через `sqlx::migrate!` (см. `02-rust-core.md` §4).
- Миграции необратимы в проде (`down`-миграции пишутся только для
  локальной разработки) — откат состояния делается через forward-fix
  миграцию, не через `down`.
- Любое изменение `memory_entries.embedding` (размерность/тип индекса)
  — миграция, которая явно помечена как "требует реиндексации", с
  оценкой времени выполнения на объёме прод-данных до применения.

## 4. Гибридный поиск (опционально на первом этапе)

Для устойчивости поиска по точным терминам (имена функций, точные
версии библиотек), которые плохо ловятся чистым semantic search —
задел на гибридный поиск через `pg_trgm` + векторный индекс,
комбинируемые через weighted score в `VectorMemoryStore::search`. Не
обязательно к реализации в MVP, но схема (индекс `pg_trgm` на
`content`) закладывается заранее, чтобы не делать дорогую миграцию
позже.
