-- 0009 — Administration depuis l'interface : réglages d'exploitation
-- (modules, Exchange, SharePoint, échéances, sauvegardes), modèles de tâches,
-- wilayas cibles d'une mutation. Migration additive : aucune donnée
-- existante n'est réinterprétée ; sans réglage enregistré, les valeurs du
-- fichier .env continuent de s'appliquer.

-- Réglages saisis par l'ADMIN. Une seule ligne ; chaque section absente du
-- JSON reprend la valeur du fichier .env. Aucun secret : les clés privées
-- Graph sont des fichiers du serveur (DATA_DIR/secrets), jamais en base.
CREATE TABLE runtime_settings (
    id          boolean PRIMARY KEY DEFAULT true CHECK (id),
    data        jsonb NOT NULL DEFAULT '{}'::jsonb,
    version     integer NOT NULL DEFAULT 1,
    updated_at  timestamptz NOT NULL DEFAULT now(),
    updated_by  uuid REFERENCES users(id)
);
INSERT INTO runtime_settings (id) VALUES (true);

-- Sauvegardes chiffrées produites par le worker (base, documents, clés).
CREATE TABLE backup_runs (
    id            uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    trigger       text NOT NULL CHECK (trigger IN ('auto', 'manuel')),
    status        text NOT NULL DEFAULT 'demande' CHECK (status IN ('demande', 'en_cours', 'reussie', 'echec')),
    stamp         text,
    files         jsonb NOT NULL DEFAULT '[]'::jsonb,
    warnings      jsonb NOT NULL DEFAULT '[]'::jsonb,
    error         text,
    fingerprint   text,
    requested_by  uuid REFERENCES users(id),
    requested_at  timestamptz NOT NULL DEFAULT now(),
    started_at    timestamptz,
    finished_at   timestamptz,
    purged_at     timestamptz
);
CREATE INDEX backup_runs_recent_idx ON backup_runs (requested_at DESC);
-- Une seule sauvegarde à la fois.
CREATE UNIQUE INDEX backup_runs_one_active_uq ON backup_runs ((true)) WHERE status IN ('demande', 'en_cours');

-- Modèles de tâches : libellé, consignes, délai et ordre d'affichage par
-- type de tâche (clé de génération : « need:pc », « formation »…). Copiés
-- dans la tâche à sa création : modifier un modèle ne réécrit pas les
-- tâches existantes.
CREATE TABLE task_templates (
    task_key       text PRIMARY KEY,
    title          text,
    instructions   text,
    delay_days     integer CHECK (delay_days BETWEEN -365 AND 365),
    display_order  integer NOT NULL DEFAULT 100,
    updated_at     timestamptz NOT NULL DEFAULT now(),
    updated_by     uuid REFERENCES users(id)
);

ALTER TABLE tasks
    ADD COLUMN instructions   text,
    ADD COLUMN display_order  integer NOT NULL DEFAULT 100;

-- Wilayas couvertes après la mutation. NULL : inchangées ; tableau vide :
-- plus aucune wilaya.
ALTER TABLE transfers ADD COLUMN target_wilaya_ids uuid[];

-- Sites : renommage et fermeture par l'ADMIN (jamais de suppression : des
-- salariés et des biens y restent rattachés).
ALTER TABLE sites ADD COLUMN updated_at timestamptz NOT NULL DEFAULT now();
