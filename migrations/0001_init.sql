-- Celluloplast | Parcours salarié — schéma initial (tranches 1 et 2).
-- Les noms suivent 04-Donnees.md. Les contraintes de concurrence et
-- d'unicité sont portées par PostgreSQL.

CREATE EXTENSION IF NOT EXISTS btree_gist;

-- ---------------------------------------------------------------- référentiels

CREATE TABLE sites (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    code        text NOT NULL UNIQUE,
    name        text NOT NULL,
    active      boolean NOT NULL DEFAULT true,
    created_at  timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE locations (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    site_id     uuid NOT NULL REFERENCES sites(id),
    name        text NOT NULL,
    active      boolean NOT NULL DEFAULT true,
    UNIQUE (site_id, name)
);

-- ---------------------------------------------------------------- utilisateurs

CREATE TABLE roles (
    code   text PRIMARY KEY,
    label  text NOT NULL
);
INSERT INTO roles (code, label) VALUES
    ('ADMIN', 'Administration'),
    ('IT', 'IT'),
    ('RH', 'Ressources humaines'),
    ('GRAPHISTE', 'Graphiste'),
    ('FORMATION', 'Formation');

CREATE TABLE users (
    id                     uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    email                  text NOT NULL,
    display_name           text NOT NULL,
    site_id                uuid REFERENCES sites(id),
    active                 boolean NOT NULL DEFAULT true,
    password_hash          text,
    activation_token_hash  bytea,
    activation_expires_at  timestamptz,
    failed_logins          integer NOT NULL DEFAULT 0,
    locked_until           timestamptz,
    created_at             timestamptz NOT NULL DEFAULT now(),
    updated_at             timestamptz NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX users_email_uq ON users (lower(email));
CREATE UNIQUE INDEX users_activation_uq ON users (activation_token_hash) WHERE activation_token_hash IS NOT NULL;

CREATE TABLE user_roles (
    user_id  uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role     text NOT NULL REFERENCES roles(code),
    PRIMARY KEY (user_id, role)
);

CREATE TABLE sessions (
    token_hash    bytea PRIMARY KEY,
    user_id       uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at    timestamptz NOT NULL DEFAULT now(),
    last_seen_at  timestamptz NOT NULL DEFAULT now(),
    expires_at    timestamptz NOT NULL,
    revoked_at    timestamptz
);
CREATE INDEX sessions_user_idx ON sessions (user_id);

-- ---------------------------------------------------------------- personnel

CREATE TABLE employees (
    id               uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    matricule        text,
    matricule_norm   text,
    first_name       text NOT NULL,
    last_name        text NOT NULL,
    name_key         text NOT NULL,
    status           text NOT NULL DEFAULT 'a_venir' CHECK (status IN ('a_venir', 'actif', 'sorti')),
    planned_arrival  date,
    actual_arrival   date,
    exit_date        date,
    m365_verified_at timestamptz,
    m365_verified_by uuid REFERENCES users(id),
    version          integer NOT NULL DEFAULT 1,
    created_at       timestamptz NOT NULL DEFAULT now(),
    updated_at       timestamptz NOT NULL DEFAULT now()
);
-- Unicité du matricule renseigné et normalisé ; jamais sur le nom.
CREATE UNIQUE INDEX employees_matricule_uq ON employees (matricule_norm) WHERE matricule_norm IS NOT NULL;
CREATE INDEX employees_name_key_idx ON employees (name_key);

CREATE TABLE employee_positions (
    id           uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    employee_id  uuid NOT NULL REFERENCES employees(id),
    job_title    text NOT NULL,
    site_id      uuid NOT NULL REFERENCES sites(id),
    department   text,
    manager      text,
    valid_from   date NOT NULL,
    valid_to     date,
    created_at   timestamptz NOT NULL DEFAULT now(),
    CHECK (valid_to IS NULL OR valid_to > valid_from),
    -- Pas de chevauchement de deux situations effectives.
    EXCLUDE USING gist (employee_id WITH =, daterange(valid_from, valid_to, '[)') WITH &&)
);

-- ---------------------------------------------------------------- dossiers

CREATE SEQUENCE case_ref_onb;
CREATE SEQUENCE case_ref_mut;
CREATE SEQUENCE case_ref_off;
CREATE SEQUENCE case_ref_dem;
CREATE SEQUENCE movement_ref;

CREATE TABLE cases (
    id              uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    reference       text NOT NULL UNIQUE,
    kind            text NOT NULL CHECK (kind IN ('ONB', 'MUT', 'OFF', 'DEM')),
    employee_id     uuid NOT NULL REFERENCES employees(id),
    parent_case_id  uuid REFERENCES cases(id),
    status          text NOT NULL DEFAULT 'ouvert' CHECK (status IN ('ouvert', 'clos', 'annule')),
    effective_date  date,
    due_date        date,
    note            text,
    created_by      uuid NOT NULL REFERENCES users(id),
    closed_by       uuid REFERENCES users(id),
    closed_at       timestamptz,
    close_note      text,
    version         integer NOT NULL DEFAULT 1,
    created_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX cases_employee_idx ON cases (employee_id);
CREATE INDEX cases_open_idx ON cases (kind, status);

-- Référence immuable.
CREATE FUNCTION forbid_reference_change() RETURNS trigger AS $$
BEGIN
    IF NEW.reference <> OLD.reference THEN
        RAISE EXCEPTION 'la référence publique est immuable';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;
CREATE TRIGGER cases_reference_immutable BEFORE UPDATE ON cases
    FOR EACH ROW EXECUTE FUNCTION forbid_reference_change();

CREATE TABLE case_needs (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    case_id     uuid NOT NULL REFERENCES cases(id),
    category    text NOT NULL CHECK (category IN ('pc', 'telephone', 'sim', 'email_pro', 'xena', 'intelix', 'carte_visite', 'vehicule', 'formation')),
    answer      text NOT NULL DEFAULT 'a_preciser' CHECK (answer IN ('oui', 'non', 'a_preciser')),
    quantity    integer CHECK (quantity IS NULL OR quantity BETWEEN 1 AND 999),
    preference  text CHECK (preference IS NULL OR preference IN ('fixe', 'portable', 'sans_preference')),
    detail      text,
    origin      text NOT NULL DEFAULT 'rh' CHECK (origin IN ('rh', 'profil')),
    revision    integer NOT NULL DEFAULT 1,
    updated_by  uuid NOT NULL REFERENCES users(id),
    updated_at  timestamptz NOT NULL DEFAULT now(),
    UNIQUE (case_id, category)
);

CREATE TABLE need_revisions (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    need_id     uuid NOT NULL REFERENCES case_needs(id),
    revision    integer NOT NULL,
    answer      text NOT NULL,
    quantity    integer,
    preference  text,
    detail      text,
    changed_by  uuid NOT NULL REFERENCES users(id),
    changed_at  timestamptz NOT NULL DEFAULT now(),
    UNIQUE (need_id, revision)
);

-- ---------------------------------------------------------------- formations

CREATE TABLE training_catalog (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    name        text NOT NULL,
    active      boolean NOT NULL DEFAULT true,
    created_at  timestamptz NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX training_catalog_name_uq ON training_catalog (lower(name));

CREATE TABLE training_sessions (
    id            uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    case_id       uuid REFERENCES cases(id),
    employee_id   uuid NOT NULL REFERENCES employees(id),
    catalog_id    uuid NOT NULL REFERENCES training_catalog(id),
    trainer_id    uuid REFERENCES users(id),
    status        text NOT NULL DEFAULT 'a_planifier' CHECK (status IN ('a_planifier', 'planifiee', 'realisee', 'reportee', 'dispensee', 'annulee')),
    planned_date  date,
    done_date     date,
    reason        text,
    result        text,
    created_at    timestamptz NOT NULL DEFAULT now(),
    updated_at    timestamptz NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX training_sessions_case_uq ON training_sessions (case_id, catalog_id) WHERE case_id IS NOT NULL;

-- ---------------------------------------------------------------- tâches

CREATE TABLE tasks (
    id                   uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    case_id              uuid NOT NULL REFERENCES cases(id),
    need_id              uuid REFERENCES case_needs(id),
    training_session_id  uuid REFERENCES training_sessions(id),
    gen_key              text NOT NULL,
    role                 text NOT NULL REFERENCES roles(code),
    title                text NOT NULL,
    owner_id             uuid REFERENCES users(id),
    status               text NOT NULL DEFAULT 'a_faire' CHECK (status IN ('a_faire', 'en_cours', 'bloque', 'termine', 'annule')),
    due_date             date,
    blocked_reason       text,
    done_note            text,
    claimed_at           timestamptz,
    completed_at         timestamptz,
    completed_by         uuid REFERENCES users(id),
    version              integer NOT NULL DEFAULT 1,
    created_at           timestamptz NOT NULL DEFAULT now(),
    updated_at           timestamptz NOT NULL DEFAULT now(),
    -- Rejouer une génération ne crée pas de doublon.
    UNIQUE (case_id, gen_key),
    -- Un seul propriétaire, présent dès que la tâche est prise.
    CHECK (status = 'a_faire' OR status = 'annule' OR owner_id IS NOT NULL)
);
CREATE INDEX tasks_queue_idx ON tasks (role, status);
CREATE INDEX tasks_owner_idx ON tasks (owner_id) WHERE owner_id IS NOT NULL;

-- ---------------------------------------------------------------- comptes (sans secret)

CREATE TABLE employee_accounts (
    id           uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    employee_id  uuid NOT NULL REFERENCES employees(id),
    system       text NOT NULL CHECK (system IN ('email', 'xena', 'intelix', 'autre')),
    platform     text CHECK (platform IS NULL OR platform IN ('cpanel', 'm365')),
    identifier   text NOT NULL,
    status       text NOT NULL DEFAULT 'actif' CHECK (status IN ('actif', 'a_verifier', 'coupe')),
    created_by   uuid NOT NULL REFERENCES users(id),
    verified_at  timestamptz,
    ended_at     timestamptz,
    created_at   timestamptz NOT NULL DEFAULT now(),
    CHECK (system <> 'email' OR platform IS NOT NULL)
);
CREATE UNIQUE INDEX employee_accounts_active_uq ON employee_accounts (system, lower(identifier)) WHERE status <> 'coupe';

-- ---------------------------------------------------------------- parc IT

CREATE TABLE assets (
    id            uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    inventory_no  text NOT NULL UNIQUE,
    qr_token      text NOT NULL UNIQUE,
    kind          text NOT NULL CHECK (kind IN ('pc_portable', 'pc_fixe', 'telephone', 'ecran', 'imprimante', 'autre')),
    label         text NOT NULL,
    serial_no     text,
    status        text NOT NULL DEFAULT 'disponible' CHECK (status IN ('disponible', 'reserve', 'affecte', 'controle', 'reparation', 'reforme')),
    site_id       uuid REFERENCES sites(id),
    location_id   uuid REFERENCES locations(id),
    notes         text,
    version       integer NOT NULL DEFAULT 1,
    created_at    timestamptz NOT NULL DEFAULT now(),
    updated_at    timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE accessories (
    id           uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    label        text NOT NULL,
    site_id      uuid REFERENCES sites(id),
    quantity_on_hand integer NOT NULL DEFAULT 0 CHECK (quantity_on_hand >= 0),
    created_at   timestamptz NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX accessories_label_site_uq ON accessories (lower(label), site_id) NULLS NOT DISTINCT;

CREATE TABLE mobile_lines (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    number      text NOT NULL UNIQUE,
    operator    text NOT NULL,
    status      text NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'suspendue', 'resiliee')),
    created_at  timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE sim_cards (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    iccid       text NOT NULL UNIQUE,
    line_id     uuid REFERENCES mobile_lines(id),
    status      text NOT NULL DEFAULT 'disponible' CHECK (status IN ('disponible', 'affecte', 'desactivee')),
    created_at  timestamptz NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX sim_cards_line_uq ON sim_cards (line_id) WHERE line_id IS NOT NULL AND status <> 'desactivee';

-- ---------------------------------------------------------------- mouvements

CREATE TABLE movements (
    id                   uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    reference            text NOT NULL UNIQUE,
    kind                 text NOT NULL CHECK (kind IN ('remise', 'restitution')),
    employee_id          uuid NOT NULL REFERENCES employees(id),
    case_id              uuid REFERENCES cases(id),
    task_id              uuid REFERENCES tasks(id),
    operator_id          uuid NOT NULL REFERENCES users(id),
    effective_at         timestamptz NOT NULL DEFAULT now(),
    substitution_reason  text,
    note                 text,
    created_at           timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX movements_employee_idx ON movements (employee_id);

CREATE TABLE movement_lines (
    id              uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    movement_id     uuid NOT NULL REFERENCES movements(id),
    line_no         integer NOT NULL,
    item_type       text NOT NULL CHECK (item_type IN ('asset', 'accessoire', 'sim')),
    asset_id        uuid REFERENCES assets(id),
    accessory_id    uuid REFERENCES accessories(id),
    sim_id          uuid REFERENCES sim_cards(id),
    label           text NOT NULL,
    identifier      text,
    quantity        integer NOT NULL DEFAULT 1 CHECK (quantity >= 1),
    condition       text,
    note            text,
    origin_line_id  uuid REFERENCES movement_lines(id),
    UNIQUE (movement_id, line_no),
    CHECK (
        (item_type = 'asset' AND asset_id IS NOT NULL AND accessory_id IS NULL AND sim_id IS NULL AND quantity = 1) OR
        (item_type = 'accessoire' AND accessory_id IS NOT NULL AND asset_id IS NULL AND sim_id IS NULL) OR
        (item_type = 'sim' AND sim_id IS NOT NULL AND asset_id IS NULL AND accessory_id IS NULL AND quantity = 1)
    )
);

-- Garde réelle : une ligne par élément remis, soldée par les restitutions.
CREATE TABLE assignments (
    id               uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    employee_id      uuid NOT NULL REFERENCES employees(id),
    handover_line_id uuid NOT NULL UNIQUE REFERENCES movement_lines(id),
    item_type        text NOT NULL,
    asset_id         uuid REFERENCES assets(id),
    accessory_id     uuid REFERENCES accessories(id),
    sim_id           uuid REFERENCES sim_cards(id),
    quantity         integer NOT NULL CHECK (quantity >= 1),
    open_quantity    integer NOT NULL CHECK (open_quantity >= 0),
    started_at       timestamptz NOT NULL,
    ended_at         timestamptz,
    CHECK (open_quantity <= quantity),
    CHECK ((open_quantity = 0) = (ended_at IS NOT NULL))
);
-- Un seul détenteur actif par bien individuel.
CREATE UNIQUE INDEX assignments_asset_active_uq ON assignments (asset_id) WHERE asset_id IS NOT NULL AND ended_at IS NULL;
CREATE UNIQUE INDEX assignments_sim_active_uq ON assignments (sim_id) WHERE sim_id IS NOT NULL AND ended_at IS NULL;
CREATE INDEX assignments_employee_idx ON assignments (employee_id) WHERE ended_at IS NULL;

-- ---------------------------------------------------------------- documents

CREATE TABLE documents (
    id                uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    movement_id       uuid NOT NULL UNIQUE REFERENCES movements(id),
    kind              text NOT NULL CHECK (kind IN ('affectation', 'restitution')),
    template_version  text NOT NULL,
    status            text NOT NULL DEFAULT 'a_generer' CHECK (status IN ('a_generer', 'genere', 'signe')),
    created_at        timestamptz NOT NULL DEFAULT now(),
    updated_at        timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE document_versions (
    id              uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    document_id     uuid NOT NULL REFERENCES documents(id),
    version         integer NOT NULL,
    file_kind       text NOT NULL CHECK (file_kind IN ('original', 'scan')),
    storage_path    text NOT NULL UNIQUE,
    sha256          text NOT NULL,
    size_bytes      bigint NOT NULL,
    mime_type       text NOT NULL,
    archive_status  text NOT NULL DEFAULT 'en_attente' CHECK (archive_status IN ('en_attente', 'archive', 'echec', 'desactive')),
    archive_id      text,
    archive_error   text,
    created_by      uuid REFERENCES users(id),
    created_at      timestamptz NOT NULL DEFAULT now(),
    UNIQUE (document_id, file_kind, version)
);

-- ---------------------------------------------------------------- audit, idempotence, outbox

CREATE TABLE audit_events (
    id              bigserial PRIMARY KEY,
    at              timestamptz NOT NULL DEFAULT now(),
    actor_id        uuid REFERENCES users(id),
    action          text NOT NULL,
    entity          text NOT NULL,
    entity_id       uuid,
    case_id         uuid REFERENCES cases(id),
    employee_id     uuid REFERENCES employees(id),
    correlation_id  uuid NOT NULL,
    data            jsonb NOT NULL DEFAULT '{}'::jsonb
);
CREATE INDEX audit_case_idx ON audit_events (case_id, at);
CREATE INDEX audit_employee_idx ON audit_events (employee_id, at);

-- Historique en lecture seule.
CREATE FUNCTION forbid_audit_change() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'le journal d''audit est en lecture seule';
END;
$$ LANGUAGE plpgsql;
CREATE TRIGGER audit_events_readonly BEFORE UPDATE OR DELETE ON audit_events
    FOR EACH ROW EXECUTE FUNCTION forbid_audit_change();

CREATE TABLE command_keys (
    user_id          uuid NOT NULL REFERENCES users(id),
    key              text NOT NULL,
    endpoint         text NOT NULL,
    request_hash     text NOT NULL,
    response_status  integer,
    response_body    jsonb,
    created_at       timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, key)
);

CREATE TABLE outbox (
    id              uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    event_type      text NOT NULL,
    case_id         uuid REFERENCES cases(id),
    -- Un avis vise une ou plusieurs équipes et/ou personnes ; les
    -- destinataires sont dédupliqués (un utilisateur IT + FORMATION reçoit
    -- un seul mail pour le même avis).
    target_roles    text[] NOT NULL DEFAULT '{}',
    target_user_ids uuid[] NOT NULL DEFAULT '{}',
    subject         text NOT NULL,
    body            text NOT NULL,
    link_path       text,
    correlation_id  uuid NOT NULL,
    status          text NOT NULL DEFAULT 'en_attente' CHECK (status IN ('en_attente', 'traite', 'erreur')),
    lease_until     timestamptz,
    attempts        integer NOT NULL DEFAULT 0,
    last_error      text,
    created_at      timestamptz NOT NULL DEFAULT now(),
    processed_at    timestamptz,
    CHECK (cardinality(target_roles) > 0 OR cardinality(target_user_ids) > 0)
);
CREATE INDEX outbox_pending_idx ON outbox (created_at) WHERE status = 'en_attente';

CREATE TABLE deliveries (
    id               uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    outbox_id        uuid NOT NULL REFERENCES outbox(id),
    recipient_email  text NOT NULL,
    sent_to          text NOT NULL,
    status           text NOT NULL DEFAULT 'en_attente' CHECK (status IN ('en_attente', 'journalise', 'accepte', 'a_verifier', 'echec')),
    attempts         integer NOT NULL DEFAULT 0,
    next_attempt_at  timestamptz NOT NULL DEFAULT now(),
    lease_until      timestamptz,
    last_result      text,
    last_attempt_at  timestamptz,
    created_at       timestamptz NOT NULL DEFAULT now(),
    -- Déduplication par événement et destinataire, même avec plusieurs rôles.
    UNIQUE (outbox_id, recipient_email)
);
CREATE INDEX deliveries_due_idx ON deliveries (next_attempt_at) WHERE status = 'en_attente';
