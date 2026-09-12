-- duplicata — migração v3 (Fatia 3, US5: fixar/desafixar).
-- Aplicada por duplicata-store::migrations, guardada por
-- schema_meta.schema_version. Não idempotente por si só (ALTER TABLE ADD
-- COLUMN falha se a coluna já existir) — migrations::apply só a executa
-- quando schema_version < 3. Mesmo padrão da v2 (Fatia 2, has_text).

ALTER TABLE clip ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0;

CREATE INDEX IF NOT EXISTS ix_clip_pinned ON clip(pinned);

UPDATE schema_meta SET value = '3' WHERE key = 'schema_version';
