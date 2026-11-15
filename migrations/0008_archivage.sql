-- 0008 — Archivage SharePoint et reprise des envois (T05, T06) : essais,
-- prochaine tentative, confirmation distante. La copie locale n'est jamais
-- supprimée.

ALTER TABLE document_versions
    ADD COLUMN archive_attempts  integer NOT NULL DEFAULT 0,
    ADD COLUMN archive_next_at   timestamptz,
    ADD COLUMN archived_at       timestamptz;

CREATE INDEX document_versions_archive_due_idx ON document_versions (archive_next_at) WHERE archive_status = 'en_attente';

-- Reprise motivée d'un avis par l'ADMIN (après correction des droits ou
-- vérification d'un résultat inconnu).
ALTER TABLE deliveries
    ADD COLUMN retried_by      uuid REFERENCES users(id),
    ADD COLUMN retried_reason  text;
