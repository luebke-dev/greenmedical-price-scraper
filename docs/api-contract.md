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
| `SCRAPE_CRON` | `0 0 * * * *` | `cron`-Crate-Format: sec min hour dom mon dow |
| `SCRAPE_TIMEZONE` | `Europe/Berlin` | IANA |
| `SCRAPE_BOOTSTRAP` | `true` | beim Start scrapen wenn kein usable Run oder älter als … |
| `SCRAPE_BOOTSTRAP_MAX_AGE` | `2h` | |
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

## Erweiterung: serverseitige Paginierung der Sortenliste

`GET /api/v1/strains` wird paginiert, gefiltert und sortiert **serverseitig** (im Backend über den Snapshot des
letzten Laufs, nicht per SQL). Die Liste enthält **keine `offers` und kein `search`** mehr (Angebote gibt es auf
`GET /api/v1/strains/{id}`); `GET /api/v1/export.json` bleibt das vollständige Array inkl. `offers`.

### Query-Parameter
| Parameter | Default | Semantik |
|---|---|---|
| `q` | – | Volltext: lowercased Substring-Match auf dem bisherigen `search`-Text |
| `genetik` | – | kommagetrennt, case-insensitiv; Sorte muss eine der Werte haben |
| `price_min`, `price_max` | – | inkl. Grenzen auf `sort.price`; Sorten ohne Wert nur wenn **beide** fehlen |
| `thc_min`, `thc_max` | – | dito auf `sort.thc` |
| `cbd_min`, `cbd_max` | – | dito auf `sort.cbd` |
| `rating_min` | – | `sort.rating >= rating_min`; ohne Wert nur wenn Parameter fehlt |
| `sort` | `price` | `price \| price_per_thc_gram \| thc \| cbd \| pharmacy_count \| rating \| name \| bezeichnung \| genetik` |
| `dir` | `asc` | `asc \| desc`; numerische Nulls **immer zuletzt**; Text mit deutscher, case-insensitiver, numerischer Sortierung (wie `Intl.Collator('de', {numeric:true, sensitivity:'base'})` – im Backend z. B. via `icu_collator`/eigene Normalisierung); Tie-Break `id asc` |
| `limit` | `50` | 1–500, sonst `400 bad_request` |
| `offset` | `0` | ≥ 0 |

Ungültige `sort`/`dir` → `400 bad_request`. Unbekannte Parameter werden ignoriert.

### Antwort
```ts
export interface StrainListItem extends Omit<Strain, 'offers' | 'search'> {}
export interface Facets {
  genetik: { value: string; count: number }[];          // über ALLE Sorten des Laufs, alphabetisch (de), leerer Wert weggelassen
  price: { min: number; max: number } | null;           // Rohgrenzen (ungerundet) über alle Sorten mit Wert
  thc:   { min: number; max: number } | null;
  cbd:   { min: number; max: number } | null;
  rating:{ min: number; max: number } | null;
}
export interface StrainsPage {
  run: Run; reference_run: Run | null;
  total: number;           // Treffer nach Filter
  limit: number; offset: number;
  facets: Facets;          // unabhängig vom Filter (für Slider-Grenzen/Chips)
  strains: StrainListItem[];
}
```
Header: `ETag: "run-<id>[-r<ms>]-<fnv/xxhash der normalisierten Query>"`, `Cache-Control: public, max-age=300`, 304 bei `If-None-Match`.

Festlegungen der Backend-Implementierung (Auflösung von Unklarheiten):
- Kollation: eigene Normalisierung (Kleinschreibung, NFKD ohne kombinierende Zeichen, `ß` → `ss`, Ziffernfolgen numerisch);
  `Äpfel` und `apfel` sind gleichrangig und werden per `id asc` getrennt. Bei `dir=desc` wird nur der Primärschlüssel umgekehrt, der
  Tie-Break bleibt `id asc`.
