# GreenMedical – gemeinsamer Vertrag Backend ↔ Frontend ↔ Deployment

Dieses Dokument ist die **einzige Wahrheit** für JSON-Shapes, Env-Variablen, Ports und Namen.
Backend (Rust), Frontend (Quasar) und Helm-Chart müssen sich exakt daran halten.

## Ports / Namen

| Was | Wert |
|---|---|
| Backend HTTP (API + `/healthz` + `/readyz`) | `HTTP_BIND=0.0.0.0:8080` |
| Backend Prometheus | `METRICS_BIND=0.0.0.0:9090`, Pfad `/metrics` |
| Frontend nginx (unprivileged) | Container-Port `8080`, Service-Port `80` |
| Frontend → Backend Proxy | nginx `location ^~ /api/ { proxy_pass ${BACKEND_URL}; }`, Env `BACKEND_URL=http://<backend-svc>:8080` |
| Backend-Image | `ghcr.io/luebke-dev/greenmedical-backend` |
| Frontend-Image | `ghcr.io/luebke-dev/greenmedical-frontend` |
| Helm-Chart (OCI) | `oci://ghcr.io/luebke-dev/charts/greenmedical` |
| Backend-Binary / CLI | `greenmedical-backend serve` (Default) \| `scrape-once [--reviews-only]` \| `migrate` |
| Frontend-Route Sortendetail | `/sorte/:id` (`id` = Integer-DB-ID) |

## Backend Env-Variablen

| Variable | Default | Bedeutung |
|---|---|---|
| `DATABASE_URL` | – (Pflicht) | `postgres://user:pw@host:5432/db` |
| `DATABASE_MAX_CONNECTIONS` | `10` | sqlx Pool (≥ 4, ein Lauf hält eine Connection für den Advisory-Lock) |
| `HTTP_BIND` | `0.0.0.0:8080` | |
| `METRICS_BIND` | `0.0.0.0:9090` | |
| `HTTP_REQUEST_TIMEOUT` | `30s` | serverseitiges Timeout pro Request → `408` |
| `SNAPSHOT_REVALIDATE_INTERVAL` | `30s` | Revalidierung des Snapshot-Caches gegen die DB (erkennt neuere Läufe anderer Replikate; `0s` = bei jedem Request) |
| `CORS_ALLOWED_ORIGINS` | leer | kommagetrennt; leer = kein CORS-Header |
| `LOG_FORMAT` | `json` | `json` \| `pretty` |
| `RUST_LOG` | `info,sqlx=warn` | |
| `MIGRATE_ON_STARTUP` | `true` | |
| `SCRAPE_ENABLED` | `true` | Scheduler an/aus (API läuft immer) |
| `SCRAPE_CRON` | `0 0 4,10,16,22 * * *` | `cron`-Crate-Format: sec min hour dom mon dow |
| `SCRAPE_TIMEZONE` | `Europe/Berlin` | IANA |
| `SCRAPE_BOOTSTRAP` | `true` | beim Start scrapen wenn kein usable Run oder älter als … |
| `SCRAPE_BOOTSTRAP_MAX_AGE` | `8h` | |
| `SCRAPE_STALE_RUN_AFTER` | `2h` | `running`-Runs älter → `failed` |
| `SCRAPE_BASE_URL` | `https://greenmedical.health` | für Tests auf wiremock umbiegbar |
| `SCRAPE_USER_AGENT` | Firefox-UA wie in `scraper.py` | |
| `SCRAPE_REQUEST_TIMEOUT` | `30s` | |
| `SCRAPE_RETRY_TOTAL` | `4` | |
| `SCRAPE_BACKOFF_FACTOR` | `1.0` | Sleeps 0,2,4,8 s |
| `SCRAPE_PHARMACY_DELAY` | `300ms` | |
| `SCRAPE_PAGE_DELAY` | `500ms` | |
| `SCRAPE_MIN_SUCCESS_RATIO` | `0.5` | |
| `REVIEWS_ENABLED` | `true` | Phase 2 (Bewertungen von Produktseiten) an/aus |
| `REVIEWS_MAX_AGE` | `24h` | Sorten mit jüngerem `reviews_scraped_at` werden in Phase 2 übersprungen |
| `REVIEWS_MAX_PER_RUN` | `0` | max. Produktseiten pro Lauf, `0` = unbegrenzt (älteste zuerst) |
| `ADMIN_TOKEN` | leer | leer ⇒ `POST /api/v1/admin/scrape` antwortet 404 |
| `INSTANCE_NAME` | `$HOSTNAME` | |

