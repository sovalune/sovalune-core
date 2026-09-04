-- Sovalune Storage Schema - Row Level Security
-- Enable RLS for all tables (Supabase feature)

ALTER TABLE projects ENABLE ROW LEVEL SECURITY;
ALTER TABLE sessions ENABLE ROW LEVEL SECURITY;
ALTER TABLE messages ENABLE ROW LEVEL SECURITY;
ALTER TABLE memory_entries ENABLE ROW LEVEL SECURITY;
ALTER TABLE learning_cycles ENABLE ROW LEVEL SECURITY;
ALTER TABLE learning_cycle_evidence ENABLE ROW LEVEL SECURITY;
ALTER TABLE learning_cycle_test_results ENABLE ROW LEVEL SECURITY;
ALTER TABLE training_artifacts ENABLE ROW LEVEL SECURITY;

-- Basic policies (can be customized later for multi-tenant)
CREATE POLICY "Allow all for anon" ON projects FOR ALL USING (true);
CREATE POLICY "Allow all for anon" ON sessions FOR ALL USING (true);
CREATE POLICY "Allow all for anon" ON messages FOR ALL USING (true);
CREATE POLICY "Allow all for anon" ON memory_entries FOR ALL USING (true);
CREATE POLICY "Allow all for anon" ON learning_cycles FOR ALL USING (true);
CREATE POLICY "Allow all for anon" ON learning_cycle_evidence FOR ALL USING (true);
CREATE POLICY "Allow all for anon" ON learning_cycle_test_results FOR ALL USING (true);
CREATE POLICY "Allow all for anon" ON training_artifacts FOR ALL USING (true);
