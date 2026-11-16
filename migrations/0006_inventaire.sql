-- 0006 — Campagnes d'inventaire QR (M03, U3). Un pointage n'est jamais
-- une affectation : il ne modifie ni la garde ni l'emplacement d'un actif.

CREATE SEQUENCE inventory_ref;

CREATE TABLE inventory_campaigns (
    id            uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    reference     text NOT NULL UNIQUE,
    name          text NOT NULL,
    site_id       uuid NOT NULL REFERENCES sites(id),
    -- Périmètre : tout le site, ou un seul local.
    location_id   uuid REFERENCES locations(id),
    status        text NOT NULL DEFAULT 'ouverte' CHECK (status IN ('ouverte', 'close')),
    -- Heure de référence du périmètre figé.
    reference_at  timestamptz NOT NULL DEFAULT now(),
    created_by    uuid NOT NULL REFERENCES users(id),
    created_at    timestamptz NOT NULL DEFAULT now(),
    closed_at     timestamptz,
    closed_by     uuid REFERENCES users(id),
    close_note    text,
    CHECK ((status = 'close') = (closed_at IS NOT NULL))
);

-- Photographie initiale, reproductible : actif, emplacement attendu,
-- détenteur et état au moment de la référence.
CREATE TABLE inventory_expected (
    campaign_id           uuid NOT NULL REFERENCES inventory_campaigns(id),
    asset_id              uuid NOT NULL REFERENCES assets(id),
    expected_location_id  uuid REFERENCES locations(id),
    holder_employee_id    uuid REFERENCES employees(id),
    asset_status          text NOT NULL,
    PRIMARY KEY (campaign_id, asset_id)
);

CREATE TABLE inventory_observations (
    id            uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    campaign_id   uuid NOT NULL REFERENCES inventory_campaigns(id),
    -- Clé de commande : un scan rejoué (réseau coupé) ne double rien.
    command_key   text NOT NULL,
    scanned_code  text NOT NULL,
    asset_id      uuid REFERENCES assets(id),
    location_id   uuid NOT NULL REFERENCES locations(id),
    observed_at   timestamptz NOT NULL DEFAULT now(),
    operator_id   uuid NOT NULL REFERENCES users(id),
    manual        boolean NOT NULL DEFAULT false,
    result        text NOT NULL CHECK (result IN ('vu', 'ailleurs', 'hors_perimetre', 'inconnu', 'doublon')),
    -- Pointage utile (compté) ; un doublon est conservé mais non compté.
    counted       boolean NOT NULL,
    UNIQUE (campaign_id, command_key)
);
-- Un seul pointage utile par actif, par local et par campagne.
CREATE UNIQUE INDEX inventory_observations_useful_uq
    ON inventory_observations (campaign_id, asset_id, location_id) WHERE counted AND asset_id IS NOT NULL;
CREATE INDEX inventory_observations_campaign_idx ON inventory_observations (campaign_id, observed_at);

-- Arbitrage d'un écart, distinct de toute rectification du parc.
CREATE TABLE inventory_arbitrations (
    id              uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    campaign_id     uuid NOT NULL REFERENCES inventory_campaigns(id),
    asset_id        uuid REFERENCES assets(id),
    observation_id  uuid REFERENCES inventory_observations(id),
    decision        text NOT NULL CHECK (decision IN ('a_rechercher', 'explique', 'parc_rectifie')),
    note            text NOT NULL,
    decided_by      uuid NOT NULL REFERENCES users(id),
    decided_at      timestamptz NOT NULL DEFAULT now(),
    CHECK (asset_id IS NOT NULL OR observation_id IS NOT NULL)
);
