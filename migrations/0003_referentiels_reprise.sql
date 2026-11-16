-- 0003 — Référentiels complétés (wilayas, profils de poste, contrat,
-- manager), parc IT détaillé, et reprise des listes SharePoint.
-- Migration additive : aucune donnée existante n'est réinterprétée.

-- ------------------------------------------------------------ wilayas
CREATE TABLE wilayas (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    code        integer NOT NULL UNIQUE CHECK (code BETWEEN 1 AND 999),
    name        text NOT NULL,
    region      text,
    sort_order  integer,
    -- Coordonnées de la source si elles existent ; jamais calculées.
    latitude    numeric(9, 6),
    longitude   numeric(9, 6),
    active      boolean NOT NULL DEFAULT true,
    created_at  timestamptz NOT NULL DEFAULT now()
);

-- ------------------------------------------------------------ personnel
ALTER TABLE employees
    ADD COLUMN contract_type text CHECK (contract_type IS NULL OR contract_type IN
        ('cdi', 'cdd', 'stage', 'interim', 'apprentissage', 'consultant')),
    -- Ancien champ Personnel.PosteFixe, repris tel quel : son sens
    -- (ordinateur fixe ? téléphone fixe ?) reste à confirmer par le RH.
    ADD COLUMN poste_fixe text,
    -- Ancienne « RemarqueImport » : note de reprise, visible du RH seul.
    ADD COLUMN import_note text,
    -- « En départ » : la personne prépare sa sortie mais n'est pas sortie.
    ADD COLUMN leaving boolean NOT NULL DEFAULT false,
    ADD COLUMN planned_exit_date date;

-- Wilayas couvertes (commerciaux terrain) : information organisationnelle.
CREATE TABLE employee_wilayas (
    employee_id  uuid NOT NULL REFERENCES employees(id),
    wilaya_id    uuid NOT NULL REFERENCES wilayas(id),
    PRIMARY KEY (employee_id, wilaya_id)
);

-- ------------------------------------------------------------ besoins : équipements distincts
-- Écran, tablette, imprimante et accessoire deviennent des besoins à part
-- entière, avec quantité (ex. deux écrans), au lieu d'une note libre.
ALTER TABLE case_needs DROP CONSTRAINT case_needs_category_check;
ALTER TABLE case_needs ADD CONSTRAINT case_needs_category_check CHECK (category IN (
    'pc', 'ecran', 'tablette', 'imprimante', 'accessoire', 'telephone', 'sim',
    'email_pro', 'xena', 'intelix', 'carte_visite', 'vehicule', 'formation'));

