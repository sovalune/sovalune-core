# Roadmap реализации

Этот документ задаёт порядок, в котором имеет смысл реализовывать
систему — не бизнес-приоритеты, а инженерная зависимость: каждый
следующий этап опирается на готовность предыдущего. Каждый этап имеет
явный критерий готовности ("Definition of Done"), чтобы можно было
однозначно понять, когда переходить дальше — это особенно важно, если
реализацией занимается ИИ-агент, которому нужен чёткий стоп-критерий.

## Обновления архитектуры

### Supabase интеграция
Вместо голого PostgreSQL используется стек Supabase:
- **sovalune-storage** — форк Supabase Storage (S3-совместимое объектное хранилище)
- **sovalune-vector** — форк pgvector (расширение для векторного поиска в PostgreSQL)
- **sovalune-studio** — форк Supabase Studio (дашборд для управления БД, кастомизация: фиолетовая палитра)

### CI/CD
Каждый репозиторий имеет GitHub Actions workflow:
- **Lint & Test** — при пуше в feature-ветки и PR
- **Build & Deploy** — при мерже в main (для deployable компонентов)
- **Schema Validation** — для миграций БД

### Docker
`sovalune-infra` содержит `docker-compose.yml` с:
- Supabase Postgres + pgvector (без images/builds, чистые образы)
- NATS (с JetStream)
- Sovalune Core
- Model Runtime (CPU fallback dev-mode)
- Frontend
- Sovalune Studio (фиолетовая тема)

## Этап 0 — Скелет инфраструктуры

**Цель:** можно поднять всё окружение локально одной командой, даже
без реальной бизнес-логики.

- [x] Создать организацию/репозитории на GitHub согласно
      `01-architecture.md` §1.
- [x] Форкнуть Supabase repos: `sovalune-storage`, `sovalune-vector`, `sovalune-studio`.
- [ ] `sovalune-infra`: `docker-compose.yml` — Supabase Postgres+pgvector, NATS
      (JetStream включён), заглушки для остальных сервисов.
- [ ] `sovalune-storage-schema`: первая миграция — таблицы `projects`,
      `sessions`, `messages` (без memory/learning — они появятся на
      Этапе 2-3).
- [ ] `sovalune-core`: workspace скелет, `sovalune-api` с
      `/health/live`, `/health/ready`, поднимается и подключается к
      Postgres/NATS из docker-compose.
- [ ] Настроить CI/CD workflows для всех репозиториев.

**DoD:** `docker-compose up` + `cargo run` → `/health/ready` отвечает
200.

## Этап 1 — Базовый чат без памяти

**Цель:** пользователь может написать сообщение и получить ответ от
модели, без Vector Memory и Self-Learning — просто голый инференс.

- [ ] `sovalune-model-runtime`: минимальный инференс-движок, CPU
      dev-mode (см. `05-model-runtime.md` §2.5), без tool-calling.
- [ ] `sovalune-bus`: базовые subjects `inference.request.*`,
      `inference.response.*`.
- [ ] `sovalune-api`: WS `/ws/chat`, минимальный протокол (`user_message`,
      `token`, `message_complete`).
- [ ] `sovalune-frontend`: базовый чат-интерфейс (Vercel AI SDK
      стриминг), без Monaco/memory dashboard.

**DoD:** сквозной путь — сообщение из браузера доходит до модели и
ответ стримится обратно, сохраняется в `messages`.

## Этап 2 — Vector Memory + Context Weaver

**Цель:** модель отвечает с учётом сохранённой памяти.

- [ ] `sovalune-storage-schema`: миграция `memory_entries` (см.
      `07-storage-schema.md` §2.2).
- [ ] Выбор и интеграция embedding-модели (`03-vector-memory.md` §5).
- [ ] `sovalune-vector-memory`: `insert_raw`, `search`, без
      consolidate/promote пока.
- [ ] Context Weaver: базовая версия — semantic search + жёсткое
      усечение по токен-бюджету (упрощённая версия шага 4 из
      `03-vector-memory.md` §4.2, без summarization — это можно
      добавить позже).
- [ ] Instruction Tool `memory_search` доступен модели (требует
      tool-calling в Model Runtime — минимальная grammar-guided
      реализация).
- [ ] Frontend: простой memory dashboard (список, без evidence chain).

**DoD:** можно сохранить факт в память, задать модели вопрос, и она
использует этот факт в ответе (проверяется вручную на 3-5 сценариях).

