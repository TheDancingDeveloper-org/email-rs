-- Immutable directory identity for an OAuth account.
-- Microsoft binds to tenant id (tid) + object id (oid). Existing rows stay
-- null until the next successful connect fills them in.
ALTER TABLE accounts ADD COLUMN provider_tenant_id TEXT;
ALTER TABLE accounts ADD COLUMN provider_object_id TEXT;
