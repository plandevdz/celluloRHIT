-- 0002 — Lot 1 (fiabilisation de l'arrivée) et lot 2 (import/export Excel).
-- Migration additive : aucune donnée existante n'est réinterprétée.

-- ------------------------------------------------------------ présence réelle
-- La confirmation d'arrivée est une action RH distincte de la clôture du
-- dossier. Les fiches existantes ne sont pas basculées automatiquement :
-- celles « à venir » dont la date prévue est passée sont signalées au RH.
ALTER TABLE employees
    ADD COLUMN arrival_confirmed_at timestamptz,
    ADD COLUMN arrival_confirmed_by uuid REFERENCES users(id);

-- ------------------------------------------------------------ preuves de fin de tâche
-- remis        : lignes de remise correspondantes (catégorie et quantité)
-- deja_detenu  : affectation active vérifiable du bon bien au même salarié
-- exception    : soldée sur motif ; jamais comptée comme matériel remis
-- realise      : action sans matériel (compte consigné, carte, formation…)
-- Les tâches terminées avant cette migration gardent une résolution vide
-- (« terminée avant le contrôle des preuves ») : leurs observations ne sont
-- pas transformées en preuves.
ALTER TABLE tasks
    ADD COLUMN resolution text CHECK (resolution IS NULL OR resolution IN ('remis', 'deja_detenu', 'exception', 'realise')),
    ADD COLUMN resolution_evidence jsonb;
ALTER TABLE tasks
    ADD CONSTRAINT tasks_resolution_required
    CHECK (status <> 'termine' OR resolution IS NOT NULL) NOT VALID;

-- ------------------------------------------------------------ routage des avis
-- Équipes visées sans aucun destinataire actif : anomalie visible en
-- Administration, sans annuler l'opération métier.
ALTER TABLE outbox
    ADD COLUMN missing_roles text[] NOT NULL DEFAULT '{}';

-- ------------------------------------------------------------ import Excel du personnel
CREATE TABLE import_batches (
    id              uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    created_by      uuid NOT NULL REFERENCES users(id),
    file_name       text NOT NULL,
    file_sha256     text NOT NULL,
    format_version  text NOT NULL,
    status          text NOT NULL DEFAULT 'apercu' CHECK (status IN ('apercu', 'applique', 'abandonne')),
    summary         jsonb,
    created_at      timestamptz NOT NULL DEFAULT now(),
    committed_at    timestamptz,
    committed_by    uuid REFERENCES users(id)
);
-- Un même classeur ne peut être appliqué qu'une fois (rejeu sans doublon).
CREATE UNIQUE INDEX import_batches_applied_file_uq ON import_batches (file_sha256) WHERE status = 'applique';

CREATE TABLE import_rows (
    batch_id              uuid NOT NULL REFERENCES import_batches(id) ON DELETE CASCADE,
    row_number            integer NOT NULL,
    raw                   jsonb NOT NULL,
    proposal              text NOT NULL CHECK (proposal IN ('creation', 'mise_a_jour', 'inchange', 'a_revoir', 'rejet')),
    messages              jsonb NOT NULL DEFAULT '[]'::jsonb,
    employee_id           uuid REFERENCES employees(id),
    employee_version      integer,
    candidates            jsonb NOT NULL DEFAULT '[]'::jsonb,
    changes               jsonb NOT NULL DEFAULT '{}'::jsonb,
    decision              text CHECK (decision IS NULL OR decision IN ('creer', 'rattacher', 'exclure')),
    decision_employee_id  uuid REFERENCES employees(id),
    decided_by            uuid REFERENCES users(id),
    decided_at            timestamptz,
    result                text CHECK (result IS NULL OR result IN ('cree', 'mis_a_jour', 'inchange', 'ignore', 'conflit', 'rejete')),
    result_employee_id    uuid REFERENCES employees(id),
    result_message        text,
    PRIMARY KEY (batch_id, row_number)
);
