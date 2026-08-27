# greenmedical-backend

Rust-Backend des GreenMedical Price Scrapers: scrapt greenmedical.health nach Zeitplan
(Standard 04/10/16/22 Uhr Europe/Berlin), speichert jeden Lauf in PostgreSQL und
liefert die JSON-API für das Frontend (`docs/api-contract.md` ist der verbindliche Vertrag).

## Starten

```bash
# Postgres z.B. via docker-compose.yml im Repo-Root
export DATABASE_URL=postgres://greenmedical:greenmedical@localhost:5432/greenmedical
export LOG_FORMAT=pretty
cargo run -- serve          # Migrationen, Scheduler, API :8080, Metrics :9090
cargo run -- scrape-once    # genau ein Lauf, Exit-Code ≠ 0 bei Status "failed"
cargo run -- scrape-once --reviews-only   # nur Phase 2: Bewertungen aller Sorten des letzten usable Runs
cargo run -- migrate        # nur Migrationen anwenden
```

Ohne Subcommand wird `serve` ausgeführt. Ein manueller Lauf lässt sich mit gesetztem
`ADMIN_TOKEN` per `POST /api/v1/admin/scrape` (Bearer-Token) auslösen.

## Konfiguration (Env)

| Variable | Default | Bedeutung |
|---|---|---|
| `DATABASE_URL` | – (Pflicht) | Postgres-DSN; Advisory-Locks brauchen Session-Pooling (kein PgBouncer im Transaction-Mode) |
| `DATABASE_MAX_CONNECTIONS` | `10` | sqlx-Pool (mind. 4) |
| `HTTP_BIND` / `METRICS_BIND` | `0.0.0.0:8080` / `0.0.0.0:9090` | API bzw. Prometheus `/metrics` |
| `HTTP_REQUEST_TIMEOUT` | `30s` | Timeout pro API-Request (Antwort `408` mit Fehler-Envelope, `code: internal`) |
| `SNAPSHOT_REVALIDATE_INTERVAL` | `30s` | Wie oft der Snapshot-Cache per Index-Query prüft, ob ein anderes Replikat einen neueren Lauf gespeichert hat (`0s` = bei jedem Request) |
| `CORS_ALLOWED_ORIGINS` | leer | kommagetrennt; leer = kein CORS-Header |
| `LOG_FORMAT` / `RUST_LOG` | `json` / `info,sqlx=warn` | |
| `MIGRATE_ON_STARTUP` | `true` | `sqlx::migrate!` beim Start (replikasicher via Advisory-Lock) |
| `SCRAPE_ENABLED` | `true` | Scheduler an/aus, API läuft immer |
| `SCRAPE_CRON` | `0 0 4,10,16,22 * * *` | `cron`-Crate-Format (sec min hour dom mon dow) |
| `SCRAPE_TIMEZONE` | `Europe/Berlin` | Zeitzone für Cron und Tages-Buckets |
| `SCRAPE_BOOTSTRAP` / `SCRAPE_BOOTSTRAP_MAX_AGE` | `true` / `8h` | Sofort scrapen, wenn kein/zu alter Lauf |
| `SCRAPE_STALE_RUN_AFTER` | `2h` | ältere `running`-Läufe werden `failed` |
| `SCRAPE_BASE_URL` | `https://greenmedical.health` | in Tests auf wiremock umgebogen |
| `SCRAPE_USER_AGENT` | Firefox-UA | wie im alten Python-Scraper |
| `SCRAPE_REQUEST_TIMEOUT` / `SCRAPE_RETRY_TOTAL` / `SCRAPE_BACKOFF_FACTOR` | `30s` / `4` / `1.0` | Retry auf 429/5xx, Sleeps 0/2/4/8 s, `Retry-After` wird beachtet (max. 120 s) |
| `SCRAPE_PHARMACY_DELAY` / `SCRAPE_PAGE_DELAY` | `300ms` / `500ms` | Höflichkeitspausen |
| `SCRAPE_MIN_SUCCESS_RATIO` | `0.5` | darunter ist der Lauf `failed` |
| `REVIEWS_ENABLED` | `true` | Phase 2 (Bewertungen) an/aus |
| `REVIEWS_MAX_AGE` | `24h` | Sorten mit jüngerem `reviews_scraped_at` werden in Phase 2 übersprungen |
| `REVIEWS_MAX_PER_RUN` | `0` | max. Produktseiten pro Lauf (`0` = unbegrenzt), älteste zuerst |
| `ADMIN_TOKEN` | leer | leer ⇒ Admin-Endpoint antwortet 404 |
| `INSTANCE_NAME` | `$HOSTNAME` | Label auf `scrape_runs.instance` |