## HTTP-API

Basis `/api/v1`. JSON UTF-8. Zeitstempel RFC 3339 UTC (`2026-08-27T08:00:03Z`; ausgegebene Zeitstempel können Sekundenbruchteile enthalten, z. B. `2026-08-27T08:00:03.123456Z` – Clients müssen beides parsen). Zahlen sind JSON-Numbers (nie formatierte Strings), außer den verbatim gescrapten Labels (`thc`, `cbd`, `preis_pro_gramm`).

Fehler: `{"error":{"code":"not_found|bad_request|unauthorized|conflict|no_data|internal","message":"…"}}` mit passendem Status.
Zwei Statuscodes ohne eigenen Code: `405 Method Not Allowed` (bekannter Pfad, falsche Methode) trägt `bad_request`,
`408 Request Timeout` (`HTTP_REQUEST_TIMEOUT` überschritten) trägt `internal`. Clients behandeln `code` als offenen String.

```ts
export type RunStatus = 'running' | 'success' | 'partial' | 'failed';
export type RunTrigger = 'schedule' | 'manual' | 'bootstrap';

export interface Run {
  id: number;
  started_at: string;
  finished_at: string | null;
  status: RunStatus;
  trigger: RunTrigger;
  instance: string | null;
  pharmacies_total: number | null;
  pharmacies_scraped: number | null;
  pharmacies_failed: number | null;
  offer_count: number | null;
  http_requests: number | null;
  error: string | null;
}

export interface RunError { pharmacy_name: string; pharmacy_url: string; stage: 'uuid' | 'pages'; message: string; }

export interface Offer {
  offer_id: number;
  pharmacy_id: number;
  apotheke: string;
  apotheke_plz: string;
  apotheke_stadt: string;
  preis_pro_gramm: string;               // verbatim, z.B. "5,49 €/g"
  preis_eur_pro_gramm: number | null;
  preis_eur_pro_gramm_thc: number | null;
  verfuegbarkeit: string;                // "Auf Lager" | "NEU" | …
  produkt_url: string;
}

export interface Trend {
  reference_run_id: number;
  reference_at: string;                   // = reference_run.started_at (RFC3339 UTC)
  min_price_then: number;
  delta: number;                          // min_price_now - min_price_then
  delta_pct: number;                      // delta / min_price_then * 100
  direction: 'up' | 'down' | 'flat';      // flat wenn |delta| < 0.005
}

export interface Strain {
  id: number;                             // stabile DB-ID
  name: string;
  bezeichnung: string;
  genetik: string;
  thc: string;                            // verbatim "27%", "<1%"
  cbd: string;
  thc_value: number | null;               // geparst ("<1%" → 0.99)
  cbd_value: number | null;
  min_price: number | null;
  min_price_per_thc_gram: number | null;
  pharmacy_count: number;
  offers: Offer[];                        // günstigste zuerst, null-Preise zuletzt
  sort: { price: number | null; price_per_thc_gram: number | null; thc: number | null; cbd: number | null };
  search: string;                         // lowercased Suchtext wie build_site.py
  trend: Trend | null;
}

export interface StrainDetail extends Strain {
  first_seen_at: string;
  last_seen_at: string;
  in_latest_run: boolean;
  run: Run;                               // latest usable run
}

export interface Highlight {
  price: number | null;
  name: string;
  apotheke: string;
  genetik: string;
  thc: string;
  cbd: string;
  produkt_url: string;
  strain_id: number;
  pharmacy_id: number;
}

export interface Metadata {
  generated_at: string;                   // = run.finished_at des neuesten Laufs
  source: string;                         // "https://greenmedical.health/de/cannabis/flowers"
  total: number;                          // Angebote
  pharmacy_count: number;
  strain_count: number;
  lowest_price: number | null;
  cheapest_gram: Highlight | null;
  cheapest_thc_gram: Highlight | null;
  cheapest_cbd_gram: Highlight | null;
  highest_thc: Highlight | null;
  highest_cbd: Highlight | null;
  highest_thc_cbd: Highlight | null;      // max(thc*cbd), Tie-Break günstigster Preis
  run: Run;
}

export interface StrainsResponse { run: Run; reference_run: Run | null; strains: Strain[]; }

export type HistoryBucket = 'run' | 'day';
export interface HistoryPoint {
  run_id?: number;                        // nur bucket=run
  run_count?: number;                     // nur bucket=day
  at: string;                             // run: RFC3339; day: "YYYY-MM-DD" (Europe/Berlin)
  status?: RunStatus;                     // nur bucket=run
  min: number | null;
  avg: number | null;
  max: number | null;
  min_per_thc_gram: number | null;
  avg_per_thc_gram: number | null;
  max_per_thc_gram: number | null;
  offer_count: number;                    // run: Angebote des Laufs; day: gerundeter Ø Angebote pro Lauf des Tages
  pharmacy_count: number;                 // run: Apotheken des Laufs; day: distinct Apotheken über den Tag
}
export interface PharmacySeriesPoint { run_id?: number; at: string; price: number | null; price_per_thc_gram: number | null; availability: string; }
export interface PharmacySeries { pharmacy_id: number; name: string; city: string; points: PharmacySeriesPoint[]; }
export interface History {
  strain_id: number;
  bucket: HistoryBucket;
  from: string;
  to: string;
  timezone: string;
  points: HistoryPoint[];                 // aufsteigend
  pharmacies?: PharmacySeries[];          // nur bei ?pharmacies=true
}

export interface Pharmacy {
  id: number; external_id: string; name: string; plz: string; city: string; address: string; url: string;
  first_seen_at: string; last_seen_at: string; offer_count_latest: number;
}
export interface RunsResponse { runs: Run[]; total: number; }
export interface RunDetail extends Run { errors: RunError[]; }
```

