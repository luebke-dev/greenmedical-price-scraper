ALTER TABLE strains RENAME COLUMN bezeichnung_key TO designation_key;
ALTER TABLE strains RENAME COLUMN bezeichnung TO designation;
ALTER TABLE strains RENAME COLUMN genetik TO genetics;
ALTER TABLE offers RENAME COLUMN genetik TO genetics;
ALTER TABLE pharmacies RENAME COLUMN plz TO postal_code;