-- ------------------------------------------------------------ profils de poste
-- Un profil suggère des besoins ; le modifier ne change aucun dossier
-- existant (les besoins sont copiés dans le dossier à l'annonce).
CREATE TABLE job_profiles (
    id              uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    name            text NOT NULL,
    department      text,
    description     text,
    typical_titles  text,
    access_notes    text,
    field_based     boolean NOT NULL DEFAULT false,
    active          boolean NOT NULL DEFAULT true,
    version         integer NOT NULL DEFAULT 1,
    created_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX job_profiles_name_uq ON job_profiles (lower(name));

CREATE TABLE profile_needs (
    profile_id  uuid NOT NULL REFERENCES job_profiles(id) ON DELETE CASCADE,
    category    text NOT NULL CHECK (category IN ('pc', 'ecran', 'tablette', 'imprimante', 'accessoire', 'telephone', 'sim',
                                               'email_pro', 'xena', 'intelix', 'carte_visite', 'vehicule')),
    quantity    integer CHECK (quantity IS NULL OR quantity BETWEEN 1 AND 999),
    preference  text CHECK (preference IS NULL OR preference IN ('fixe', 'portable', 'sans_preference')),
    detail      text,
    PRIMARY KEY (profile_id, category),
    CHECK (preference IS NULL OR category = 'pc')
);

CREATE TABLE profile_trainings (
    profile_id  uuid NOT NULL REFERENCES job_profiles(id) ON DELETE CASCADE,
    catalog_id  uuid NOT NULL REFERENCES training_catalog(id),
    PRIMARY KEY (profile_id, catalog_id)
);

-- Situation : profil et manager (fiche salarié) facultatifs, historisés
-- avec le poste.
ALTER TABLE employee_positions
    ADD COLUMN job_profile_id       uuid REFERENCES job_profiles(id),
    ADD COLUMN manager_employee_id  uuid REFERENCES employees(id);

-- ------------------------------------------------------------ catalogue des formations
ALTER TABLE training_catalog
    ADD COLUMN category            text CHECK (category IS NULL OR category IN ('produit', 'solution_digitale', 'qhse', 'autre')),
    ADD COLUMN duration_hours      numeric(6, 2) CHECK (duration_hours IS NULL OR duration_hours > 0),
    ADD COLUMN description         text,
    ADD COLUMN support_url         text,
    ADD COLUMN default_trainer_id  uuid REFERENCES users(id),
    -- Intervenant par défaut sans compte applicatif (nom conservé tel quel).
    ADD COLUMN default_trainer_name text;

-- Évaluation d'une session réalisée (le commentaire libre `result` reste).
-- Qui a animé (compte FORMATION ou simple nom) est distinct de qui a
-- enregistré la session dans l'application (recorded_by, recorded_at).
ALTER TABLE training_sessions
    ADD COLUMN evaluation   text CHECK (evaluation IS NULL OR evaluation IN ('acquis', 'a_revoir')),
    ADD COLUMN trainer_name text,
    ADD COLUMN recorded_by  uuid REFERENCES users(id),
    ADD COLUMN recorded_at  timestamptz;

-- ------------------------------------------------------------ parc IT détaillé
ALTER TABLE assets DROP CONSTRAINT assets_kind_check;
ALTER TABLE assets ADD CONSTRAINT assets_kind_check CHECK (kind IN (
    'pc_portable', 'pc_fixe', 'telephone', 'tablette', 'ecran', 'imprimante', 'scanner', 'serveur',
    'onduleur', 'reseau', 'pointeuse', 'videoprojecteur', 'destructeur', 'videosurveillance', 'autre'));
ALTER TABLE assets DROP CONSTRAINT assets_status_check;
ALTER TABLE assets ADD CONSTRAINT assets_status_check CHECK (status IN (
    'disponible', 'reserve', 'affecte', 'en_service', 'controle', 'reparation', 'reforme'));
ALTER TABLE assets
    ADD COLUMN brand          text,
    ADD COLUMN model          text,
    ADD COLUMN intune_name    text,
    ADD COLUMN purchase_date  date,
    ADD COLUMN warranty_end   date,
    ADD COLUMN condition      text CHECK (condition IS NULL OR condition IN ('neuf', 'bon', 'use', 'hs'));

ALTER TABLE mobile_lines
    ADD COLUMN plan       text,
    ADD COLUMN line_type  text CHECK (line_type IS NULL OR line_type IN ('voix', 'data', 'voix_data'));

-- Ligne principale du salarié : choisie parmi les lignes dont il détient
-- réellement la SIM (jamais saisie en texte libre).
ALTER TABLE employees ADD COLUMN primary_line_id uuid REFERENCES mobile_lines(id);

-- Garde reprise depuis l'ancienne application : mouvement distinct d'une
-- remise réelle (aucune décharge n'est fabriquée pour le passé).
ALTER TABLE movements DROP CONSTRAINT movements_kind_check;
ALTER TABLE movements ADD CONSTRAINT movements_kind_check CHECK (kind IN ('remise', 'restitution', 'reprise'));
-- Garde reprise : justificatif resté dans SharePoint. Lien vérifié
-- (https://….sharepoint.com) ou référence identifiable (pièce jointe de
-- l'élément n° X de la liste Affectations). Jamais de PDF régénéré.
ALTER TABLE movements
    ADD COLUMN legacy_proof_url  text CHECK (legacy_proof_url IS NULL OR legacy_proof_url ~ '^https://[a-z0-9-]+\.sharepoint\.com/'),
    ADD COLUMN legacy_proof_ref  text;
-- Compte repris de SharePoint : sans preuve de vérification, verified_at
-- reste vide (« vérification non renseignée »), jamais la date de reprise.
ALTER TABLE employee_accounts ADD COLUMN imported boolean NOT NULL DEFAULT false;

-- ------------------------------------------------------------ reprise SharePoint
-- Un lot par fichier exporté d'une liste ; aperçu sans écriture métier,
-- décisions, puis application transactionnelle ligne par ligne.
CREATE TABLE migration_batches (
    id            uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    source_list   text NOT NULL,
    file_name     text NOT NULL,
    file_sha256   text NOT NULL,
    status        text NOT NULL DEFAULT 'apercu' CHECK (status IN ('apercu', 'applique', 'abandonne')),
    summary       jsonb,
    created_by    uuid NOT NULL REFERENCES users(id),
    created_at    timestamptz NOT NULL DEFAULT now(),
    committed_at  timestamptz,
    committed_by  uuid REFERENCES users(id)
);
CREATE UNIQUE INDEX migration_batches_applied_uq ON migration_batches (source_list, file_sha256) WHERE status = 'applique';

CREATE TABLE migration_rows (
    batch_id        uuid NOT NULL REFERENCES migration_batches(id) ON DELETE CASCADE,
    row_number      integer NOT NULL,
    source_id       text,
    raw             jsonb NOT NULL,
    proposal        text NOT NULL CHECK (proposal IN ('creation', 'mise_a_jour', 'inchange', 'a_revoir', 'rejet')),
    messages        jsonb NOT NULL DEFAULT '[]'::jsonb,
    target_id       uuid,
    candidates      jsonb NOT NULL DEFAULT '[]'::jsonb,
    resolved        jsonb NOT NULL DEFAULT '{}'::jsonb,
    decision        text CHECK (decision IS NULL OR decision IN ('appliquer', 'exclure')),
    decision_target uuid,
    decided_by      uuid REFERENCES users(id),
    decided_at      timestamptz,
    result          text CHECK (result IS NULL OR result IN ('cree', 'mis_a_jour', 'inchange', 'ignore', 'conflit', 'rejete')),
    result_id       uuid,
    result_message  text,
    PRIMARY KEY (batch_id, row_number)
);

-- Correspondance ancien ID SharePoint → enregistrement de la base ; permet
-- de préserver les liens et de rejouer une reprise sans doublon.
CREATE TABLE legacy_ids (
    source_list   text NOT NULL,
    source_id     text NOT NULL,
    entity        text NOT NULL,
    entity_id     uuid NOT NULL,
    source_title  text,
    batch_id      uuid REFERENCES migration_batches(id),
    created_at    timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (source_list, source_id)
);
CREATE INDEX legacy_ids_entity_idx ON legacy_ids (entity, entity_id);
CREATE INDEX legacy_ids_title_idx ON legacy_ids (source_list, lower(source_title));