| Methode & Pfad | Antwort / Verhalten |
|---|---|
| `GET /healthz` | `200 {"status":"ok"}`; keine Abhängigkeiten |
| `GET /readyz` | `200 {"status":"ready","db":"ok"}` wenn `SELECT 1` ≤ 2 s und nicht im Shutdown; sonst `503 {"status":"not_ready",…}` |
| `GET /metrics` (Port 9090) | Prometheus-Text |
| `GET /api/v1/metadata` | `Metadata`; `404 no_data` ohne usable Run |
| `GET /api/v1/strains` | `StrainsResponse`; Header `ETag: "run-<id>"`, `Cache-Control: public, max-age=300`; `If-None-Match` → 304; `404 no_data` |
| `GET /api/v1/strains/{id}` | `StrainDetail`; `404 not_found` bei unbekannter ID; `404 no_data` wenn die Sorte existiert, aber kein usable Run vorliegt |
| `GET /api/v1/strains/{id}/history?from=&to=&bucket=run\|day&include_partial=true&pharmacies=false` | `History`; `from`/`to` als vollständige RFC-3339-Zeitstempel (`2026-01-01T00:00:00Z` oder mit URL-kodiertem Offset `%2B02:00`; ein reines Datum wie `2026-01-01` ist `400 bad_request`); Defaults `to=now`, `from=to − 90 d` (also relativ zum übergebenen `to`, nicht zu `now`), `bucket=run`, `include_partial=true`; `from > to` oder Spanne > 730 d → `400 bad_request` (genau 730 d ist erlaubt); `404 not_found` bei unbekannter ID (leere `points` sind OK) |
| `GET /api/v1/runs?limit=50&offset=0&status=` | `RunsResponse` neueste zuerst, `limit` ≤ 500 |
| `GET /api/v1/runs/{id}` | `RunDetail` |
| `GET /api/v1/pharmacies` | `{"pharmacies": Pharmacy[]}` |
| `GET /api/v1/export.csv?run_id=` | `text/csv; charset=utf-8`, `Content-Disposition: attachment; filename="greenmedical_flowers.csv"`; Spalten exakt: `apotheke,apotheke_plz,apotheke_stadt,name,bezeichnung,genetik,thc,cbd,preis_pro_gramm,verfuegbarkeit,produkt_url`; Reihenfolge = Scrape-Reihenfolge |
| `GET /api/v1/export.json?run_id=` | **bare Array** `Strain[]`, `Content-Disposition: attachment; filename="flowers.json"` |
| `POST /api/v1/admin/scrape` | `Authorization: Bearer <ADMIN_TOKEN>`; `202 {"run_id":n,"status":"running"}`; `409 conflict` (`message` „scrape_in_progress“ oder „scrape_locked_elsewhere“); `401 unauthorized`; `404` wenn `ADMIN_TOKEN` leer |

