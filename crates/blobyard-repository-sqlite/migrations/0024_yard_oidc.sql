-- OIDC first binding compares the provider-verified normalized email against the stored
-- local-user email. Normalize stored emails so pre-existing users remain bindable. The
-- workspace-scoped unique index on active emails makes this statement fail closed when two
-- active users in one workspace normalize to the same address, blocking an ambiguous upgrade.
UPDATE local_users
SET email = lower(trim(email))
WHERE email IS NOT NULL AND email != lower(trim(email));

CREATE TABLE yard_oidc_identities (
  issuer TEXT NOT NULL CHECK(length(issuer) BETWEEN 8 AND 2048),
  provider_subject TEXT NOT NULL CHECK(length(provider_subject) BETWEEN 1 AND 512),
  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
  yard_subject_id TEXT NOT NULL,
  normalized_email TEXT NOT NULL CHECK(
    length(normalized_email) BETWEEN 3 AND 254
    AND normalized_email = lower(normalized_email)
    AND normalized_email = trim(normalized_email)
  ),
  created_at_ms INTEGER NOT NULL CHECK(created_at_ms >= 0),
  last_authenticated_at_ms INTEGER NOT NULL CHECK(last_authenticated_at_ms >= created_at_ms),
  PRIMARY KEY(issuer, provider_subject, workspace_id),
  FOREIGN KEY(yard_subject_id, workspace_id)
    REFERENCES yard_subjects(id, workspace_id) ON DELETE RESTRICT
) STRICT;

CREATE INDEX yard_oidc_identities_by_subject
  ON yard_oidc_identities(workspace_id, yard_subject_id);

CREATE TABLE yard_oidc_attempts (
  state_hash TEXT PRIMARY KEY NOT NULL CHECK(
    length(state_hash) = 64
    AND state_hash NOT GLOB '*[^0-9a-f]*'
  ),
  continuation_hash TEXT NOT NULL UNIQUE CHECK(
    length(continuation_hash) = 64
    AND continuation_hash NOT GLOB '*[^0-9a-f]*'
  ),
  host_label TEXT NOT NULL CHECK(length(host_label) BETWEEN 3 AND 63),
  return_path TEXT NOT NULL CHECK(
    length(return_path) BETWEEN 1 AND 2048
    AND substr(return_path, 1, 1) = '/'
    AND substr(return_path, 1, 2) != '//'
  ),
  created_at_ms INTEGER NOT NULL CHECK(created_at_ms >= 0),
  expires_at_ms INTEGER NOT NULL CHECK(expires_at_ms > created_at_ms),
  claimed_at_ms INTEGER CHECK(
    claimed_at_ms >= created_at_ms
    AND claimed_at_ms < expires_at_ms
  )
) STRICT;

CREATE INDEX yard_oidc_attempts_by_housekeeping
  ON yard_oidc_attempts(claimed_at_ms, expires_at_ms, created_at_ms);
