-- Initial schema for the GreenMedical price scraper backend.
--
-- One scrape run stores every offer it saw (offers), linked to stable
-- pharmacy and strain identities. Strains are identified by the normalised
-- (name, bezeichnung) pair; per-pharmacy attributes (THC/CBD/genetics/URL)
-- live on the offer because they vary between pharmacies.

CREATE TABLE scrape_runs (
    id                 BIGSERIAL PRIMARY KEY,
    started_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    finished_at        TIMESTAMPTZ,
    status             TEXT NOT NULL CHECK (status IN ('running', 'success', 'partial', 'failed')),
    trigger            TEXT NOT NULL CHECK (trigger IN ('schedule', 'manual', 'bootstrap')),
    instance           TEXT,
    pharmacies_total   INT,
    pharmacies_scraped INT,
    pharmacies_failed  INT,
    offer_count        INT,
    http_requests      INT,
    error              TEXT
);

-- "Usable" runs are looked up by recency very frequently (snapshot cache, trend reference).
CREATE INDEX scrape_runs_usable_started_at_idx
    ON scrape_runs (started_at DESC)
    WHERE status IN ('success', 'partial');

CREATE TABLE scrape_run_errors (
    id            BIGSERIAL PRIMARY KEY,
    run_id        BIGINT NOT NULL REFERENCES scrape_runs (id) ON DELETE CASCADE,
    pharmacy_name TEXT NOT NULL,
    pharmacy_url  TEXT NOT NULL,
    stage         TEXT NOT NULL CHECK (stage IN ('uuid', 'pages')),
    message       TEXT NOT NULL
);

CREATE INDEX scrape_run_errors_run_id_idx ON scrape_run_errors (run_id);

CREATE TABLE pharmacies (
    id            BIGSERIAL PRIMARY KEY,
    -- UUID taken from the "pharmacyAvailability" query parameter on the detail page.
    external_id   TEXT NOT NULL UNIQUE,
    name          TEXT NOT NULL,
    plz           TEXT NOT NULL,
    city          TEXT NOT NULL,
    address       TEXT NOT NULL,
    url           TEXT NOT NULL,
    first_seen_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_seen_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE strains (
    id               BIGSERIAL PRIMARY KEY,
    -- lower(clean_text(...)) of name / bezeichnung; the grouping identity (API contract).
    name_key         TEXT NOT NULL,
    bezeichnung_key  TEXT NOT NULL,
    name             TEXT NOT NULL,
    bezeichnung      TEXT NOT NULL,
    genetik          TEXT NOT NULL DEFAULT '',
    thc_label        TEXT NOT NULL DEFAULT '',
    cbd_label        TEXT NOT NULL DEFAULT '',
    first_seen_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_seen_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (name_key, bezeichnung_key)
);

CREATE TABLE offers (
    id              BIGSERIAL PRIMARY KEY,
    run_id          BIGINT NOT NULL REFERENCES scrape_runs (id) ON DELETE CASCADE,
    pharmacy_id     BIGINT NOT NULL REFERENCES pharmacies (id),
    strain_id       BIGINT NOT NULL REFERENCES strains (id),
    -- Scrape order within the run; export.csv reproduces it.
    position        INT NOT NULL,
    genetik         TEXT NOT NULL DEFAULT '',
    thc_label       TEXT NOT NULL DEFAULT '',
    cbd_label       TEXT NOT NULL DEFAULT '',
    price_label     TEXT NOT NULL DEFAULT '',
    availability    TEXT NOT NULL DEFAULT '',
    product_url     TEXT NOT NULL DEFAULT '',
    price_eur       NUMERIC(8, 2),
    thc_pct         NUMERIC(6, 2),
    cbd_pct         NUMERIC(6, 2),
    price_per_thc_g NUMERIC(10, 2),
    price_per_cbd_g NUMERIC(10, 2)
);

-- No UNIQUE (run_id, pharmacy_id, strain_id): pharmacies do list the same strain twice.
CREATE INDEX offers_run_position_idx ON offers (run_id, position);
CREATE INDEX offers_strain_run_idx ON offers (strain_id, run_id);
CREATE INDEX offers_pharmacy_strain_run_idx ON offers (pharmacy_id, strain_id, run_id);
