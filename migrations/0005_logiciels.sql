-- 0005 — Abonnements et licences logiciels (M03 IT) : sièges achetés,
-- attribués et disponibles. Un siège n'est jamais matérialisé par un QR.

CREATE TABLE software_subscriptions (
    id            uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    vendor        text NOT NULL,
    product       text NOT NULL,
    offer         text,
    contract_ref  text,
    license_kind  text NOT NULL CHECK (license_kind IN ('abonnement', 'perpetuelle')),
    start_date    date,
    end_date      date,
    auto_renew    boolean NOT NULL DEFAULT false,
    seats         integer NOT NULL CHECK (seats >= 0),
    -- Un siège rendu redevient disponible selon le contrat.
    seat_release  text NOT NULL DEFAULT 'immediat' CHECK (seat_release IN ('immediat', 'fin_periode', 'non_reattribuable')),
    notes         text,
    active        boolean NOT NULL DEFAULT true,
    version       integer NOT NULL DEFAULT 1,
    created_at    timestamptz NOT NULL DEFAULT now(),
    updated_at    timestamptz NOT NULL DEFAULT now(),
    -- Une licence perpétuelle ne reçoit pas de fausse date de renouvellement.
    CHECK (license_kind <> 'perpetuelle' OR (end_date IS NULL AND NOT auto_renew)),
    CHECK (end_date IS NULL OR start_date IS NULL OR end_date >= start_date)
);
CREATE UNIQUE INDEX software_subscriptions_uq ON software_subscriptions (lower(vendor), lower(product), coalesce(lower(contract_ref), ''));

-- Un siège est occupé tant que l'attribution est active, puis jusqu'à
-- `seat_free_at` (fin de période) ; jamais libéré si `seat_free_at` est vide
-- pour une licence non réattribuable.
CREATE TABLE license_allocations (
    id               uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    subscription_id  uuid NOT NULL REFERENCES software_subscriptions(id),
    employee_id      uuid NOT NULL REFERENCES employees(id),
    case_id          uuid REFERENCES cases(id),
    started_at       timestamptz NOT NULL DEFAULT now(),
    ended_at         timestamptz,
    seat_free_at     timestamptz,
    assigned_by      uuid NOT NULL REFERENCES users(id),
    released_by      uuid REFERENCES users(id),
    note             text,
    release_note     text,
    CHECK (ended_at IS NOT NULL OR seat_free_at IS NULL)
);
CREATE UNIQUE INDEX license_allocations_active_uq ON license_allocations (subscription_id, employee_id) WHERE ended_at IS NULL;
CREATE INDEX license_allocations_employee_idx ON license_allocations (employee_id) WHERE ended_at IS NULL;
