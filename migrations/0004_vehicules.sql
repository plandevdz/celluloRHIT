-- 0004 — Parc véhicules RH (M03) : propriété ou location, documents,
-- entretien, remises, restitutions et retour au loueur.

CREATE TABLE vehicles (
    id           uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    plate        text NOT NULL,
    plate_norm   text NOT NULL UNIQUE,
    kind         text NOT NULL CHECK (kind IN ('propriete', 'location')),
    brand        text,
    model        text,
    site_id      uuid REFERENCES sites(id),
    odometer_km  integer CHECK (odometer_km IS NULL OR odometer_km >= 0),
    odometer_at  timestamptz,
    status       text NOT NULL DEFAULT 'disponible'
                 CHECK (status IN ('disponible', 'affecte', 'immobilise', 'rendu_loueur', 'reforme')),
    notes        text,
    version      integer NOT NULL DEFAULT 1,
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now()
);

-- Contrats de location : un renouvellement crée un nouveau contrat et
-- conserve le précédent.
CREATE TABLE vehicle_leases (
    id           uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    vehicle_id   uuid NOT NULL REFERENCES vehicles(id),
    lessor       text NOT NULL,
    contract_no  text,
    start_date   date,
    end_date     date,
    status       text NOT NULL DEFAULT 'en_cours' CHECK (status IN ('en_cours', 'renouvele', 'restitue')),
    returned_on  date,
    created_by   uuid REFERENCES users(id),
    created_at   timestamptz NOT NULL DEFAULT now(),
    CHECK (end_date IS NULL OR start_date IS NULL OR end_date >= start_date),
    CHECK ((status = 'restitue') = (returned_on IS NOT NULL))
);
CREATE UNIQUE INDEX vehicle_leases_current_uq ON vehicle_leases (vehicle_id) WHERE status = 'en_cours';

-- Assurance, contrôle technique… : chaque renouvellement est une nouvelle
-- pièce ; la plus récente de chaque nature fait foi.
CREATE TABLE vehicle_documents (
    id           uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    vehicle_id   uuid NOT NULL REFERENCES vehicles(id),
    kind         text NOT NULL CHECK (kind IN ('assurance', 'controle_technique', 'carte_grise', 'vignette', 'autre')),
    reference    text,
    valid_from   date,
    valid_until  date,
    note         text,
    created_by   uuid REFERENCES users(id),
    created_at   timestamptz NOT NULL DEFAULT now(),
    CHECK (valid_until IS NULL OR valid_from IS NULL OR valid_until >= valid_from)
);
CREATE INDEX vehicle_documents_vehicle_idx ON vehicle_documents (vehicle_id, kind, created_at);

-- Interventions et prochaines échéances (date et/ou kilométrage) issues
-- des pièces réelles ; aucune périodicité supposée.
CREATE TABLE maintenance_events (
    id             uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    vehicle_id     uuid NOT NULL REFERENCES vehicles(id),
    kind           text NOT NULL CHECK (kind IN ('vidange', 'entretien', 'reparation', 'pneus', 'autre')),
    done_on        date NOT NULL,
    odometer_km    integer CHECK (odometer_km IS NULL OR odometer_km >= 0),
    description    text,
    provider       text,
    next_due_date  date,
    next_due_km    integer CHECK (next_due_km IS NULL OR next_due_km >= 0),
    created_by     uuid REFERENCES users(id),
    created_at     timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX maintenance_events_vehicle_idx ON maintenance_events (vehicle_id, kind, done_on);

CREATE SEQUENCE vehicle_movement_ref;

-- Remise, restitution (au retour d'un salarié) et retour au loueur sont
-- des mouvements distincts, horodatés, avec kilométrage et état.
CREATE TABLE vehicle_movements (
    id                  uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    reference           text NOT NULL UNIQUE,
    vehicle_id          uuid NOT NULL REFERENCES vehicles(id),
    kind                text NOT NULL CHECK (kind IN ('remise', 'restitution', 'retour_loueur', 'reprise')),
    employee_id         uuid REFERENCES employees(id),
    case_id             uuid REFERENCES cases(id),
    task_id             uuid REFERENCES tasks(id),
    effective_at        timestamptz NOT NULL,
    odometer_km         integer CHECK (odometer_km IS NULL OR odometer_km >= 0),
    odometer_reason     text,
    condition           text,
    keys_count          integer CHECK (keys_count IS NULL OR keys_count >= 0),
    fuel_level          text,
    note                text,
    operator_id         uuid NOT NULL REFERENCES users(id),
    origin_movement_id  uuid REFERENCES vehicle_movements(id),
    created_at          timestamptz NOT NULL DEFAULT now(),
    CHECK (kind = 'retour_loueur' OR employee_id IS NOT NULL)
);
CREATE INDEX vehicle_movements_vehicle_idx ON vehicle_movements (vehicle_id, effective_at);
CREATE INDEX vehicle_movements_employee_idx ON vehicle_movements (employee_id);

CREATE TABLE vehicle_assignments (
    id           uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    vehicle_id   uuid NOT NULL REFERENCES vehicles(id),
    employee_id  uuid NOT NULL REFERENCES employees(id),
    handover_id  uuid NOT NULL UNIQUE REFERENCES vehicle_movements(id),
    return_id    uuid REFERENCES vehicle_movements(id),
    started_at   timestamptz NOT NULL,
    ended_at     timestamptz,
    CHECK (return_id IS NULL OR ended_at IS NOT NULL)
);
-- Un seul titulaire à la fois.
CREATE UNIQUE INDEX vehicle_assignments_active_uq ON vehicle_assignments (vehicle_id) WHERE ended_at IS NULL;
CREATE INDEX vehicle_assignments_employee_idx ON vehicle_assignments (employee_id) WHERE ended_at IS NULL;

-- Alertes d'échéance déjà émises (véhicules, logiciels) : une seule par
-- pièce et par seuil, même après redémarrage du worker.
CREATE TABLE alerts_sent (
    kind       text NOT NULL,
    entity_id  uuid NOT NULL,
    due_key    text NOT NULL,
    sent_at    timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (kind, entity_id, due_key)
);
