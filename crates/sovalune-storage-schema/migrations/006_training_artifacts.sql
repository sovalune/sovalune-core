-- Sovalune Storage Schema - Training Artifacts
-- Model training artifact storage

CREATE TABLE training_artifacts (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    cycle_id UUID REFERENCES learning_cycles(id) ON DELETE SET NULL,
    version INT NOT NULL,
    artifact_uri TEXT NOT NULL,
    metrics JSONB NOT NULL DEFAULT '{}',
    promoted BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Indexes
CREATE INDEX idx_training_artifacts_cycle_id ON training_artifacts(cycle_id);
CREATE INDEX idx_training_artifacts_version ON training_artifacts(version);
