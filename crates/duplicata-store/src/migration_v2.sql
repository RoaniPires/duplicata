-- duplicata — migração v2 (Fatia 2, US3: colar texto puro).
-- Aplicada por duplicata-store::migrations, guardada por
-- schema_meta.schema_version. Não idempotente por si só (ALTER TABLE ADD
-- COLUMN falha se a coluna já existir) — migrations::apply só a executa
-- quando schema_version < 2.

ALTER TABLE clip ADD COLUMN has_text INTEGER NOT NULL DEFAULT 0;

UPDATE schema_meta SET value = '2' WHERE key = 'schema_version';
