-- Sovalune Storage Schema - Memory Entries
-- Vector memory storage with pgvector

CREATE TYPE memory_tier AS ENUM ('raw', 'consolidated', 'verified');

CREATE TABLE memory_entries (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    tier memory_tier NOT NULL DEFAULT 'raw',
    content TEXT NOT NULL,
    embedding VECTOR(768),
    metadata JSONB NOT NULL DEFAULT '{}',
    confidence_score REAL NOT NULL DEFAULT 0.5,
    decay_score REAL NOT NULL DEFAULT 1.0,
    archived BOOLEAN NOT NULL DEFAULT false,
    source_entry_ids UUID[] DEFAULT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Vector similarity index (HNSW)
CREATE INDEX idx_memory_entries_embedding 
    ON memory_entries USING hnsw (embedding vector_cosine_ops);

-- Composite index for filtered queries
CREATE INDEX idx_memory_entries_project_tier 
    ON memory_entries (project_id, tier) WHERE NOT archived;

-- Index for decay operations
CREATE INDEX idx_memory_entries_decay 
    ON memory_entries (decay_score) WHERE NOT archived AND tier = 'raw';
