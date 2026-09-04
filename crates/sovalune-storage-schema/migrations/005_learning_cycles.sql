-- Sovalune Storage Schema - Learning Cycles
-- Self-learning loop tracking

CREATE TYPE learning_cycle_status AS ENUM (
    'detected', 'researching', 'verifying',
    'practicing', 'testing', 'applying',
    'completed', 'failed'
);

CREATE TABLE learning_cycles (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    status learning_cycle_status NOT NULL DEFAULT 'detected',
    origin_task_id UUID NOT NULL,
    failure_reason TEXT,
    retry_count INT NOT NULL DEFAULT 0,
    confidence_score REAL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE learning_cycle_evidence (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    cycle_id UUID NOT NULL REFERENCES learning_cycles(id) ON DELETE CASCADE,
    source_type TEXT NOT NULL,
    source_url TEXT,
    excerpt TEXT NOT NULL,
    trust_tier INT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE learning_cycle_test_results (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    cycle_id UUID NOT NULL REFERENCES learning_cycles(id) ON DELETE CASCADE,
    stage TEXT NOT NULL,
    passed BOOLEAN NOT NULL,
    detail JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Indexes
CREATE INDEX idx_learning_cycles_project_id ON learning_cycles(project_id);
CREATE INDEX idx_learning_cycles_status ON learning_cycles(status);
CREATE INDEX idx_learning_cycle_evidence_cycle_id ON learning_cycle_evidence(cycle_id);
CREATE INDEX idx_learning_cycle_test_results_cycle_id ON learning_cycle_test_results(cycle_id);
