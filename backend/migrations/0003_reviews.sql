-- Per-strain reviews scraped from the product page (docs/api-contract.md,
-- "Erweiterung: Bewertungen").

ALTER TABLE strains
    ADD COLUMN product_uuid       TEXT,
    ADD COLUMN rating_value       NUMERIC(3, 1),
    ADD COLUMN review_count       INT,
    ADD COLUMN reviews_scraped_at TIMESTAMPTZ;

-- One row per (strain, review scrape): history of the aggregate rating.
CREATE TABLE strain_rating_snapshots (
    id           BIGSERIAL PRIMARY KEY,
    strain_id    BIGINT NOT NULL REFERENCES strains (id) ON DELETE CASCADE,
    run_id       BIGINT REFERENCES scrape_runs (id) ON DELETE SET NULL,
    scraped_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    rating_value NUMERIC(3, 1),
    review_count INT NOT NULL
);

CREATE INDEX strain_rating_snapshots_strain_scraped_idx
    ON strain_rating_snapshots (strain_id, scraped_at);

-- Individual reviews, deduplicated per strain by a content fingerprint
-- (sha256 of "author|reviewed_on|rating|content").
CREATE TABLE reviews (
    id            BIGSERIAL PRIMARY KEY,
    strain_id     BIGINT NOT NULL REFERENCES strains (id) ON DELETE CASCADE,
    fingerprint   TEXT NOT NULL,
    author        TEXT NOT NULL,
    reviewed_on   DATE,
    rating        NUMERIC(2, 1) NOT NULL,
    verified      BOOLEAN NOT NULL DEFAULT false,
    content       TEXT NOT NULL DEFAULT '',
    first_seen_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_seen_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (strain_id, fingerprint)
);

CREATE INDEX reviews_strain_reviewed_on_idx ON reviews (strain_id, reviewed_on DESC);

ALTER TABLE scrape_runs
    ADD COLUMN reviews_scraped INT,
    ADD COLUMN reviews_failed  INT;