- Ein leerer Text-Sortierschlüssel (z. B. `genetik=""`) ist kein Null, sondern der kleinste Wert (steht bei `asc` vorn, bei `desc` hinten).
- `q` wird getrimmt und lowercased; leere Werte (`q=`, `genetik=,`) gelten als nicht gesetzt. Nicht-endliche Zahlen (`NaN`, `inf`) → `400`.
- Hash: 64-Bit FNV-1a (hex) über `key=value&…` der effektiven Parameter in Schlüsselreihenfolge inkl. Defaults
  (`dir`, `limit`, `offset`, `sort`); `genetik` als sortierte, lowercased Menge.

### Frontend
- q-table mit `server`-Pagination (`v-model:pagination`, `@request`), Seitengrößen 25/50/100 (Default 50), Seitenwechsel oben/unten.
- Filter/Suche/Sortierung lösen API-Calls aus (Suche 250 ms debounced, Requests per AbortController abgebrochen); Slider-Grenzen und Genetik-Chips aus `facets`; Slider auf voller Breite ⇒ Parameter weglassen.
- URL-State erweitert um `page` und `size` (Defaults weggelassen). Ergebniszähler aus `total`.
- `lib/filter.ts`/`lib/sort.ts` werden auf Query-Aufbau reduziert (kein Client-Filtern/-Sortieren mehr); Tests entsprechend.

## Erweiterung: paginierte Angebotshistorie

`GET /api/v1/strains/{id}/offer-history?from&to&bucket=run|day&mode=changes|all&pharmacy_id=&limit=50&offset=0`

- `from`/`to`/`bucket` wie bei `/history` (Defaults `now-90d`, `now`, `run`; max. 730 d).
- `mode=changes` (Default): **Phasen** – eine Zeile je Apotheke und zusammenhängender Folge von Läufen mit gleichem Preis+Status;
  fehlt die Apotheke in einem Lauf, in dem die Sorte sonst gesehen wurde, beginnt eine Phase `delisted=true`; Läufe vor dem
  ersten Auftauchen der Apotheke werden ignoriert. Sortierung: `from` desc, Apotheke asc (de-Kollation).
- `mode=all`: eine Zeile je (Bucket, Apotheke) mit Angebot; Sortierung `at` desc, Apotheke asc.
- `pharmacy_id` filtert optional auf eine Apotheke. `limit` 1–500 (sonst 400), `offset` ≥ 0. 404 bei unbekannter Sorte.
- Backend-Festlegungen: `include_partial` (Default `true`) wird wie bei `/history` akzeptiert. Die Bucket-Menge sind alle Buckets, in denen
  die Sorte irgendein Angebot hatte; `pharmacy_id` filtert erst die fertigen Zeilen (Phasen werden über alle Apotheken berechnet), `total`
  zählt die gefilterten Zeilen vor dem Slicing. `from`/`to` einer Phase sind die rohen Bucket-Labels (RFC 3339 bzw. `YYYY-MM-DD`).
  Bei `mode=changes` ohne Angebot einer Apotheke: `price`/`price_per_thc_gram` null, `availability` leer, `delisted=true`.

```ts
export interface OfferHistoryRow {          // mode=all
  at: string; run_id?: number; pharmacy_id: number; pharmacy: string; city: string;
  price: number | null; price_per_thc_gram: number | null; availability: string;
}
export interface OfferPhaseRow {            // mode=changes
  pharmacy_id: number; pharmacy: string; city: string;
  price: number | null; price_per_thc_gram: number | null; availability: string;
  from: string; to: string | null;          // to=null ⇒ gilt im letzten Bucket des Zeitraums noch
  runs: number; delisted: boolean;
}
export interface OfferHistoryPage {
  strain_id: number; bucket: HistoryBucket; mode: 'changes' | 'all'; from: string; to: string;
  total: number; limit: number; offset: number;
  rows: OfferHistoryRow[] | OfferPhaseRow[];
}
```
Frontend: Tabelle „Angebotshistorie" nutzt diesen Endpoint mit q-table-Server-Pagination (25/50/100), Toggle „Nur Änderungen / Alle Läufe" = `mode`;
Abschnitt „Bewertungen" bekommt statt „Mehr anzeigen" Seitensteuerung (`limit`/`offset`, 25/50/100). „Aktuelle Angebote" bleibt unpaginiert (≤ Anzahl Apotheken).

