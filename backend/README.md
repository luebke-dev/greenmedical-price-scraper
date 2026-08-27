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

## HTTP-API (Überblick)

Verbindlich ist `docs/api-contract.md`; Kurzfassung der Lese-Endpoints:

| Endpoint | Inhalt |
|---|---|
| `GET /api/v1/metadata` | Kennzahlen des letzten usable Runs (vorserialisiert) |
| `GET /api/v1/strains` | **Serverseitig paginierte** Sortenliste (`StrainsPage`): `q`, `genetik` (kommagetrennt, case-insensitiv), `price_/thc_/cbd_min|max` (inklusive; Sorten ohne Wert nur ohne Grenzen), `rating_min`, `sort` (`price` \| `price_per_thc_gram` \| `thc` \| `cbd` \| `pharmacy_count` \| `rating` \| `name` \| `bezeichnung` \| `genetik`), `dir`, `limit` 1–500 (Default 50), `offset`. Einträge ohne `offers`/`search`; `facets` über alle Sorten des Laufs; ETag `"run-<id>[-r<ms>]-<fnv1a der normalisierten Query>"`, 304 bei `If-None-Match` |
| `GET /api/v1/strains/{id}` | Detail inkl. `offers` und `search` |
| `GET /api/v1/strains/{id}/history` | Preisverlauf (`bucket=run\|day`, optional `pharmacies=true`) |
| `GET /api/v1/strains/{id}/offer-history` | Paginierte Angebotshistorie je Apotheke: `mode=changes` (Phasen gleicher Preis+Status, `delisted`) oder `mode=all` (eine Zeile je Bucket und Apotheke); `bucket`, `from`/`to`, `include_partial`, `pharmacy_id`, `limit`/`offset` |
| `GET /api/v1/strains/{id}/reviews` | Bewertungen (`limit`/`offset`/`sort`) |
| `GET /api/v1/runs`, `/runs/{id}`, `/pharmacies` | Läufe und Apotheken |
| `GET /api/v1/export.json`, `/export.csv` | Vollständiger Export des Laufs (`?run_id=`), `export.json` als Array inkl. `offers` |
| `POST /api/v1/subscriptions` | Preisalarm anlegen (`SubscriptionCreate`), immer `202 {"status":"confirmation_sent"}`; `429` bei Rate-Limit |
| `POST /api/v1/subscriptions/confirm` | `{token}` aus der Bestätigungsmail → `200 Subscription` |
| `GET`/`PUT`/`DELETE /api/v1/subscriptions/manage?token=` | Abo anzeigen / Regeln ersetzen (`{rules}`) / abmelden (`204`) mit dem `manage_token` |
| `GET /api/openapi.json`, `GET /api/docs` | OpenAPI-3.1-Dokument (`utoipa`) und Swagger UI (Assets im Binary, kein CDN) |

Filterung und Sortierung von `/strains` laufen im Speicher über den Snapshot des Laufs
(keine SQL-Abfrage pro Request): Sortierschlüssel werden einmal pro Snapshot berechnet,
die Reihenfolge je (`sort`, `dir`) wird lazily gecacht, ein Request ist danach O(n).
Textsortierung (`name`, `bezeichnung`, `genetik`, Apothekennamen in der Angebotshistorie)
ahmt `Intl.Collator('de', { numeric: true, sensitivity: 'base' })` nach
(`domain::collate`): Kleinschreibung, NFKD ohne kombinierende Zeichen (`Ä` → `a`,
`ß` → `ss`), Ziffernfolgen numerisch (`Sorte 9` < `Sorte 10`), Tie-Break `id` aufsteigend.

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
| `PUBLIC_URL` | `http://localhost:9000` | Basis der Links in Mails (`/sorte/{id}`, `/abo/bestaetigen?token=`, `/abo/verwalten?token=`) |
| `EMAIL_ENABLED` | `false` | `false` ⇒ Mails werden nur geloggt (Empfänger, Betreff, Text auf INFO) |
| `SMTP_HOST`, `SMTP_PORT` | –, `587` | SMTP-Relay (`lettre`, rustls – kein OpenSSL); `SMTP_HOST` ist Pflicht bei `EMAIL_ENABLED=true` |
| `SMTP_USERNAME`, `SMTP_PASSWORD` | – | optional; PLAIN/LOGIN-Auth nur wenn beide gesetzt |
| `SMTP_TLS` | `starttls` | `starttls` \| `tls` (implizit, Port 465) \| `none` (z. B. mailpit) |
| `EMAIL_FROM` | `GreenMedical Livebestand <noreply@localhost>` | Absender (`Name <adresse>` oder nur Adresse) |
| `SUBSCRIPTION_RATE_LIMIT` | `5/1h` | `<Anzahl>/<Dauer>`: max. Abo-Anlagen (= Bestätigungsmails) pro Client-IP, in-memory pro Prozess |

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

## OpenAPI