## Этап 3 — Self-Learning Loop (MVP)

**Цель:** цикл проходит все стадии на упрощённых условиях, без
реального fine-tuning на этапе `PRACTICING`.

- [ ] `sovalune-storage-schema`: миграции `learning_cycles`,
      `learning_cycle_evidence`, `learning_cycle_test_results`.
- [ ] `sovalune-self-learning`: state machine стадий
      (`04-self-learning.md` §2), с `PRACTICING` реализованным как
      few-shot промпт-инъекция (без реального обучения весов — Python
      Training Pipeline подключается позже).
- [ ] Instruction Tools `web_search`/`docs_search` для стадии
      `RESEARCHING`.
- [ ] Frontend: визуализация стадий цикла (`06-frontend.md` §5).

**DoD:** искусственно вызванная ошибка модели (например, дать заведомо
неверный факт) запускает цикл, который проходит все стадии до
`COMPLETED` и итоговый ответ пользователю оказывается исправленным.

## Этап 4 — Consolidation, Decay, Verified Promotion

**Цель:** память перестаёт быть плоским логом и становится
многоуровневой системой знаний.

- [ ] `consolidate()` — объединение дублирующихся/связанных raw-записей.
- [ ] `promote_to_verified()` — интеграция с завершением Self-Learning
      Loop (`APPLYING` стадия промотирует знание, см.
      `04-self-learning.md` §2.6).
- [ ] `decay_tick()` фоновая задача + scheduler в Core.
- [ ] Context Weaver: добавить summarization для записей, не
      влезающих целиком в бюджет (`03-vector-memory.md` §4.2, шаг 4).

**DoD:** после серии из ~50 синтетических взаимодействий видно, что
память не растёт неограниченно плоским списком — есть консолидация и
архивация нерелевантного.

## Этап 5 — Реальное дообучение (Training Pipeline)

**Цель:** стадия `PRACTICING` может реально запускать
adapter/LoRA-тюнинг вместо few-shot заглушки.

- [ ] `sovalune-training`: `adapter_tune.py`, `nats_worker.py`.
- [ ] Контракт `training.job.request/result` (`08-api-contracts.md`
      §3.4) реализован с обеих сторон.
- [ ] `training_artifacts` таблица, версионирование, `promoted` флаг с
      ручным подтверждением (`auto_promote_adapter = false` по
      умолчанию).
- [ ] Model Runtime: возможность горячей подгрузки/переключения
      адаптера без полного рестарта процесса (если бэкенд инференса
      это поддерживает — иначе fallback на graceful restart).

**DoD:** цикл Self-Learning реально производит новый адаптер, он
проходит `TESTING`, и после ручного review виден рост качества на
регрессионном наборе.

## Этап 6 — Research submodule и полировка

- [ ] `sovalune-research`: подключается как независимый сабмодуль,
      используется для экспериментов с альтернативными
      embedding-моделями, инференс-бэкендами, стратегиями ранжирования
      в Context Weaver — без влияния на прод-код core.
- [ ] Метрики/observability (`02-rust-core.md` §6) доведены до полноты.
- [ ] Нагрузочное тестирование WS-слоя и NATS под конкурентными
      сессиями.

## Примечание для ИИ-агента, реализующего эту систему

Не начинай следующий этап, пока не выполнен DoD предыдущего — этапы
специально выстроены так, чтобы каждый следующий опирался на реально
работающий, а не частично написанный предыдущий слой. Если в процессе
реализации обнаруживается противоречие между этим roadmap и
детальными ТЗ (файлы 01-08) — приоритет у детальных ТЗ, roadmap лишь
задаёт порядок, не переопределяет контракты.

## Этап 0.5 — Supabase кастомизация

**Цель:** адаптировать форки Supabase под требования Sovalune.

- [ ] `sovalune-studio`: изменить палитру на фиолетовую (CSS переменные, тема)
- [ ] `sovalune-storage`: настроить под Sovalune Storage API (при необходимости)
- [ ] `sovalune-vector`: убедиться в совместимости с используемой embedding-моделью
- [ ] `sovalune-storage-schema`: добавить расширения `vector`, `uuid-ossp`, `pg_trgm`

**DoD:** `sovalune-studio` отображает фиолетовую тему, все Supabase компоненты
работают в docker-compose вместе с остальными сервисами.
