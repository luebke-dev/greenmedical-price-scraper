ALTER TABLE pharmacies
    ADD COLUMN provider TEXT NOT NULL DEFAULT 'greenmedical'
    CHECK (provider IN ('greenmedical', 'ansay'));

CREATE INDEX pharmacies_provider_idx ON pharmacies (provider);