Status eines Laufs: `failed` bei 0 Apotheken, 0 aufgelösten UUIDs, Erfolgsquote unter
`SCRAPE_MIN_SUCCESS_RATIO` oder 0 Angeboten (Layout-Guard – ein leerer Lauf wird nie
"latest"); `partial` bei einzelnen Apotheken-Fehlern (auch fehlende UUID); sonst `success`.
Bei `failed` werden keine Angebote gespeichert. Gleichzeitige Läufe verhindert ein
in-process Mutex plus ein Postgres-Advisory-Lock (`greenmedical:scrape`) – bei mehreren
Replikas scrapt genau eine, die anderen loggen `advisory lock held ... skipping`.

## Phase 2: Bewertungen

Nach dem Persistieren der Angebote (der Lauf ist dann bereits abgeschlossen und der
„latest usable Run“) lädt derselbe Lauf für jede Sorte, deren `reviews_scraped_at`
fehlt oder älter als `REVIEWS_MAX_AGE` ist, die Produktseite (`produkt_url` ohne
Query/Fragment, Pause `SCRAPE_PAGE_DELAY`, älteste zuerst, höchstens
`REVIEWS_MAX_PER_RUN`). Geparst werden JSON-LD `aggregateRating` (Fallback: Header-Spans),
die Produkt-UUID und alle `div.pdpReview`-Blöcke (`src/scrape/reviews.rs`). Pro Sorte
wird in einer kleinen Transaktion committet: `strains.product_uuid/rating_value/
review_count/reviews_scraped_at`, eine Zeile in `strain_rating_snapshots` (mit `run_id`)
und die Reviews per Upsert auf `(strain_id, fingerprint)` mit `fingerprint =
sha256("author|reviewed_on|rating|content")`. Fehler einzelner Seiten werden gezählt
(`scrape_runs.reviews_failed`, Metrik `scrape_reviews_total{result}`), machen den Lauf
aber nie `failed`; Gate und Advisory-Lock bleiben bis zum Ende von Phase 2 gehalten,
danach wird der Snapshot-Cache erneut verworfen (Ratings hängen an `strains`, nicht am Lauf).
`scrape-once --reviews-only` führt nur Phase 2 für alle Sorten des letzten usable Runs
aus (ignoriert `REVIEWS_MAX_AGE`). API: `rating`/`sort.rating`/`product_uuid` je Sorte,
`best_rated` in `/metadata` (≥ 5 Bewertungen) und `GET /api/v1/strains/{id}/reviews`.

`bucket=day` in der History aggregiert über alle Läufe eines Berlin-Tages: `min`/`max`
über alle Angebote, `avg` über alle Angebote, `offer_count` = Ø Angebote pro Lauf,
`pharmacy_count` = distinkte Apotheken, `run_count` = Anzahl Läufe.

## Mehrere Replikas / Caching

Die Read-Endpoints (`/metadata`, `/strains`, `/export.*`, `offer_count_latest` in
`/pharmacies`) werden aus einem vorserialisierten Snapshot des letzten usable Runs bedient.
Das Replikat, das gescrapt hat, verwirft seinen Cache sofort; alle anderen prüfen höchstens
alle `SNAPSHOT_REVALIDATE_INTERVAL` mit `SELECT id … ORDER BY started_at DESC LIMIT 1`
(Partial-Index), ob ein neuerer Lauf existiert, und bauen den Snapshot dann neu. Die maximale
Verzögerung zwischen Replikas entspricht also dem Intervall (Default 30 s); schlägt die
Prüfung fehl, wird der gecachte Lauf weiter ausgeliefert. `?run_id=`-Exporte älterer Läufe
werden in einem kleinen LRU (4 Einträge) gehalten und seriell gebaut, damit der öffentliche
Endpoint keine beliebige DB-/CPU-Last erzeugen kann; laufende Runs werden nicht gecacht.

## Fehlerformat

Jede Fehlerantwort ist `{"error":{"code","message"}}` – auch bei ungültigen Pfad-/Query-
Parametern (`400 bad_request`, eigene Extractor-Wrapper `ApiPath`/`ApiQuery`), bei
falscher Methode auf bekanntem Pfad (`405`, `code: bad_request`, „Methode nicht erlaubt“)
und bei Request-Timeout (`408`, `code: internal`). Der Vertrag kennt für 405/408 keinen
eigenen Code; dies ist die dokumentierte Zuordnung.

