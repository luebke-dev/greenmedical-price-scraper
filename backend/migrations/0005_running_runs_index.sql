-- `Metadata.scrape_running` asks on every request whether a run is in progress;
-- `mark_stale` scans the same rows. Both are served by this tiny partial index.
CREATE INDEX scrape_runs_running_idx
    ON scrape_runs (started_at)
    WHERE status = 'running';
