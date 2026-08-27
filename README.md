# green.luebke.dev

Preise, Verfügbarkeit, Preisverlauf und Bewertungen lieferbarer Cannabisblüten
von [greenmedical.health](https://greenmedical.health) und
[DrAnsay](https://shop.dransay.com). Angebote bleiben nach Anbieter getrennt;
doppelte DrAnsay-Produkte, Abholangebote und nicht verfügbare Ware werden verworfen.

**Live:** <https://green.luebke.dev>

## Aufbau

| Teil | Inhalt |
|---|---|
| [`backend/`](backend/README.md) | Rust (axum + sqlx). Scrapt GreenMedical und lieferfähige DrAnsay-Apotheken stündlich, speichert jeden Lauf in PostgreSQL und liefert die JSON-API unter `/api/v1`, `/healthz`, `/readyz`, `/metrics`. |
| [`frontend/`](frontend/README.md) | Quasar-SPA (Vue 3): Tabelle mit Suche/Filtern, Kennzahlen, Sortenseite mit Preisverlauf, Angebotshistorie und Bewertungen. |
| [`charts/greenmedical/`](charts/greenmedical/README.md) | Helm-Chart für Kubernetes (externe PostgreSQL). |
| [`docs/api-contract.md`](docs/api-contract.md) | API-Vertrag und alle Env-Variablen. |

## Lokal starten

```bash
cp .env.example .env
docker compose up -d                          # nur PostgreSQL
docker compose --profile app up -d --build    # + Backend (:8080) + Frontend (:8081) + Mailpit (:8025)
```

API-Dokumentation (OpenAPI): <http://localhost:8080/api/docs> (bzw. `/api/docs` hinter Frontend/Ingress).
Preisalarm-E-Mails landen in der Entwicklung im Mailpit-UI unter <http://localhost:8025>.

Entwicklung mit Hot Reload:

```bash
cd backend  && cargo run                      # API auf :8080 (DATABASE_URL aus .env)
cd frontend && pnpm install && pnpm dev       # UI auf :9000, /api → :8080 (ohne Backend: pnpm dev:mock)
```

Manueller Scrape-Lauf: `curl -X POST -H "Authorization: Bearer $ADMIN_TOKEN" localhost:8080/api/v1/admin/scrape`

## Deployment

Images und Chart werden bei jedem grünen `main`-Build und bei Tags `v*` nach `ghcr.io/luebke-dev/` veröffentlicht.

```bash
helm install greenmedical oci://ghcr.io/luebke-dev/charts/greenmedical \
  --set database.existingSecret=<secret-mit-DATABASE_URL> \
  --set ingress.enabled=true --set 'ingress.hosts[0].host=green.luebke.dev'
```

Details (Werte, CloudNativePG-Beispiel, Betrieb) im [Chart-README](charts/greenmedical/README.md).
