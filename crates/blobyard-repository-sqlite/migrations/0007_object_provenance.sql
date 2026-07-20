ALTER TABLE object_versions
  ADD COLUMN source TEXT NOT NULL DEFAULT 'cli'
  CHECK(source IN ('ci', 'cli', 'inbox', 'preview', 'web'));

ALTER TABLE object_versions
  ADD COLUMN git_repository TEXT;

ALTER TABLE object_versions
  ADD COLUMN git_commit TEXT;
