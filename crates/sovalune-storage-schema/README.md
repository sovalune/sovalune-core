# Sovalune Storage Schema

SQL migrations and schema definitions for Sovalune Storage (Supabase Postgres + pgvector).

## Overview

This repository contains the database schema for the Sovalune AI Agent Platform. It defines tables for:
- Projects and sessions
- Messages and chat history
- Vector memory entries (with pgvector)
- Learning cycles and evidence
- Training artifacts

## Migrations

Migrations are numbered sequentially and apply in order:

| Migration | Description |
|-----------|-------------|
| 001 | Enable extensions (vector, uuid-ossp, pg_trgm) |
| 002 | Projects table |
| 003 | Sessions and messages tables |
| 004 | Memory entries with vector embeddings |
| 005 | Learning cycles, evidence, test results |
| 006 | Training artifacts |
| 007 | Row Level Security policies |

## Usage

### With Sovalune Core (Rust)

Migrations are applied automatically via `sqlx::migrate!` in development:

```rust
sqlx::migrate!("../sovalune-storage-schema/migrations")
    .run(&pool)
    .await?;
```

### Manual Application

```bash
# Connect to PostgreSQL
psql -U sovalune -d sovalune

# Run migrations in order
\i migrations/001_extensions.sql
\i migrations/002_projects.sql
# ... etc
```

### With Docker Compose

Migrations are automatically applied on first startup via `sovalune-infra/migrations/`.

## Schema Extensions

- **pgvector** - Vector similarity search
- **uuid-ossp** - UUID generation
- **pg_trgm** - Trigram matching for hybrid search

## Development

When adding new migrations:
1. Create a new file with sequential number: `NNN_description.sql`
2. Never modify existing applied migrations
3. Update this README with the new migration description