"usable Run" = `status IN ('success','partial')`, neuester nach `started_at`.

## Semantik (Port aus `build_site.py` / `scraper.py`, in Git-Historie: `git show HEAD:build_site.py`)

- `parse_decimal("5,49 €/g") = 5.49` (erste Zahl, `,`→`.`, nbsp normalisiert); `parse_percent("<1%") = 0.99`, `"<0%"` → `0`; `price_per_thc = round2(price / (thc/100))`, `None` wenn thc ≤ 0.
- Sorten-Gruppierung: Key `(lower(clean_text(name)), lower(clean_text(bezeichnung)))`; Gruppenreihenfolge nach Key; Anzeige-Felder = erster nicht-leerer Wert der Mitglieder; Offers nach `(preis is null, preis)`.
- `search` = lowercased Join aus name, bezeichnung, genetik, thc, cbd, je Offer (apotheke, plz, stadt, preis_pro_gramm, verfuegbarkeit) und `"{min_thc_price:.2f} €/g thc"`.
- Highlights: `cheapest_*` = min über Offers; `highest_*` = max, Tie-Break günstigster Preis (null = +∞).
- Trend: Referenz-Run = neuester usable Run mit `started_at <= latest.started_at - 7d`; `null` wenn keiner, oder Sorte/Preis dort fehlt.

## Lokale Test-Infrastruktur (bereits vorhanden)

- Postgres 17 in Podman: Container `gm-pg`, `postgres://greenmedical:greenmedical@localhost:5432/greenmedical` (Superuser ⇒ `sqlx::test` darf DBs anlegen).
- npm: globale Registry `npm.luebke.internal` ist **nicht erreichbar** → immer `npm_config_registry=https://registry.npmjs.org pnpm …` setzen (nicht `~/.npmrc` ändern).
- Cargo/crates.io erreichbar. Verifizierte Versionen: axum 0.8.9, sqlx 0.9.0, reqwest 0.13.4, tokio 1.53, tower-http 0.7.0, scraper 0.27.0, cron 0.17.0, chrono-tz 0.10.4, wiremock 0.6.5, metrics 0.24.6, metrics-exporter-prometheus 0.18.3, arc-swap 1.9.2, constant_time_eq 0.5.0, humantime 2.4.0, unicode-normalization 0.1.25, rstest 0.26.1, clap 4.6.1.
- npm-Versionen: quasar 2.27.0, @quasar/app-vite 3.8.1, vue-echarts 8.1.0, echarts 6.1.0, vitest 4.1.11, happy-dom 20.11.8, @quasar/quasar-app-extension-testing-unit-vitest 3.0.0, pinia 4.0.3, vue-router 5.2.0.
- kubectl-Kontext `openkoder` ist ein fremder Cluster: **niemals** benutzen. Smoke-Tests nur in minikube.