## Erweiterung: OpenAPI-Dokumentation statt Downloads

- Backend liefert die Spec unter `GET /api/openapi.json` (OpenAPI 3.1, via `utoipa`) und eine Doku-Seite unter `GET /api/docs`
  (Scalar oder Swagger-UI, self-contained, kein CDN). Umsetzung: Swagger UI mit im Binary eingebetteten Assets unter `/api/docs/*`
  (`utoipa-swagger-ui`, Feature `vendored`); Scalar lädt seine Assets standardmäßig per CDN und scheidet daher aus. Alle `/api/v1`-Endpunkte inkl. Schemas, Query-Parameter, Fehler-Envelope
  sind dokumentiert (Beschreibungen deutsch).
- `GET /api/v1/export.csv` und `export.json` **bleiben** als dokumentierte Endpunkte bestehen, werden im Frontend aber nicht mehr verlinkt.
- Frontend-Header: Links „CSV"/„JSON" entfallen; stattdessen Link **„API"** → `/api/docs` (neuer Tab).

## Erweiterung: Preisalarm-Abos (E-Mail)

### Regeln
| `kind` | Felder | Auslöser (Vergleich letzter usable Run ↔ vorheriger usable Run) |
|---|---|---|
| `strain_available` | `strain_id` | Sorte hat im letzten Lauf ≥ 1 Angebot, im vorherigen keines (wieder verfügbar) |
| `strain_price_below` | `strain_id`, `threshold` (€/g) | `min_price < threshold` im letzten Lauf und (`min_price >= threshold` oder nicht gelistet) im vorherigen — nur beim Unterschreiten |
| `any_price_below` | `threshold` | wie oben, für jede Sorte; Ereignis je Sorte |
| `thc_above` | `threshold` (%) | Sorte **neu gelistet** (im vorherigen Lauf nicht vorhanden) mit `thc_value > threshold` |
| `new_strain` | – | jede neu gelistete Sorte |
| `strain_price_change` | `strain_id` | `min_price` der Sorte hat sich gegenüber dem vorherigen Lauf geändert |

Ein Abonnent bekommt pro Lauf **höchstens eine** E-Mail mit allen ausgelösten Ereignissen (gruppiert nach Regel, Links auf `PUBLIC_URL/sorte/{id}`).
Auswertung läuft im Backend direkt nach einem Lauf mit Status `success|partial`, nur für **bestätigte** Abonnenten; jedes Ereignis wird in
`notifications` protokolliert (Dedupe: gleiche Regel + Sorte + Run nie doppelt). E-Mail-Versand-Fehler werden geloggt und beim
nächsten Lauf nicht nachgeholt.

### Schema
```sql
CREATE EXTENSION IF NOT EXISTS citext;
subscribers(id BIGSERIAL PK, email CITEXT UNIQUE, confirmed_at TIMESTAMPTZ NULL, confirm_token TEXT UNIQUE, manage_token TEXT UNIQUE,
            created_at, updated_at, last_notified_run_id BIGINT NULL)
subscription_rules(id BIGSERIAL PK, subscriber_id FK CASCADE, kind TEXT CHECK (…), strain_id BIGINT NULL FK, threshold NUMERIC(8,2) NULL,
            created_at, UNIQUE(subscriber_id, kind, strain_id, threshold))
notifications(id BIGSERIAL PK, subscriber_id FK CASCADE, run_id FK CASCADE, rule_id FK CASCADE, strain_id BIGINT NULL, payload JSONB,
            sent_at TIMESTAMPTZ NULL, error TEXT NULL, UNIQUE(rule_id, strain_id, run_id))
```
Tokens: 32 Byte zufällig, base64url; `manage_token` dient zum Verwalten **und** Abmelden. Unbestätigte Abonnenten werden nach 7 Tagen gelöscht (beim Scheduler-Tick).

