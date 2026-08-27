-- Price-alert subscriptions (docs/api-contract.md, "Erweiterung: Preisalarm-Abos").

CREATE EXTENSION IF NOT EXISTS citext;

CREATE TABLE subscribers (
    id                   BIGSERIAL PRIMARY KEY,
    email                CITEXT NOT NULL UNIQUE,
    confirmed_at         TIMESTAMPTZ,
    -- 32 random bytes, base64url; the manage token also unsubscribes.
    confirm_token        TEXT NOT NULL UNIQUE,
    manage_token         TEXT NOT NULL UNIQUE,
    created_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_notified_run_id BIGINT REFERENCES scrape_runs (id) ON DELETE SET NULL
);

-- Unconfirmed subscribers are deleted after 7 days (scheduler tick).
CREATE INDEX subscribers_unconfirmed_created_idx ON subscribers (created_at) WHERE confirmed_at IS NULL;

CREATE TABLE subscription_rules (
    id            BIGSERIAL PRIMARY KEY,
    subscriber_id BIGINT NOT NULL REFERENCES subscribers (id) ON DELETE CASCADE,
    kind          TEXT NOT NULL CHECK (kind IN ('strain_available', 'strain_price_below', 'any_price_below',
                                               'thc_above', 'new_strain', 'strain_price_change')),
    strain_id     BIGINT REFERENCES strains (id) ON DELETE CASCADE,
    threshold     NUMERIC(8, 2),
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- NULLS NOT DISTINCT so that e.g. two `new_strain` rules of one subscriber collide.
    UNIQUE NULLS NOT DISTINCT (subscriber_id, kind, strain_id, threshold)
);

CREATE INDEX subscription_rules_subscriber_idx ON subscription_rules (subscriber_id);

-- One row per triggered event; the UNIQUE dedupes re-evaluations of a run.
CREATE TABLE notifications (
    id            BIGSERIAL PRIMARY KEY,
    subscriber_id BIGINT NOT NULL REFERENCES subscribers (id) ON DELETE CASCADE,
    run_id        BIGINT NOT NULL REFERENCES scrape_runs (id) ON DELETE CASCADE,
    rule_id       BIGINT NOT NULL REFERENCES subscription_rules (id) ON DELETE CASCADE,
    strain_id     BIGINT REFERENCES strains (id) ON DELETE CASCADE,
    payload       JSONB NOT NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    sent_at       TIMESTAMPTZ,
    error         TEXT,
    UNIQUE NULLS NOT DISTINCT (rule_id, strain_id, run_id)
);

CREATE INDEX notifications_subscriber_run_idx ON notifications (subscriber_id, run_id);
