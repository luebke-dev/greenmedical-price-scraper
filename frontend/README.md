# GreenMedical Livebestand – Frontend

Quasar 2 / Vue 3 / TypeScript SPA (`@quasar/app-vite` v3, pnpm). Zeigt Kennzahlen, die
filter- und sortierbare Preistabelle sowie Preisentwicklung, Bewertungen und Angebote pro Sorte
(`/sorte/:id`).
Die Daten kommen ausschließlich vom Backend unter `/api/v1` (siehe `docs/api-contract.md`).

## Entwicklung

```bash
pnpm install
pnpm dev            # http://localhost:9000, /api → http://localhost:8080 (laufendes Backend)
```

Der Dev-Server reicht `/api` an das Backend weiter – standardmäßig `http://localhost:8080`
(`cargo run` im `backend/`), per `API_PROXY_TARGET` an ein anderes Ziel:

```bash
API_PROXY_TARGET=http://localhost:18080 pnpm dev
```

Ohne Backend beantwortet ein kleines Vite-Plugin (`dev/mock-api.ts`) alle `/api/v1/*`-Anfragen
aus `dev/fixtures/*.json` (30 Sorten, 40 Läufe, Verlauf für 10 Sorten, Bewertungen; Zeitstempel
werden beim Start so verschoben, dass der letzte Lauf 45 Minuten zurückliegt):

```bash
pnpm dev:mock       # = MOCK_API=1 pnpm dev; kein Proxy
```

Hinweis: Ein schlichtes `pnpm dev` **ohne** laufendes Backend liefert keine Mock-Daten, sondern
zeigt „Daten konnten nicht geladen werden." (mit „Erneut laden"-Button) – der Mock ist nur mit
`MOCK_API=1` aktiv.

Fixtures neu erzeugen (aus einer gescrapten CSV im alten `greenmedical_flowers.csv`-Format):

```bash
pnpm fixtures path/to/greenmedical_flowers.csv
```

Bewertungen kommen nicht aus der CSV: `dev/fixtures/reviews.json` hält pro Sorten-ID
`product_uuid` und `rating` (`null` = nie gescrapt, `count: 0` = gescrapt ohne Bewertungen). Die
Mock-API mischt das beim Start in `strains`/`metadata.best_rated` und erzeugt die Rezensionen
für `/api/v1/strains/{id}/reviews` deterministisch aus der Sorten-ID (Sortierung, `limit`/`offset`
wie im Backend).

## Qualität

```bash
pnpm lint          # prettier --write + eslint --fix
pnpm lint:check    # nur prüfen (CI)
pnpm typecheck     # vue-tsc --noEmit
pnpm test          # vitest (lib/* + Komponenten, happy-dom)
pnpm build         # dist/spa – ECharts landet in einem eigenen Chunk (PriceHistoryChart-*.js)
```

## Bewusste Abweichungen zur alten statischen Seite (`site/app.js`, siehe Git-Historie)

Suche, Filter, Sortierung, Aufklappen und Formatierung sind 1:1 portiert. Drei Details weichen
absichtlich ab:

- **Sortierung:** Zeilen ohne Wert (z. B. ohne Preis) stehen in **beiden** Sortierrichtungen
  zuletzt (`src/lib/sort.ts`; app.js sortierte sie bei „absteigend" nach oben).
- **Slider-Grenzen:** `floorToStep`/`ceilToStep` (`src/lib/filter.ts`) runden auf die
  Nachkommastellen des Schritts und lassen exakte Vielfache stehen. app.js rechnete
  `Math.floor(min / step) * step`, wodurch Fließkomma-Rauschen exakte Werte eine Stufe nach
  außen schob (CBD-Minimum 0,3 → `0.3 / 0.1 = 2.9999999999999996` → Slider ab 0,20). Die
  Slider-Grenzen entsprechen jetzt immer den angezeigten Werten.
- **Aufgeklappte Zeilen:** Der Aufklapp-Zustand überlebt Filter- und Sortieränderungen; nur
  Zeilen, die aus der Ergebnismenge fallen, werden eingeklappt (app.js rendert bei jedem
  `applyFilters` neu und klappte dadurch alles zu).

## Struktur

```
src/api          Typen (1:1 API-Vertrag), fetchJson/ApiError, Endpunkte
src/lib          reine Logik: format, filter, search, sort, trend, history, url-state, chart
src/stores       Pinia: catalog (Metadata + Sorten), history (Verlaufs-Cache),
                 reviews (Bewertungen je Sorte + Sortierung, „Mehr anzeigen“ hängt an), navigation
src/composables  useStrainFilters (Filter ⇄ URL ?q&genetik&preis&thc&cbd&sort&dir), useHistoryQuery
src/components   AppHeader, MetricCards, FilterToolbar/FilterPanel, StrainTable, OffersTable,
                 TrendIndicator, HistoryControls, PriceHistoryChart (vue-echarts, async),
                 RatingStars, ReviewsSection, OfferHistoryTable, …
src/pages        IndexPage (keep-alive), StrainPage (/sorte/:id), ErrorNotFound
src/css          tokens.scss (Design-Tokens hell/dunkel), app.scss, quasar.variables.scss
dev/             Mock-API-Plugin + Fixtures (nicht im Produktions-Build)
test/            Vitest-Specs
```

## Bewertungen

- Übersicht: Spalte „Bewertung“ (`★ 4,3 (124)`) nach „Apotheken“, sortierbar über `?sort=rating`;
  Sorten ohne Bewertung stehen in beiden Richtungen zuletzt, der erste Klick sortiert absteigend.
- Kennzahl „Bestbewertet“ (10. Karte) aus `metadata.best_rated`.
- Sortenseite: Abschnitt „Bewertungen“ zwischen Preisentwicklung und aktuellen Angeboten –
  Zusammenfassung (Ø, Anzahl, Anteil verifizierter Käufe, Verteilung 5→1), Sortierung
  (neueste/älteste/beste/schlechteste), Liste, „Mehr anzeigen“ in 50er-Schritten (`offset`).
  „Bewertungen noch nicht erfasst“ solange `summary.scraped_at` null ist, sonst
  „Noch keine Bewertungen“ bei `count: 0`.

## Container

```bash
podman build -t greenmedical-frontend:dev .
podman run --rm -p 127.0.0.1:8089:8080 --add-host backend:host-gateway \
  -e BACKEND_URL=http://backend:8080 greenmedical-frontend:dev
```

nginx löst den Host aus `BACKEND_URL` beim Start auf – er muss also auflösbar sein (Compose-/K8s-
Service-Name; standalone wie oben per `--add-host`), sonst beendet sich der Container sofort.

Das Image basiert auf `nginxinc/nginx-unprivileged` (UID 101, Port **8080**). nginx liefert die
SPA (History-Mode-Fallback, `index.html` mit `no-cache`, `/assets/` immutable), setzt
Security-Header inkl. CSP, cacht Icons einen Tag, antwortet auf `/healthz` und reicht `/api/` an
das Backend weiter.

| Variable           | Wo                | Default                 | Bedeutung                                             |
| ------------------ | ----------------- | ----------------------- | ----------------------------------------------------- |
| `BACKEND_URL`      | Container (nginx) | `http://backend:8080`   | Ziel von `location /api/` (ohne abschließenden Slash) |
| `API_PROXY_TARGET` | `pnpm dev`        | `http://localhost:8080` | Vite-Proxy-Ziel für `/api`                            |
| `MOCK_API`         | `pnpm dev`        | – (aus)                 | `1` ⇒ Mock-API aus `dev/fixtures`, kein Proxy         |
