-- 0007 — Mutation (M05) et départ (M06) : comparaison figée, application
-- unique à échéance, contrôles de sortie, coupures confirmées par l'IT.

-- ------------------------------------------------------------ tâches à horaire
-- Une coupure devient exigible à un instant précis ; une relance est émise
-- si elle n'est pas confirmée à temps.
ALTER TABLE tasks
    ADD COLUMN not_before    timestamptz,
    ADD COLUMN due_at        timestamptz,
    ADD COLUMN escalated_at  timestamptz;

-- ------------------------------------------------------------ mutation
CREATE TABLE transfers (
    id                          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    case_id                     uuid NOT NULL UNIQUE REFERENCES cases(id),
    employee_id                 uuid NOT NULL REFERENCES employees(id),
    -- Situation comparée : l'application est suspendue si elle a changé.
    from_position_id            uuid NOT NULL REFERENCES employee_positions(id),
    target_job_title            text NOT NULL,
    target_site_id              uuid NOT NULL REFERENCES sites(id),
    target_department           text,
    target_manager              text,
    target_manager_employee_id  uuid REFERENCES employees(id),
    target_profile_id           uuid REFERENCES job_profiles(id),
    effective_date              date NOT NULL,
    status                      text NOT NULL DEFAULT 'brouillon'
                                CHECK (status IN ('brouillon', 'confirmee', 'appliquee', 'suspendue', 'annulee')),
    snapshot                    jsonb NOT NULL,
    conflict                    jsonb,
    confirmed_at                timestamptz,
    confirmed_by                uuid REFERENCES users(id),
    applied_at                  timestamptz,
    -- Application unique : une seule nouvelle situation par mutation.
    applied_position_id         uuid UNIQUE REFERENCES employee_positions(id),
    cancelled_at                timestamptz,
    cancel_reason               text,
    version                     integer NOT NULL DEFAULT 1,
    created_at                  timestamptz NOT NULL DEFAULT now(),
    updated_at                  timestamptz NOT NULL DEFAULT now(),
    CHECK ((status = 'appliquee') = (applied_position_id IS NOT NULL))
);
CREATE UNIQUE INDEX transfers_active_uq ON transfers (employee_id) WHERE status IN ('brouillon', 'confirmee', 'suspendue');

CREATE TABLE transfer_deltas (
    id           uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    transfer_id  uuid NOT NULL REFERENCES transfers(id),
    -- Clé stable de l'objet comparé (dédoublonne les tâches au recalcul).
    object_key   text NOT NULL,
    object_kind  text NOT NULL CHECK (object_kind IN ('compte', 'licence', 'bien', 'vehicule', 'formation', 'besoin')),
    object_ref   uuid,
    category     text,
    label        text NOT NULL,
    current      text,
    target       text,
    proposal     text NOT NULL CHECK (proposal IN ('conserver', 'ajouter', 'reprendre', 'verifier')),
    decision     text NOT NULL CHECK (decision IN ('conserver', 'ajouter', 'reprendre', 'verifier')),
    note         text,
    UNIQUE (transfer_id, object_key)
);

-- ------------------------------------------------------------ départ
CREATE TABLE departures (
    id                  uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    case_id             uuid NOT NULL UNIQUE REFERENCES cases(id),
    employee_id         uuid NOT NULL REFERENCES employees(id),
    reason              text NOT NULL CHECK (reason IN ('demission', 'fin_contrat', 'rupture', 'retraite', 'autre')),
    -- Pièce ou motif privé : RH et ADMIN uniquement.
    private_note        text,
    last_day            date NOT NULL,
    -- Instant de coupure convenu (UTC, affiché en heure d'Alger).
    cutoff_at           timestamptz NOT NULL,
    hr_contact          text,
    status              text NOT NULL DEFAULT 'planifie' CHECK (status IN ('planifie', 'sortie_effective', 'annule')),
    cutoff_notified_at  timestamptz,
    escalated_at        timestamptz,
    exit_applied_at     timestamptz,
    cancelled_at        timestamptz,
    cancel_reason       text,
    version             integer NOT NULL DEFAULT 1,
    created_at          timestamptz NOT NULL DEFAULT now(),
    updated_at          timestamptz NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX departures_planned_uq ON departures (employee_id) WHERE status = 'planifie';

-- Ce qui est réellement confié au salarié, figé au départ puis rapproché.
CREATE TABLE exit_controls (
    id                     uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    departure_id           uuid NOT NULL REFERENCES departures(id),
    kind                   text NOT NULL CHECK (kind IN ('compte', 'licence', 'bien', 'vehicule', 'donnees', 'autre')),
    account_id             uuid REFERENCES employee_accounts(id),
    license_allocation_id  uuid REFERENCES license_allocations(id),
    assignment_id          uuid REFERENCES assignments(id),
    vehicle_assignment_id  uuid REFERENCES vehicle_assignments(id),
    label                  text NOT NULL,
    detail                 text,
    state                  text NOT NULL DEFAULT 'a_traiter' CHECK (state IN ('a_traiter', 'fait', 'ecart', 'perte', 'non_applicable')),
    -- Heure réelle confirmée par l'opérateur (≠ heure demandée).
    done_at                timestamptz,
    confirmed_by           uuid REFERENCES users(id),
    system                 text,
    note                   text,
    follow_up_date         date,
    created_at             timestamptz NOT NULL DEFAULT now(),
    updated_at             timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX exit_controls_departure_idx ON exit_controls (departure_id);

ALTER TABLE tasks
    ADD COLUMN exit_control_id    uuid REFERENCES exit_controls(id),
    ADD COLUMN transfer_delta_id  uuid REFERENCES transfer_deltas(id);
