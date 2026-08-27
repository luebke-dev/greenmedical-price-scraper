# Helm-Chart `greenmedical`

Deployt den **GreenMedical Livebestand** auf Kubernetes:

| Komponente | Image | Ports | Aufgabe |
|---|---|---|---|
| `backend` | `ghcr.io/luebke-dev/greenmedical-backend` | `8080` (HTTP-API), `9090` (Prometheus) | Scraper-Scheduler (stündlich), REST-API `/api/v1`, Migrationen |
| `frontend` | `ghcr.io/luebke-dev/greenmedical-frontend` | `8080` (Container) → Service `80` | nginx mit Quasar-SPA, proxied `/api/` an das Backend |

Das Chart legt **keine Datenbank** an. Eine PostgreSQL-Instanz (≥ 14, getestet mit 17) muss extern
bereitgestellt werden – z. B. per [CloudNativePG](https://cloudnative-pg.io/), einer Managed-DB oder
für Entwicklungs-Cluster über `examples/postgres-dev.yaml`.

* Chart: `oci://ghcr.io/luebke-dev/charts/greenmedical`
* Kubernetes ≥ 1.28, Helm ≥ 3.14 (entwickelt mit Helm 4.2)
* Vertrag für Env-Variablen und API: [`docs/api-contract.md`](../../docs/api-contract.md)

## Schnellstart

```bash
# 1. Secret mit der Verbindungs-URL anlegen (oder von CloudNativePG erzeugen lassen, s. u.)
kubectl -n greenmedical create secret generic greenmedical-db \
  --from-literal=DATABASE_URL='postgres://greenmedical:GEHEIM@db.example.internal:5432/greenmedical'

# 2. Chart installieren
helm upgrade --install greenmedical oci://ghcr.io/luebke-dev/charts/greenmedical \
  --version 0.1.0 \
  --namespace greenmedical --create-namespace \
  --set database.existingSecret=greenmedical-db \
  --set ingress.enabled=true \
  --set 'ingress.hosts[0].host=preise.example.com' \
  --wait --atomic

# 3. Verbindungstest
helm -n greenmedical test greenmedical
```

`ingress.hosts[].paths` ist optional (Default `/`, `pathType: Prefix`) – nötig, weil `--set` auf einen
Listenindex die gesamte Default-Liste ersetzt. Das `[0]` im `--set`-Argument in Anführungszeichen setzen,
sonst behandelt zsh es als Glob (`no matches found`).

Ohne `database.existingSecret` **oder** `database.url` bricht das Rendering mit einer klaren Fehlermeldung ab.
Weitere Guards: `networkPolicy.enabled=true` ohne DB-Egress (`extraEgressCIDRs`/`dbPodSelector`) und
`*.autoscaling.enabled=true` ohne CPU-/Memory-Ziel schlagen ebenfalls beim Rendern fehl.

## Datenbank

### Variante A – bestehendes Secret (empfohlen)

```yaml
database:
  existingSecret: greenmedical-db     # Secret im Release-Namespace
  existingSecretKey: DATABASE_URL     # Key mit postgres://user:pw@host:5432/db
```

### Variante B – CloudNativePG

CloudNativePG erzeugt für jeden Cluster ein Secret `<cluster>-app` mit dem Key `uri`:

```yaml
# cnpg-cluster.yaml
apiVersion: postgresql.cnpg.io/v1
kind: Cluster
metadata:
  name: greenmedical-db
  namespace: greenmedical
spec:
  imageName: ghcr.io/cloudnative-pg/postgresql:17
  instances: 2
  primaryUpdateStrategy: unsupervised
  storage:
    size: 10Gi
  bootstrap:
    initdb:
      database: greenmedical
      owner: greenmedical
  postgresql:
    parameters:
      max_connections: "100"
  resources:
    requests:
      cpu: 100m
      memory: 256Mi
    limits:
      memory: 1Gi
```

```yaml
# values.yaml
database:
  existingSecret: greenmedical-db-app
  existingSecretKey: uri
networkPolicy:
  enabled: true
  dbPodSelector:
    matchLabels:
      cnpg.io/cluster: greenmedical-db
```

> **Kein PgBouncer im Transaction-Modus.** Der Scheduler serialisiert Läufe über einen
> PostgreSQL-Advisory-Lock, der an die Session gebunden ist. Ein CNPG-`Pooler` muss deshalb mit
> `poolMode: session` betrieben werden – oder das Backend spricht direkt mit dem `-rw`-Service (Default).

### Variante C – URL im Values-File (nur Entwicklung)

```yaml
database:
  url: postgres://greenmedical:greenmedical@postgres:5432/greenmedical
```

Das Chart legt daraus das Secret `<release>-greenmedical-backend` an. Für Entwicklungs-Cluster liefert
`examples/postgres-dev.yaml` ein passendes Postgres-17-Deployment (Secret + PVC + Deployment + Service `postgres`):

```bash
kubectl -n greenmedical apply -f charts/greenmedical/examples/postgres-dev.yaml
```

## Admin-Token

`POST /api/v1/admin/scrape` (manueller Lauf) ist nur aktiv, wenn `ADMIN_TOKEN` gesetzt ist:

```yaml
adminToken:
  existingSecret: greenmedical-admin   # Key: adminToken.existingSecretKey (Default ADMIN_TOKEN)
# oder – nur Entwicklung –
adminToken:
  value: dev-token
```

## Preisalarm-E-Mails

Das Backend verschickt Bestätigungs- und Preisalarm-Mails (siehe `docs/api-contract.md`, „Preisalarm-Abos“).
Ohne `email.enabled=true` werden Mails nur geloggt – Abos lassen sich dann nicht bestätigen.

```yaml
backend:
  config:
    publicUrl: https://preise.example.com   # Basis für Links in Mails; Default: erster Ingress-Host
    subscriptionRateLimit: "5/1h"           # optional, Backend-Default
email:
  enabled: true
  from: "GreenMedical Livebestand <noreply@example.com>"
  smtp:
    host: smtp.example.com
    port: 587
    tls: starttls                           # starttls | tls | none
    existingSecret: greenmedical-smtp       # Keys SMTP_USERNAME / SMTP_PASSWORD (beide optional)
# oder – nur Entwicklung – email.smtp.username / email.smtp.password (landen im Chart-Secret)
```

```bash
kubectl -n greenmedical create secret generic greenmedical-smtp \
  --from-literal=SMTP_USERNAME=preisalarm --from-literal=SMTP_PASSWORD='GEHEIM'
```

`backend.config.publicUrl` ist Pflicht, wenn kein Ingress aktiv ist; mit Ingress wird
`https://<erster Host>` (bzw. `http://` ohne `ingress.tls`) verwendet. Mit `networkPolicy.enabled=true`
entsteht zusätzlich eine Egress-Regel auf `email.smtp.port` – Ziel per `networkPolicy.smtp.podSelector`
(Relay im Cluster) und/oder `networkPolicy.smtp.cidrs`; ohne Angabe gilt der Bereich von
`networkPolicy.externalEgress` (Relay im Internet, private Netze ausgenommen).

## Wichtige Werte

Vollständige Liste mit Kommentaren: [`values.yaml`](values.yaml). Typen und Enums werden über
[`values.schema.json`](values.schema.json) geprüft.

| Wert | Default | Beschreibung |
|---|---|---|
| `database.existingSecret` / `existingSecretKey` | `""` / `DATABASE_URL` | Secret mit Verbindungs-URL (Pflicht, alternativ `database.url`) |
| `database.url` | `""` | Verbindungs-URL, Chart erzeugt Secret (nur Dev) |
| `adminToken.existingSecret` / `value` | `""` | aktiviert den Admin-Endpoint |
| `backend.replicaCount` | `1` | mehrere Replikas sind sicher – nur eine scrapt (Advisory-Lock) |
| `backend.image.tag` / `digest` | `appVersion` / `""` | Image-Version |
| `backend.config.scrapeEnabled` | `true` | Scheduler an/aus (API läuft immer) |
| `backend.config.scrapeCron` | `0 0 * * * *` | `cron`-Crate-Format (sec min hour dom mon dow) |
| `backend.config.scrapeTimezone` | `Europe/Berlin` | IANA-Zeitzone für den Cron |
| `backend.config.scrapeBootstrap` | `true` | beim Start scrapen, wenn kein Lauf jünger als `scrapeBootstrapMaxAge` (2h) |
| `backend.config.migrateOnStartup` | `true` | sqlx-Migrationen beim Start (replikasicher) |
| `backend.config.logFormat` | `json` | `json` \| `pretty` |
| `backend.config.databaseMaxConnections` | `10` | Pool-Größe (≥ 4) |
| `backend.config.httpRequestTimeout` | `30s` | serverseitiges Request-Timeout (`408`) |
| `backend.config.snapshotRevalidateInterval` | `""` (Backend-Default `30s`) | wie oft der Snapshot-Cache prüft, ob ein anderes Replikat einen neueren Lauf gespeichert hat |
| `backend.config.corsAllowedOrigins` | `""` | nur nötig, wenn die API ohne nginx-Proxy direkt aufgerufen wird |
| `backend.config.publicUrl` | `""` (= erster Ingress-Host) | Basis-URL für Links in E-Mails; Pflicht ohne Ingress |
| `backend.config.subscriptionRateLimit` | `""` (Backend-Default `5/1h`) | Abos/Bestätigungsmails pro IP |
| `email.enabled` | `false` | Mailversand via SMTP; `false` = nur loggen |
| `email.from` | `""` (Backend-Default) | Absender |
| `email.smtp.host` / `port` / `tls` | `""` / `587` / `starttls` | SMTP-Server (`host` Pflicht bei `email.enabled`) |
| `email.smtp.existingSecret` | `""` | Secret mit `SMTP_USERNAME`/`SMTP_PASSWORD` (alternativ `username`/`password`, nur Dev) |
| `backend.config.extra` | `{}` | weitere Env-Variablen → ConfigMap |
| `backend.resources` | 50m/64Mi, Limit 256Mi | kein CPU-Limit (bewusst) |
| `backend.terminationGracePeriodSeconds` | `60` | Zeit, um einen laufenden Scrape sauber abzubrechen |
| `backend.autoscaling.*` | aus | HPA `autoscaling/v2` (CPU, optional Memory) |
| `backend.podDisruptionBudget.*` | an, `minAvailable: 1` | nur gerendert, wenn effektiv > 1 Replika |
| `frontend.replicaCount` | `1` | |
| `frontend.service.port` | `80` | Container lauscht auf 8080 |
| `ingress.enabled` / `className` / `hosts` / `tls` / `annotations` | aus | zeigt immer auf das Frontend; `/api` wird von nginx proxied, `/metrics` ist nicht erreichbar; `hosts[].paths` optional (Default `/`) |
| `metrics.serviceMonitor.enabled` | `false` | Prometheus-Operator `ServiceMonitor` auf Port `metrics` (9090) |
| `metrics.serviceMonitor.labels` | `{}` | z. B. `release: kube-prometheus-stack` |
| `networkPolicy.enabled` | `false` | siehe unten; verlangt `extraEgressCIDRs` und/oder `dbPodSelector` |
| `networkPolicy.extraEgressCIDRs` | `[]` | DB-Egress zu externen Adressen (Port `networkPolicy.dbPort`) |
| `networkPolicy.dbPodSelector` / `dbNamespaceSelector` | `{}` | DB-Egress zu Pods im Cluster (z. B. CNPG) |
| `networkPolicy.smtp.cidrs` / `podSelector` / `namespaceSelector` | `[]` / `{}` | SMTP-Egress-Ziel (nur bei `email.enabled`); leer = wie `externalEgress` |
| `tests.enabled` | `true` | `helm test`-Pod (curl) |
| `tests.requireData` | `false` | Test schlägt fehl, wenn `/api/v1/metadata` noch 404 (kein Lauf) liefert |
| `serviceAccount.create` | `true` | Token wird nicht gemountet (`automountServiceAccountToken: false`) |
| `imagePullSecrets` | `[]` | z. B. `[{name: ghcr-pull}]` für private GHCR-Packages |

## Sicherheit

Beide Workloads laufen mit `runAsNonRoot`, `readOnlyRootFilesystem`, `capabilities.drop: [ALL]`,
`allowPrivilegeEscalation: false` und `seccompProfile: RuntimeDefault`. Das Backend (distroless) läuft als
UID 65532, das Frontend (nginx-unprivileged) als UID 101. Schreibbare Pfade sind `emptyDir`s
(`/tmp`; Frontend zusätzlich `/etc/nginx/conf.d` für die envsubst-Konfiguration).

### NetworkPolicy

Mit `networkPolicy.enabled=true` entstehen zwei Policies:

* **Backend** – Ingress nur vom Frontend (8080), von `networkPolicy.monitoring` (9090) und vom Test-Pod.
  Egress zu DNS, per HTTPS (443) ins Internet (für `greenmedical.health`; private Bereiche ausgenommen)
  sowie zur Datenbank via `extraEgressCIDRs` und/oder `dbPodSelector`; bei `email.enabled` zusätzlich zum
  SMTP-Server (`email.smtp.port`, Ziel aus `networkPolicy.smtp`). **Mindestens eines davon ist
  Pflicht** (alternativ eine eigene Regel in `backend.extraEgress`) – ohne DB-Egress würde das Backend nie
  `ready`, deshalb bricht das Chart in diesem Fall beim Rendern ab.
* **Frontend** – Ingress nur von `networkPolicy.ingressController` (Default: Namespace `ingress-nginx`)
  und vom Test-Pod; Egress zu DNS und zum Backend.

Weitere Regeln lassen sich über `networkPolicy.{backend,frontend}.extraIngress/extraEgress` ergänzen.

## Betrieb

* **Migrationen** laufen beim Start des Backends (`MIGRATE_ON_STARTUP=true`) und sind über einen
  Advisory-Lock gegen parallele Replikas abgesichert. Die Rolling-Update-Strategie (`maxUnavailable: 0`)
  in Kombination mit `helm upgrade --atomic` rollt fehlgeschlagene Starts sauber zurück.
  Manuell: `kubectl -n greenmedical run migrate --rm -it --image=ghcr.io/luebke-dev/greenmedical-backend:0.1.0 --env DATABASE_URL=… -- migrate`
* **Probes**: Startup `/readyz` (bis 3 min für DB + Migrationen), Liveness `/healthz`, Readiness `/readyz`.
* **Metriken**: Port 9090 (`/metrics`), Service-Port-Name `metrics`. `ServiceMonitor` optional.
* **Skalierung**: mehrere Backend-Replikas sind sicher; die anderen Instanzen loggen `lock held, skipping`.
* **Distroless**: kein Shell im Backend-Image. Debugging über `kubectl debug -it <pod> --image=busybox --target=backend`.
* **Checksum-Annotationen** sorgen dafür, dass Änderungen an ConfigMap/Secret des Charts einen Rollout auslösen.
  Änderungen an *externen* Secrets (`existingSecret`) erfordern `kubectl rollout restart deployment/<release>-greenmedical-backend`.

## `helm test`

Der Test-Pod (`curlimages/curl`) prüft `GET /healthz`, `/readyz`, `/api/v1/runs`, `/api/v1/metadata`
am Backend sowie `/healthz`, `/` und `/api/v1/runs` über den nginx-Proxy des Frontends.
`/api/v1/metadata` darf `404` liefern, solange noch kein Lauf existiert (`tests.requireData=false`).

## Smoke-Test auf minikube

```bash
minikube start --driver=podman
minikube addons enable ingress
minikube image build -t greenmedical-backend:dev backend/
minikube image build -t greenmedical-frontend:dev frontend/

kubectl --context minikube create namespace greenmedical
kubectl --context minikube -n greenmedical apply -f charts/greenmedical/examples/postgres-dev.yaml
helm --kube-context minikube -n greenmedical upgrade --install greenmedical charts/greenmedical \
  -f charts/greenmedical/ci/minikube-values.yaml --wait --timeout 5m
helm --kube-context minikube -n greenmedical test greenmedical

# Ingress-Host greenmedical.local → minikube ip in /etc/hosts eintragen
kubectl --context minikube -n greenmedical logs deploy/greenmedical-backend | grep -i lock
helm --kube-context minikube -n greenmedical uninstall greenmedical
minikube delete
```

`ci/minikube-values.yaml` startet zwei Backend-Replikas mit aktiviertem Bootstrap-Scrape – genau eine
Instanz führt den Lauf aus, die andere protokolliert den belegten Advisory-Lock.

## Entwicklung am Chart

```bash
helm lint --strict charts/greenmedical -f charts/greenmedical/ci/full-values.yaml
helm template gm charts/greenmedical -f charts/greenmedical/ci/full-values.yaml \
  | kubeconform -strict -schema-location default \
      -schema-location 'https://raw.githubusercontent.com/datreeio/CRDs-catalog/main/{{.Group}}/{{.ResourceKind}}_{{.ResourceAPIVersion}}.json'
helm template gm charts/greenmedical -f charts/greenmedical/ci/full-values.yaml --skip-tests \
  | kube-score score - --ignore-container-cpu-limit --ignore-test container-image-pull-policy \
      --ignore-test container-security-context-user-group-id
```

Die Varianten in `ci/` (`default`, `full`, `minikube`) werden in der CI (`.github/workflows/ci.yml`) mit
`helm lint --strict`, `kubeconform -strict` und `kube-score` geprüft.