## Bewusste Abweichungen vom Python-Scraper

- Labels (`preis_pro_gramm`, `thc`, `cbd`, Name, Bezeichnung, Apothekenfelder) werden vor
  dem Speichern mit `clean_text` whitespace-normalisiert (geschützte Leerzeichen → Leerzeichen,
  Mehrfach-Whitespace zusammengefasst). `export.csv` ist daher nicht byte-identisch zur alten
  `greenmedical_flowers.csv`, wenn die Seite `&nbsp;` in Labels liefert; Spalten, Reihenfolge
  und Zeilenreihenfolge sind identisch. `name`/`bezeichnung` in der CSV stammen aus der
  kanonischen `strains`-Zeile (Anzeigewert der Gruppe), nicht aus dem einzelnen Tile.
- Gruppierungsschlüssel ist `lower(NFKC(clean_text(x)))` (wie im Vertrag), Python nutzte
  `str.casefold()` – Unterschiede nur bei Sonderfällen wie `ß`/`SS`.
- `Retry-After` wird wie bei urllib3 für alle wiederholten Status (429/500/502/503/504)
  beachtet, aber auf 120 s gedeckelt. Nach einer fehlgeschlagenen Apotheke wird keine
  Höflichkeitspause eingelegt (wie im Python-`continue`).
- Manuelle Läufe (`POST /admin/scrape`) laufen in einem `TaskTracker`; beim Shutdown wartet
  der Prozess, bis der Lauf als `failed`/`shutdown` markiert ist, bevor der Pool schließt.
- Produkt-URLs werden mit `Url::join` aufgelöst: umgebender Whitespace im `href` wird
  entfernt (wie `.strip()`), innerer Whitespace als `%20` kodiert (Python-`urljoin` ließ ihn
  stehen). Ergebnis ist eine gültige URL, wie sie ein Browser ebenfalls normalisieren würde.

## Session-Cookie (Parität zu `requests.Session`)

greenmedical.health beantwortet `GET /de/cannabis/flowers?deliveryTarget=…&page=N` mit
`302 Location: /de/cannabis/flowers?page=N` plus `Set-Cookie: PHPSESSID=…`; die gewählte
Apotheke steckt **nur** in dieser PHP-Session. Ohne Cookie-Jar liefert die weitergeleitete
Anfrage für jede Apotheke den generischen Katalog (identische ~55 Angebote überall).
Der `ScrapeClient` hat deshalb `cookie_store(true)` und wird **pro Lauf** neu gebaut
(`execute_run`), sodass Sessions nie zwischen Läufen wandern – wie früher
`with create_session() as session`. Die wiremock-Seite in `tests/support/mod.rs` bildet
genau diesen 302-plus-Cookie-Fluss nach; ohne Cookie-Jar schlagen die E2E-Tests fehl.

## Tests

```bash
export DATABASE_URL=postgres://greenmedical:greenmedical@localhost:5432/greenmedical
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test        # #[sqlx::test] legt pro Test eine eigene Datenbank an (Superuser nötig)
```

Die Unit-Tests portieren alle Fälle aus `tests/test_build_site.py` / `tests/test_scraper.py`
des alten Python-Codes; HTML-Fixtures aus dem Live-System liegen unter `tests/fixtures/`.

## sqlx offline (`.sqlx/`)

Alle Queries laufen über `sqlx::query!` und werden gegen die Datenbank geprüft. Damit
`SQLX_OFFLINE=true cargo build` (Dockerfile, CI) ohne DB funktioniert, ist `.sqlx/`
eingecheckt. Nach jeder SQL-Änderung:

```bash
cargo install sqlx-cli --no-default-features --features postgres,rustls   # einmalig
sqlx migrate run                          # Schema in die lokale DB
cargo sqlx prepare                        # .sqlx/ aktualisieren
cargo sqlx prepare --check                # in CI
```

## Container

```bash
podman build -t greenmedical-backend:dev .
podman run --rm -e DATABASE_URL=... -p 8080:8080 -p 9090:9090 greenmedical-backend:dev
```

Das Image basiert auf `gcr.io/distroless/cc-debian12:nonroot` (keine Shell, UID 65532);
Debugging via `kubectl debug`, Wartung über die Subcommands `migrate` und `scrape-once`.
