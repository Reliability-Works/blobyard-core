ALTER TABLE api_tokens ADD COLUMN token_prefix TEXT NOT NULL DEFAULT 'legacy';
ALTER TABLE api_tokens ADD COLUMN project_id TEXT REFERENCES projects(id) ON DELETE RESTRICT;
ALTER TABLE api_tokens ADD COLUMN created_at_ms INTEGER NOT NULL DEFAULT 0 CHECK(created_at_ms >= 0);
ALTER TABLE api_tokens ADD COLUMN expires_at_ms INTEGER NOT NULL DEFAULT 9223372036854775807 CHECK(expires_at_ms >= 0);
ALTER TABLE api_tokens ADD COLUMN last_used_at_ms INTEGER CHECK(last_used_at_ms >= 0);
ALTER TABLE api_tokens ADD COLUMN revoked_at_ms INTEGER CHECK(revoked_at_ms >= 0);

UPDATE api_tokens SET revoked_at_ms = 0 WHERE revoked = 1;

CREATE INDEX api_tokens_created_idx ON api_tokens(created_at_ms DESC, id DESC);