Präzisierungen (Backend-Implementierung, verbindlich):
- Die beiden `UNIQUE`-Constraints sind `UNIQUE NULLS NOT DISTINCT`, damit auch Regeln/Ereignisse ohne `strain_id`/`threshold` dedupliziert werden.
- `notifications.strain_id` ist bei `any_price_below`, `thc_above` und `new_strain` die Sorte des Ereignisses (nie `NULL` in der Praxis).
- Vergleichsbasis je Sorte und Lauf: `min_price` = günstigster geparster Preis (Cent-genau), „gelistet“ = ≥ 1 Angebot, `thc_value` = höchster THC-Wert der Angebote.
  `strain_price_below`/`any_price_below`: vorheriger Lauf „nicht gelistet“ schließt „gelistet ohne parsebaren Preis“ ein. `strain_price_change` nur, wenn die Sorte in beiden Läufen gelistet ist (Preis ↔ kein Preis gilt als Änderung). Ohne vorherigen usable Lauf wird nichts ausgewertet.
- `PUT …/manage` verlangt wie das Anlegen 1–20 Regeln (0 Regeln ⇒ 400; Abmelden per `DELETE`). `threshold` muss > 0 sein (THC ≤ 100) und wird auf 2 Nachkommastellen gerundet; unbekannte `strain_id` ⇒ 400.
- Das Rate-Limit zählt nur validierte, nicht per Honeypot verworfene Anlage-Requests; 429 trägt `code: bad_request` (kein eigener Code im Envelope).

### Env
| Variable | Default | Bedeutung |
|---|---|---|
| `PUBLIC_URL` | `http://localhost:9000` | Basis für Links in Mails (`/sorte/{id}`, `/abo/bestaetigen?token=`, `/abo/verwalten?token=`) |
| `EMAIL_ENABLED` | `false` | `false` ⇒ Mails werden nur geloggt (Betreff + Empfänger + Text auf INFO) |
| `SMTP_HOST`, `SMTP_PORT` (`587`), `SMTP_USERNAME`, `SMTP_PASSWORD`, `SMTP_TLS` (`starttls\|tls\|none`) | – | SMTP via `lettre` |
| `EMAIL_FROM` | `GreenMedical Livebestand <noreply@localhost>` | Absender |
| `SUBSCRIPTION_RATE_LIMIT` | `5/1h` | max. Anlegen/Bestätigungsmails pro IP (in-memory) |

### API
```ts
export type RuleKind = 'strain_available' | 'strain_price_below' | 'any_price_below' | 'thc_above' | 'new_strain' | 'strain_price_change';
export interface RuleInput { kind: RuleKind; strain_id?: number; threshold?: number; }
export interface Rule extends RuleInput { id: number; strain_name?: string | null; created_at: string; }
export interface SubscriptionCreate { email: string; rules: RuleInput[]; website?: string /* Honeypot, muss leer sein */ }
export interface Subscription { email: string; confirmed: boolean; rules: Rule[]; created_at: string; }
```
| Endpoint | Verhalten |
|---|---|
| `POST /api/v1/subscriptions` | Body `SubscriptionCreate` (1–20 Regeln, valide E-Mail, Regelfelder gemäß Tabelle sonst 400). Existiert die E-Mail schon: Regeln werden **hinzugefügt**, bei unbestätigt neue Bestätigungsmail. Antwort immer `202 {"status":"confirmation_sent"}` (kein E-Mail-Enumeration-Leak). 429 bei Rate-Limit. Honeypot gefüllt ⇒ 202 ohne Aktion. |
| `POST /api/v1/subscriptions/confirm` | Body `{token}` → `200 Subscription`; 404 bei unbekanntem Token |
| `GET /api/v1/subscriptions/manage?token=` | `200 Subscription` (manage_token); 404 |
| `PUT /api/v1/subscriptions/manage?token=` | Body `{rules: RuleInput[]}` ersetzt alle Regeln → `200 Subscription` |
| `DELETE /api/v1/subscriptions/manage?token=` | löscht Abonnent + Regeln → 204 (Abmelden) |
Alle Subscription-Endpunkte antworten mit `Cache-Control: no-store`.