`GET /api/openapi.json` liefert das mit `utoipa` aus den Handler-Annotationen (`#[utoipa::path]`)
und DTOs (`ToSchema`, serde-Feldnamen) generierte OpenAPI-3.1-Dokument, `GET /api/docs` eine
Swagger-UI-Seite. Die UI-Assets sind über `utoipa-swagger-ui` (Feature `vendored`) im Binary
eingebettet und werden unter `/api/docs/*` ausgeliefert – die Seite lädt nichts von fremden Hosts
(Test `openapi_document_and_docs_page_are_served`). Der Admin-Endpoint trägt das Security-Schema
`admin_token` (Bearer). Neue Endpoints/DTOs müssen in `src/api/openapi.rs` registriert werden;
der Unit-Test dort prüft Pfade, Schemas und die `/strains`-Query-Parameter.

## Preisalarm-Abos (E-Mail)

Schema (Migration `0004_subscriptions.sql`, Extension `citext`): `subscribers` (E-Mail
case-insensitiv, `confirm_token`/`manage_token` = 32 Zufallsbytes base64url ohne Padding, 43
Zeichen), `subscription_rules` (`kind` per CHECK, `UNIQUE NULLS NOT DISTINCT (subscriber_id, kind,
strain_id, threshold)` – doppelte Regeln werden beim Anlegen ignoriert) und `notifications`
(`UNIQUE NULLS NOT DISTINCT (rule_id, strain_id, run_id)`, `payload` = das Ereignis als JSON).

Ablauf: `POST /subscriptions` validiert (E-Mail syntaktisch, 1–20 Regeln, Felder je `kind` gemäß
Vertragstabelle, `strain_id` muss existieren, `threshold` > 0 und auf Cent gerundet, THC ≤ 100),
zählt danach gegen das Rate-Limit (X-Forwarded-For erster Eintrag, sonst Peer-Adresse) und legt
den Abonnenten an bzw. fügt Regeln hinzu; unbestätigte Abonnenten bekommen bei jedem Anlegen erneut
die Bestätigungsmail mit demselben Token. `PUT …/manage` ersetzt die Regeln (ebenfalls 1–20; zum
Abmelden `DELETE`). Alle Subscription-Antworten – auch Fehler – tragen `Cache-Control: no-store`.
Unbestätigte Abonnenten löscht der Scheduler-Tick nach 7 Tagen (`notify::cleanup_unconfirmed`).

Auswertung (`src/notify`): direkt nach dem Persistieren eines Laufs mit Status `success|partial`
(vor Phase 2, Fehler werden nur geloggt) wird der Lauf mit dem vorherigen usable Lauf verglichen,
je Sorte über `min_price` (günstigster geparster Preis, Cent-genau), „gelistet“ (≥ 1 Angebot) und
`thc_value` (höchster THC-Wert der Angebote). Ohne Vorgängerlauf passiert nichts. Regeln bestätigter
Abonnenten erzeugen Ereignisse exakt nach der Vertragstabelle; präzisiert:
`strain_price_below`/`any_price_below` feuern nur beim Unterschreiten (vorher ≥ Schwellwert,
ungelistet oder ohne Preis), `strain_price_change` nur wenn die Sorte in beiden Läufen gelistet ist
(Preis ↔ kein Preis zählt als Änderung), `thc_above`/`new_strain` nur für Sorten, die im
Vorgängerlauf nicht vorkamen. Je Abonnent und Lauf entsteht höchstens eine Mail: zuerst werden die
`notifications`-Zeilen eingefügt (Dedupe über den UNIQUE-Key, bei Regeln ohne Sorte mit der Sorte
des Ereignisses), nur neu eingefügte Ereignisse landen im Digest; danach wird gesendet und
`sent_at` bzw. `error` gesetzt, `subscribers.last_notified_run_id` aktualisiert. Fehlgeschlagene
Mails werden nicht wiederholt. `scrape-once` durchläuft dieselbe Auswertung.

Mails (`src/mail/templates.rs`, deutsch, Text + einfaches HTML): Bestätigung „Bitte bestätige
deinen Preisalarm“ mit `PUBLIC_URL/abo/bestaetigen?token=…`; Digest „Preisalarm: N Ereignisse
(TT.MM.JJJJ)“ (Datum in `SCRAPE_TIMEZONE`), je Regel eine Überschrift und Liste (Sorte, Preis
bzw. „vorher“-Preis, THC, Apotheke des günstigsten Angebots, Link `PUBLIC_URL/sorte/{id}`), Fußzeile
`PUBLIC_URL/abo/verwalten?token=…`. `EMAIL_ENABLED=false` ⇒ `LogMailer` (nur Log);
`true` ⇒ `SmtpMailer` (lettre, rustls). Lokal: mailpit aus `docker-compose.yml`
(`SMTP_HOST=mailpit`, `SMTP_PORT=1025`, `SMTP_TLS=none`).

Metriken: `notifications_sent_total{result="sent|error"}`,
`subscriptions_total{state="confirmed|unconfirmed"}` (Gauge, aktualisiert beim Start, bei
API-Änderungen und beim Scheduler-Tick).

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

Für Tests wird nie SMTP verwendet: `tests/support::test_config` pinnt `--email-enabled false`,
`--public-url http://localhost:9000` und `--subscription-rate-limit 5/1h`;
`test_state_with_mailer` liefert einen `RecordingMailer`, über den Bestätigungs- und Digest-Mails
geprüft werden (`tests/subscriptions.rs`).

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