## Erweiterung: Bewertungen (Reviews) pro Sorte

Quelle: Produktseite (`produkt_url` ohne Query/Fragment). Serverseitig gerendert:
- JSON-LD `"aggregateRating":{"ratingValue":"4.3","reviewCount":"124"}` (fehlt bei 0 Bewertungen),
- Header `.pdpReviewsHeaderRating .ratingStars` mit `<span>4.3</span> <span>(124)</span>`,
- je Bewertung `div.pdpReview`: `.pdpReviewName span` (Autor, z. B. „Carlos S."), `.pdpReviewRating .ratingStars i.fullStar|halfStar|emptyStar` (Sterne zählen), `.pdpReviewDate` („25.08.2026"), `.pdpReviewContent p` (Text), optionaler Badge-Text „Verifizierter Kauf",
- Produkt-UUID aus `data-modal-url="/de/cannabis/feedback/modal/<uuid>"`.
Alle Bewertungen stehen auf einer Seite (keine Pagination).

### Scraping
- Zweite Phase eines Scrape-Laufs (nach dem Persistieren der Angebote): für jede Sorte des Laufs, deren `reviews_scraped_at` null oder älter als `REVIEWS_MAX_AGE` ist, Produktseite laden (Delay `SCRAPE_PAGE_DELAY`), parsen, **pro Sorte sofort committen** (abbruchsicher). Fehler pro Sorte werden geloggt/gezählt, machen den Lauf nie `failed`.
- Env: `REVIEWS_ENABLED=true`, `REVIEWS_MAX_AGE=24h`, `REVIEWS_MAX_PER_RUN=0` (0 = unbegrenzt).
- `scrape_runs` erhält `reviews_scraped INT`, `reviews_failed INT` (in `Run` als `reviews_scraped: number|null`, `reviews_failed: number|null`).
- CLI: `scrape-once --reviews-only` (nur Phase 2, ignoriert `REVIEWS_MAX_AGE`).

### Schema
```sql
ALTER TABLE strains ADD COLUMN product_uuid TEXT, ADD COLUMN rating_value NUMERIC(3,1), ADD COLUMN review_count INT,
                    ADD COLUMN reviews_scraped_at TIMESTAMPTZ;
CREATE TABLE strain_rating_snapshots (id BIGSERIAL PK, strain_id FK, run_id FK NULL, scraped_at TIMESTAMPTZ,
                                      rating_value NUMERIC(3,1) NULL, review_count INT NOT NULL);
  idx (strain_id, scraped_at)
CREATE TABLE reviews (id BIGSERIAL PK, strain_id FK, fingerprint TEXT /* sha256(author|reviewed_on|rating|content) */,
                      author TEXT, reviewed_on DATE NULL, rating SMALLINT /*0-5, halbe Sterne aufgerundet? nein: 0.5-Schritte als NUMERIC(2,1)*/,
                      verified BOOL, content TEXT, first_seen_at, last_seen_at, UNIQUE(strain_id, fingerprint));
```
`rating` als `NUMERIC(2,1)` (0.0–5.0, halbe Sterne möglich).

### API
```ts
export interface Rating { value: number | null; count: number; scraped_at: string; }
// Strain / StrainDetail erhalten:
//   rating: Rating | null            (null = noch nie gescrapt)
//   sort.rating: number | null       (= rating.value)
//   product_uuid: string | null
// Highlight erhält optional: rating_value?: number | null; review_count?: number
// Metadata erhält: best_rated: Highlight | null   (höchster rating_value bei review_count >= 5, Tie-Break mehr Bewertungen; Highlight.price = min_price)

export interface Review { id: number; author: string; reviewed_on: string | null; rating: number; verified: boolean;
                          content: string; first_seen_at: string; }
export interface ReviewsResponse {
  strain_id: number;
  summary: { value: number | null; count: number; scraped_at: string | null;
             distribution: { '1': number; '2': number; '3': number; '4': number; '5': number }; // gerundet auf ganze Sterne, aus `reviews`
             verified_count: number; stored_count: number };
  history: { at: string; value: number | null; count: number }[];   // strain_rating_snapshots aufsteigend, max 400
  reviews: Review[];
  total: number;
}
```
| Endpoint | Antwort |
|---|---|
| `GET /api/v1/strains/{id}/reviews?limit=50&offset=0&sort=newest\|oldest\|highest\|lowest` | `ReviewsResponse`; `limit` ≤ 500; 404 `not_found` bei unbekannter Sorte; leere Listen wenn nie gescrapt |
`search` der Sorte bleibt unverändert (Reviews nicht im Suchtext).

