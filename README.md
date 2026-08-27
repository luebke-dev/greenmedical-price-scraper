# GreenMedical Livebestand

Preisübersicht für Cannabisblüten bei den Partnerapotheken von [greenmedical.health](https://greenmedical.health):
ein **Rust-Backend** scrapt die Angebote selbstständig viermal täglich, speichert jeden Lauf in
**PostgreSQL** und liefert eine JSON-API; ein **Quasar-Frontend** zeigt Tabelle, Filter, Kennzahlen und –
neu gegenüber der früheren statischen Seite – die **Preisentwicklung pro Sorte**.

```
                     ┌──────────────────────────── Kubernetes / docker compose ────────────────────────────┐
Browser ──HTTPS──▶ Ingress ──▶ frontend (nginx :8080, SPA) ──/api/──▶ backend (axum :8080) ──▶ PostgreSQL
                                                                         │ :9090 /metrics ──▶ Prometheus
                                                                         └── 04:00 / 10:00 / 16:00 / 22:00 (Europe/Berlin)
                                                                              ──▶ greenmedical.health (Scrape)
```

| Verzeichnis | Inhalt |
|---|---|
| [`backend/`](backend/) | Rust-Crate (axum, sqlx, reqwest, scraper): Scraper, Scheduler, REST-API, Migrationen, Dockerfile |
| [`frontend/`](frontend/) | Quasar-SPA (Vue 3, TypeScript, ECharts), nginx-Dockerfile |
| [`charts/greenmedical/`](charts/greenmedical/) | Helm-Chart (externe PostgreSQL, Ingress, HPA, PDB, ServiceMonitor, NetworkPolicy) |
| [`docs/api-contract.md`](docs/api-contract.md) | **Verbindlicher Vertrag**: Env-Variablen, Ports, JSON-Shapes, Endpunkte |
| [`docker-compose.yml`](docker-compose.yml) | lokale Entwicklungsumgebung (PostgreSQL; optional Backend + Frontend als Container) |
| [`.github/workflows/`](.github/workflows/) | `ci.yml` (Tests, Lint, Chart-Validierung, Image-Builds) und `release.yml` (Images + Chart nach GHCR) |

Artefakte: Images `ghcr.io/luebke-dev/greenmedical-backend` und `ghcr.io/luebke-dev/greenmedical-frontend`,
Chart `oci://ghcr.io/luebke-dev/charts/greenmedical`.

## Architektur

* **Scraping** läuft im Backend-Prozess (kein CronJob, keine GitHub-Action): Cron `0 0 4,10,16,22 * * *`
  in `Europe/Berlin`, per `SCRAPE_CRON`/`SCRAPE_TIMEZONE` konfigurierbar. Beim Start wird gescrapt, wenn kein
  brauchbarer Lauf jünger als 8 h existiert (`SCRAPE_BOOTSTRAP`). Mehrere Replikas sind sicher: ein
  PostgreSQL-Advisory-Lock stellt sicher, dass immer nur eine Instanz scrapt.
* **Datenmodell**: `scrape_runs` (jeder Lauf mit Status `success|partial|failed`), `pharmacies`, `strains`
  (stabile ID pro Sorte = Name + Bezeichnung) und `offers` (jedes Angebot jedes Laufs). Daraus entstehen
  Historie und Trend (Vergleich mit dem Lauf vor 7 Tagen). Ein Lauf ohne Angebote wird nie „aktuell“
  (Layout-Guard).
* **API** unter `/api/v1` (JSON, RFC 3339 UTC), `GET /healthz`, `GET /readyz`, Prometheus-Metriken auf
  eigenem Port 9090. CSV/JSON-Export sind drop-in-kompatibel zum früheren `greenmedical_flowers.csv` /
  `flowers.json`.
* **Frontend**: Quasar SPA mit Suche, Filtern (Genetik, Preis, THC, CBD), Sortierung, aufklappbaren
  Angeboten pro Sorte, Kennzahl-Karten und Detailseite `/sorte/:id` mit Preisverlauf (min/avg/max,
  optional €/g THC und Apotheken einzeln). nginx proxied `/api/` an das Backend, daher kein CORS nötig.

## Lokale Entwicklung

Voraussetzungen: Rust 1.98 (`rustup`), Node 26 + pnpm 10, Docker **oder** Podman (mit `docker compose` bzw.
`podman-compose`), optional `sqlx-cli` (`cargo install sqlx-cli --no-default-features --features postgres,rustls`).

```bash
cp .env.example .env            # Ports, DATABASE_URL, ADMIN_TOKEN … anpassen
docker compose up -d            # nur PostgreSQL 17 auf 127.0.0.1:${POSTGRES_PORT:-5432}
```

Belegt bereits eine andere PostgreSQL den Port 5432, `POSTGRES_PORT=15432` in `.env` setzen und
`DATABASE_URL` dort entsprechend anpassen. Alternativ ohne compose:

```bash
podman run -d --name gm-pg -p 127.0.0.1:5432:5432 \
  -e POSTGRES_USER=greenmedical -e POSTGRES_PASSWORD=greenmedical -e POSTGRES_DB=greenmedical \
  docker.io/library/postgres:17-alpine
```

### Backend

```bash
cd backend
set -a; source ../.env; set +a
cargo run -- migrate            # Migrationen (passiert auch automatisch beim Start)
cargo run                       # = `serve`: API auf :8080, Metriken auf :9090, Scheduler gemäß SCRAPE_ENABLED
cargo run -- scrape-once        # ein einzelner Lauf ohne Server

cargo fmt --check && cargo clippy --all-targets --all-features --locked -- -D warnings && cargo test  # wie ci.yml
cargo sqlx prepare -- --all-targets --all-features          # nach SQL-Änderungen: .sqlx/ aktualisieren
cargo sqlx prepare --check -- --all-targets --all-features  # wie ci.yml
```

Einen Lauf manuell auslösen (benötigt `ADMIN_TOKEN`):

```bash
curl -X POST -H "Authorization: Bearer $ADMIN_TOKEN" http://localhost:8080/api/v1/admin/scrape
curl -s http://localhost:8080/api/v1/metadata | jq .
```

### Frontend

```bash
cd frontend
pnpm install
pnpm dev                        # http://localhost:9000 – /api → Backend auf http://localhost:8080
API_PROXY_TARGET=http://localhost:18080 pnpm dev  # /api → anderes Backend (z. B. kubectl port-forward)
pnpm dev:mock                   # = MOCK_API=1 pnpm dev: Mock-API aus frontend/dev/, kein Proxy
pnpm lint          # prettier --write + eslint --fix (verändert Dateien)
pnpm lint:check && pnpm typecheck && pnpm test && pnpm build  # wie ci.yml (nur prüfen)
```

`pnpm dev` proxied **immer** alle `/api`-Anfragen an `API_PROXY_TARGET` (Default
`http://localhost:8080`, also das lokale `cargo run`). Quasar liest die `.env` im Repo-Root **nicht**; die
Variable muss in der Shell exportiert sein (`set -a; source ../.env; set +a` oder wie oben inline) und kann
auf jede erreichbare Backend-Instanz zeigen. Nur `pnpm dev:mock` (`MOCK_API=1`) beantwortet `/api/v1/*`
aus den Fixtures in `frontend/dev/` – dann braucht es weder Backend noch Datenbank
(s. [`frontend/README.md`](frontend/README.md)).

### Alles als Container

```bash
docker compose --profile app up -d --build
# Frontend: http://127.0.0.1:8081   Backend: http://127.0.0.1:8080   Metriken: http://127.0.0.1:9090/metrics
```

Backend und Frontend laufen dabei wie in Kubernetes mit Read-only-Root-Dateisystem und `tmpfs`-Mounts.
Der Scheduler ist im compose-Setup standardmäßig **aus** (`SCRAPE_ENABLED=false`, `SCRAPE_BOOTSTRAP=false`);
Läufe über den Admin-Endpoint auslösen oder für automatisches Scrapen `SCRAPE_ENABLED=true` in `.env` setzen.

Bekanntes Verhalten: Das Backend-Image ist distroless (keine Shell, kein curl), daher hat es keinen
Compose-Healthcheck und `frontend` wartet nur auf den *Start* des Backends. Bis `/readyz` antwortet
(Migrationen, DB-Verbindung) liefert das Frontend für `/api` ein `502`. Auf Bereitschaft warten:

```bash
curl --retry 30 --retry-delay 1 --retry-all-errors -fsS http://127.0.0.1:8080/readyz
```

## Konfiguration

Alle Variablen mit Defaults und Bedeutung: [`docs/api-contract.md`](docs/api-contract.md#backend-env-variablen).
Die wichtigsten:

| Variable | Default | Bedeutung |
|---|---|---|
| `DATABASE_URL` | – (Pflicht) | `postgres://user:pw@host:5432/db` |
| `DATABASE_MAX_CONNECTIONS` | `10` | Pool-Größe (≥ 4) |
| `HTTP_BIND` / `METRICS_BIND` | `0.0.0.0:8080` / `0.0.0.0:9090` | API bzw. Prometheus |
| `LOG_FORMAT` / `RUST_LOG` | `json` / `info,sqlx=warn` | Logging |
| `MIGRATE_ON_STARTUP` | `true` | sqlx-Migrationen beim Start |
| `SCRAPE_ENABLED` | `true` | Scheduler an/aus (API läuft immer) |
| `SCRAPE_CRON` / `SCRAPE_TIMEZONE` | `0 0 4,10,16,22 * * *` / `Europe/Berlin` | Zeitplan (`cron`-Crate: sec min hour dom mon dow) |
| `SCRAPE_BOOTSTRAP` / `SCRAPE_BOOTSTRAP_MAX_AGE` | `true` / `8h` | Scrape beim Start, wenn kein junger Lauf existiert |
| `SCRAPE_STALE_RUN_AFTER` | `2h` | hängende `running`-Läufe werden `failed` |
| `SCRAPE_*_DELAY`, `SCRAPE_RETRY_TOTAL`, `SCRAPE_USER_AGENT` … | s. Vertrag | Höflichkeits-Delays, Retries, UA |
| `SNAPSHOT_REVALIDATE_INTERVAL` | `30s` | Revalidierung des Snapshot-Caches gegen die DB (neuer Lauf eines anderen Replikats) |
| `ADMIN_TOKEN` | leer | aktiviert `POST /api/v1/admin/scrape` (leer ⇒ 404) |
| `INSTANCE_NAME` | `$HOSTNAME` | Kennung in `scrape_runs.instance` |

Frontend (nur Dev-Server, per Shell exportiert): `API_PROXY_TARGET` – Proxy-Ziel für `/api` (Default
`http://localhost:8080`); `MOCK_API=1` (`pnpm dev:mock`) ⇒ Mock-API statt Proxy. Frontend-Container:
`BACKEND_URL` (z. B. `http://backend:8080`).

## API-Übersicht

| Methode & Pfad | Antwort |
|---|---|
| `GET /healthz` | `200 {"status":"ok"}` |
| `GET /readyz` | `200` wenn DB erreichbar und nicht im Shutdown, sonst `503` |
| `GET /metrics` (Port 9090) | Prometheus-Text (`scrape_runs_total`, `scrape_last_success_timestamp_seconds`, `http_request_duration_seconds`, …) |
| `GET /api/v1/metadata` | Kennzahlen des letzten Laufs (Anzahl, günstigste/stärkste Angebote) + `run` |
| `GET /api/v1/strains` | alle Sorten mit Angeboten, `sort`/`search`-Hilfsfeldern und `trend`; `ETag`/`304` |
| `GET /api/v1/strains/{id}` | Sorte inkl. `first_seen_at`, `last_seen_at`, `in_latest_run` |
| `GET /api/v1/strains/{id}/history?from&to&bucket=run\|day&pharmacies=true` | Preisverlauf (min/avg/max, €/g THC, optional pro Apotheke) |
| `GET /api/v1/runs`, `GET /api/v1/runs/{id}` | Läufe (neueste zuerst), Details inkl. Fehlern |
| `GET /api/v1/pharmacies` | Apotheken inkl. Angebotszahl im letzten Lauf |
| `GET /api/v1/export.csv?run_id=` | 11 Spalten wie früher (`apotheke,…,produkt_url`), Scrape-Reihenfolge |
| `GET /api/v1/export.json?run_id=` | bare Array – Drop-in für `flowers.json` |
| `POST /api/v1/admin/scrape` | `Authorization: Bearer <ADMIN_TOKEN>` → `202 {"run_id","status":"running"}`, `409` wenn bereits ein Lauf aktiv |

Fehler haben immer die Form `{"error":{"code":"not_found|bad_request|unauthorized|conflict|no_data|internal","message":"…"}}`
(`405` nutzt `bad_request`, `408` bei `HTTP_REQUEST_TIMEOUT` nutzt `internal`).

## Container-Images bauen

```bash
docker build -t greenmedical-backend:dev  backend/    # cargo-chef → distroless, UID 65532, Ports 8080/9090
docker build -t greenmedical-frontend:dev frontend/   # pnpm + quasar build → nginx-unprivileged, UID 101, Port 8080
docker run --rm greenmedical-backend:dev --help
```

Beide Images laufen ohne Root, ohne Shell (Backend) und mit Read-only-Root-Dateisystem
(Frontend benötigt `/tmp` und `/etc/nginx/conf.d` schreibbar – im Chart und in compose bereits vorgesehen).

## Deployment mit Helm

Das Chart verwaltet **keine** Datenbank. Es erwartet ein Secret mit der Verbindungs-URL:

```bash
kubectl -n greenmedical create secret generic greenmedical-db \
  --from-literal=DATABASE_URL='postgres://greenmedical:GEHEIM@db.example.internal:5432/greenmedical'

helm upgrade --install greenmedical oci://ghcr.io/luebke-dev/charts/greenmedical --version 0.1.0 \
  -n greenmedical --create-namespace \
  --set database.existingSecret=greenmedical-db \
  --set ingress.enabled=true --set ingress.className=nginx \
  --set 'ingress.hosts[0].host=preise.example.com' \
  --wait --atomic
helm -n greenmedical test greenmedical
```

`ingress.hosts[].paths` darf entfallen (Default: `/` mit `pathType: Prefix`); mit `--set` auf einen Listenindex
wird die Default-Liste ohnehin komplett ersetzt. Das `--set`-Argument mit `[0]` gehört in Anführungszeichen,
sonst interpretiert zsh die Klammern als Glob (`no matches found`). `networkPolicy.enabled=true` verlangt zusätzlich einen
Weg zur Datenbank (`networkPolicy.extraEgressCIDRs` und/oder `networkPolicy.dbPodSelector`), sonst bricht
das Rendering ab.

Mit [CloudNativePG](https://cloudnative-pg.io/) genügt ein `Cluster` `greenmedical-db`; das Chart liest dann
das automatisch erzeugte Secret:

```yaml
database:
  existingSecret: greenmedical-db-app
  existingSecretKey: uri
adminToken:
  existingSecret: greenmedical-admin
metrics:
  serviceMonitor:
    enabled: true
    labels: { release: kube-prometheus-stack }
networkPolicy:
  enabled: true
  dbPodSelector: { matchLabels: { cnpg.io/cluster: greenmedical-db } }
```

Alle Werte, das vollständige CNPG-Beispiel, NetworkPolicy-Details und der minikube-Smoke-Test stehen in
[`charts/greenmedical/README.md`](charts/greenmedical/README.md). Für Entwicklungs-Cluster gibt es
[`charts/greenmedical/examples/postgres-dev.yaml`](charts/greenmedical/examples/postgres-dev.yaml).

## Release-Prozess

* Jeder Push auf `main`, dessen `CI`-Workflow erfolgreich war (`workflow_run`-Trigger), baut und pusht Images mit den Tags `sha-<kurz>` und `main` sowie ein
  Prerelease-Chart `0.1.0-main.<run>.<sha>` (App-Version `sha-<kurz>`).
* Ein Release entsteht durch einen Git-Tag:

  ```bash
  # charts/greenmedical/Chart.yaml: version/appVersion anheben, backend/Cargo.toml + frontend/package.json angleichen
  git tag -a v0.2.0 -m "v0.2.0"
  git push origin v0.2.0
  ```

  `release.yml` baut beide Images (Tags `0.2.0`, `0.2`, `latest`, `sha-…`), erzeugt SBOM + Provenance,
  signiert Images und Chart keyless mit cosign (Sigstore) und pusht das Chart als Version `0.2.0`.
  Ein Vorab-Tag wie `v0.2.0-rc.1` erzeugt Images `0.2.0-rc.1` + `sha-…` (kein `0.2`, kein `latest`) und
  ein Chart `0.2.0-rc.1` mit `artifacthub.io/prerelease: "true"`.
* Signatur prüfen:

  ```bash
  cosign verify ghcr.io/luebke-dev/greenmedical-backend:0.2.0 \
    --certificate-identity-regexp 'https://github.com/luebke-dev/greenmedical-price-scraper/.github/workflows/release.yml@.*' \
    --certificate-oidc-issuer https://token.actions.githubusercontent.com
  ```

Abhängigkeiten aktualisiert [Renovate](renovate.json) (gruppiert nach Cargo, npm, GitHub Actions, Images;
PostgreSQL-Major-Upgrades sind bewusst ausgenommen).

## Betrieb

* **Advisory-Lock ⇒ Session-Pooling.** Scheduler und Migrationen nutzen `pg_try_advisory_lock`, das an die
  Verbindung gebunden ist. PgBouncer/CNPG-Pooler nur im `session`-Modus einsetzen, nie `transaction`.
* **Mehrere Replikas** sind sicher; die Instanzen ohne Lock loggen `lock held, skipping`
  (Metrik `scrape_lock_skipped_total`).
* **Speicherwachstum**: ~2 000 Angebote × 4 Läufe/Tag ≈ 1 GB pro Jahr in `offers`. Retention/Rollup ist
  noch nicht implementiert; bei Bedarf alte Läufe per SQL löschen (`DELETE FROM scrape_runs WHERE started_at < …`
  kaskadiert auf `offers`).
* **Rate-Limits**: die Delays (`SCRAPE_PHARMACY_DELAY`, `SCRAPE_PAGE_DELAY`) und der User-Agent sollten
  beibehalten werden; `scrape_http_retries_total` beobachten.
* **Shutdown**: SIGTERM bricht einen laufenden Scrape ab (Lauf wird `failed`); dafür
  `terminationGracePeriodSeconds: 60` im Chart.
* **Debugging** im distroless Backend-Image: `kubectl debug -it <pod> --image=busybox --target=backend`;
  `migrate` und `scrape-once` sind CLI-Subcommands, kein `exec` nötig.
* **Ablösung der GitHub-Pages-Version**: In den Repository-Einstellungen *Pages* deaktivieren
  (Quelle „None“) und die Umgebung `github-pages` entfernen; unter *Packages* die GHCR-Packages
  `greenmedical-backend`, `greenmedical-frontend` und `charts/greenmedical` auf **public** stellen, damit
  `helm pull`/`docker pull` ohne Login funktionieren.