### E-Mails (deutsch, Text + einfache HTML-Variante)
- Bestätigung: Betreff „Bitte bestätige deinen Preisalarm", Link `PUBLIC_URL/abo/bestaetigen?token=<confirm_token>`.
- Benachrichtigung: Betreff „Preisalarm: N Ereignisse (Datum)", je Regel eine Liste (Sorte, Preis/THC, Apotheke, Link), Fußzeile mit Verwalten-/Abmelde-Link `PUBLIC_URL/abo/verwalten?token=<manage_token>`.

### Frontend
- Header-Link **„Preisalarm"** → `/abo`: E-Mail + Regel-Editor (Art wählen; Sorte per Autocomplete über `GET /strains?q=&limit=10`;
  Schwellwert), Honeypot-Feld, Absenden → Hinweis „Bestätigungsmail gesendet".
- `/abo/bestaetigen?token=` ruft `confirm` auf und zeigt die Regeln; `/abo/verwalten?token=` zeigt/ändert Regeln, Button „Abmelden".
- Sortenseite: Button „Preisalarm für diese Sorte" → `/abo?strain_id=<id>` (vorbelegt `strain_available` bzw. `strain_price_below`).
- Dev: `docker-compose.yml` bekommt `mailpit` (`axllent/mailpit`, UI :8025, SMTP :1025), Backend im compose mit `EMAIL_ENABLED=true`, `SMTP_HOST=mailpit`, `SMTP_PORT=1025`, `SMTP_TLS=none`, `PUBLIC_URL=http://localhost:9000`.
- Helm: `backend.config.publicUrl`, `email.enabled`, `email.from`, `email.smtp.{host,port,tls}`, `email.smtp.existingSecret` (Keys `SMTP_USERNAME`, `SMTP_PASSWORD`) bzw. `email.smtp.username/password` (Dev).

## Erweiterung: stündliche Aktualisierung + Countdown-Banner

- Neuer Default `SCRAPE_CRON=0 0 * * * *` (jede volle Stunde, Europe/Berlin), `SCRAPE_BOOTSTRAP_MAX_AGE=2h`. Reviews-Phase unverändert (je Sorte alle 24 h).
- `Metadata` erhält:
  - `next_run_at: string | null` – nächster geplanter Lauf (RFC 3339 UTC) laut Cron/Zeitzone; `null` wenn `SCRAPE_ENABLED=false`. Deterministisch aus Cron berechnet (jede Replika liefert denselben Wert).
  - `scrape_running: boolean` – es existiert ein Lauf mit Status `running` (DB-Abfrage, replikaübergreifend).
  - `schedule: { cron: string; timezone: string } | null` – aktive Konfiguration.
- `GET /api/v1/metadata` wird dafür pro Request serialisiert (Snapshot-Teil + Live-Felder); `Cache-Control: public, max-age=60` (statt 300) für metadata.
- Frontend: Banner oben in `MainLayout` (über dem Header, dezent, `role="status"`, `aria-live="polite"`):
  „Nächste Aktualisierung in 37 Minuten" (unter 1 min: „in weniger als einer Minute"; Stunden: „in 1 Std. 5 Min."), während `scrape_running`: „Aktualisierung läuft …" mit Spinner; danach lädt das Frontend `metadata` alle 30 s, bis `run.id` sich ändert, und lädt dann die aktuelle Seite/Detailseite neu (Stand-Anzeige aktualisiert sich, Hinweis „Daten aktualisiert"). Countdown tickt jede Minute; `next_run_at` in der Vergangenheit ⇒ „Aktualisierung steht an …" und Polling. Banner ausgeblendet, wenn `next_run_at` null und nichts läuft.