### Implementierungsentscheidungen (Backend)
- Phase 2 startet, **nachdem** der Lauf abgeschlossen ist (`status`/`finished_at` gesetzt, Angebote committet): der Lauf ist während des Bewertungs-Scrapes bereits der „latest usable Run“, und `SCRAPE_STALE_RUN_AFTER` kann einen langen Bewertungsdurchlauf nicht als hängenden Lauf markieren. In-Process-Gate und Advisory-Lock bleiben bis zum Ende von Phase 2 gehalten; Phase 2 ändert nie den `status`, nur `reviews_scraped`/`reviews_failed`. Bei `status = failed` oder Shutdown entfällt Phase 2 (`reviews_*` bleiben `null`).
- Auswahl: Sorten des Laufs mit `reviews_scraped_at IS NULL OR < now() − REVIEWS_MAX_AGE`, älteste zuerst, begrenzt durch `REVIEWS_MAX_PER_RUN`. Geladen wird die erste `produkt_url` der Sorte im Lauf ohne Query/Fragment.
- `scrape-once --reviews-only` nimmt denselben Gate/Lock, ignoriert `REVIEWS_MAX_AGE`, beachtet `REVIEWS_MAX_PER_RUN` und **überschreibt** `reviews_scraped`/`reviews_failed` des neuesten usable Runs; Snapshots erhalten dessen `run_id`.
- Fingerprint = hex-SHA-256 von `author|reviewed_on|rating|content` mit `reviewed_on` als ISO-Datum (leer wenn fehlt) und `rating` mit einer Nachkommastelle; `verified` ist nicht Teil des Fingerprints und wird beim Upsert aktualisiert (`last_seen_at = now()`).
- `rating_value` ist `null` bei `review_count = 0`, auch wenn die Seite einen Wert zeigt. `product_uuid` wird nur überschrieben, wenn die Seite eine UUID liefert.
- `summary.distribution` rundet kaufmännisch (`4.5 → 5`, `2.5 → 3`). `history` sind die **neuesten 400** Snapshots, aufsteigend sortiert.
- Sortierung: `newest` = `reviewed_on DESC NULLS LAST, id DESC`; `oldest` = `reviewed_on ASC NULLS LAST, id ASC`; `highest`/`lowest` = `rating DESC|ASC`, dann wie `newest`. `limit` außerhalb `1..=500`, negatives `offset` oder unbekanntes `sort` → `400 bad_request`.
- `Highlight.rating_value`/`review_count` werden nur bei `best_rated` serialisiert (bei den übrigen Highlights fehlen die Felder). Metrik: `scrape_reviews_total{result="scraped"|"failed"}`.

### Frontend
- Übersicht: Spalte „Bewertung" (★ 4,3 · 124) nach „Apotheken", sortierbar (nulls zuletzt); Kennzahl-Karte „Bestbewertet" (10. Karte).
- Sortenseite: Abschnitt „Bewertungen" zwischen Preisverlauf und Aktuelle Angebote: Zusammenfassung (Ø Sterne, Anzahl, verifiziert-Anteil, Verteilung 5→1 als Balken), Sortierung (neueste/älteste/beste/schlechteste), Liste (Autor, Datum, Sterne, Text, Badge „Verifizierter Kauf"), „Mehr anzeigen" (50er-Schritte, offset-basiert).
