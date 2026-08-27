// Vite dev-server plugin that answers /api/v1/* from dev/fixtures/*.json.
// Registered in quasar.config.ts only for `quasar dev` with MOCK_API=1 (`pnpm dev:mock`).
// Not part of the production build.

import { readFile } from 'node:fs/promises';
import type { IncomingMessage, ServerResponse } from 'node:http';
import { join } from 'node:path';
import type { Plugin } from 'vite';
import type {
  History,
  HistoryPoint,
  Highlight,
  Metadata,
  Pharmacy,
  Rating,
  Review,
  ReviewSort,
  ReviewsResponse,
  PharmacySeries,
  PharmacySeriesPoint,
  Run,
  Strain,
  StrainDetail,
  StrainsResponse,
} from '../src/api/types';

export interface MockApiOptions {
  fixturesDir: string;
  /** Artificial latency per request (ms), handy to see loading states. */
  delayMs?: number;
}

interface HistoryFixture {
  points: HistoryPoint[];
  pharmacies: PharmacySeries[];
}

interface RatingFixture {
  product_uuid: string | null;
  rating: Rating | null;
}

interface Fixtures {
  metadata: Metadata;
  strains: StrainsResponse;
  history: Record<string, HistoryFixture>;
  runs: { runs: Run[]; total: number };
  reviews: Record<string, Review[]>;
}

const CSV_HEADER =
  'apotheke,apotheke_plz,apotheke_stadt,name,bezeichnung,genetik,thc,cbd,preis_pro_gramm,verfuegbarkeit,produkt_url';

const DAY_MS = 86_400_000;

// ---------------------------------------------------------------------------
// Fixture loading + time shifting
// ---------------------------------------------------------------------------

async function readJson<T>(dir: string, name: string): Promise<T> {
  return JSON.parse(await readFile(join(dir, name), 'utf8')) as T;
}

/**
 * The fixtures carry fixed timestamps. Shift them so the latest run finished
 * 45 minutes ago – then every preset (7d/30d/…) shows data regardless of today's date.
 */
function shiftTimestamps(fixtures: Fixtures): Fixtures {
  const latest = fixtures.strains.run;
  const anchor = new Date(latest.finished_at ?? latest.started_at).valueOf();
  const target = Date.now() - 45 * 60_000;
  const offset = target - anchor;
  const shift = (iso: string | null): string | null =>
    iso === null ? null : new Date(new Date(iso).valueOf() + offset).toISOString();
  const shiftRun = (run: Run): Run => ({
    ...run,
    started_at: shift(run.started_at) ?? run.started_at,
    finished_at: shift(run.finished_at),
  });
  const shiftStrain = (strain: Strain): Strain => ({
    ...strain,
    trend: strain.trend
      ? { ...strain.trend, reference_at: shift(strain.trend.reference_at) ?? '' }
      : null,
    rating: strain.rating
      ? {
          ...strain.rating,
          scraped_at: shift(strain.rating.scraped_at) ?? strain.rating.scraped_at,
        }
      : null,
  });

  const history: Record<string, HistoryFixture> = {};
  for (const [id, entry] of Object.entries(fixtures.history)) {
    history[id] = {
      points: entry.points.map((point) => ({ ...point, at: shift(point.at) ?? point.at })),
      pharmacies: entry.pharmacies.map((pharmacy) => ({
        ...pharmacy,
        points: pharmacy.points.map((point) => ({ ...point, at: shift(point.at) ?? point.at })),
      })),
    };
  }

  return {
    metadata: {
      ...fixtures.metadata,
      generated_at: shift(fixtures.metadata.generated_at) ?? fixtures.metadata.generated_at,
      run: shiftRun(fixtures.metadata.run),
    },
    strains: {
      run: shiftRun(fixtures.strains.run),
      reference_run: fixtures.strains.reference_run
        ? shiftRun(fixtures.strains.reference_run)
        : null,
      strains: fixtures.strains.strains.map(shiftStrain),
    },
    history,
    runs: { runs: fixtures.runs.runs.map(shiftRun), total: fixtures.runs.total },
    reviews: Object.fromEntries(
      Object.entries(fixtures.reviews).map(([id, reviews]) => [
        id,
        reviews.map((review) => ({
          ...review,
          first_seen_at: shift(review.first_seen_at) ?? review.first_seen_at,
        })),
      ]),
    ),
  };
}

// ---------------------------------------------------------------------------
// Ratings + reviews (dev/fixtures/reviews.json holds the per-strain rating; the review texts
// are generated deterministically from the strain id so the mock stays stable across restarts)
// ---------------------------------------------------------------------------

const REVIEW_AUTHORS = [
  'Carlos S.',
  'Anna M.',
  'Jonas K.',
  'Lea B.',
  'Mehmet Y.',
  'Sabine R.',
  'Tobias W.',
  'Nina H.',
  'Felix P.',
  'Miriam L.',
  'Daniel F.',
  'Katrin Z.',
  'Sven O.',
  'Julia T.',
  'Patrick N.',
  'Anonym',
];

const REVIEW_TEXTS: Record<number, string[]> = {
  5: [
    'Sehr angenehme Wirkung, hilft mir abends zuverlässig beim Einschlafen. Blüten sind frisch und gut getrimmt.',
    'Top Qualität, intensiver Geruch und gleichmäßige Blüten. Schmerzlinderung setzt schnell ein.',
    'Meine bisher beste Sorte. Sehr gut verträglich, keine Nebenwirkungen, klare Wirkung gegen die Verspannungen.',
    'Preis-Leistung stimmt hier absolut. Lieferung war schnell, Verpackung sauber. Klare Empfehlung.',
    'Wirkt entspannend, ohne dass ich müde werde. Genau das, was ich tagsüber brauche.',
  ],
  4: [
    'Gute Wirkung, etwas trocken bei mir angekommen. Insgesamt aber zufrieden.',
    'Hilft gut gegen die Schmerzen, der Geschmack ist allerdings nicht ganz meins.',
    'Solide Sorte für den Abend. Ein Stern Abzug, weil die letzte Charge kleinere Blüten hatte.',
    'Angenehm mild, gut dosierbar. Für den Tag etwas zu stark.',
  ],
  3: [
    'Wirkung okay, aber schwächer als erwartet. Die Charge davor war deutlich besser.',
    'Durchschnitt. Macht, was es soll, aber nichts Besonderes.',
    'Recht trocken und die Wirkung hält nicht lange an.',
  ],
  2: [
    'Für den Preis leider enttäuschend. Viele kleine Blüten und wenig Aroma.',
    'Wirkung kaum spürbar, hatte mir deutlich mehr erhofft.',
  ],
  1: [
    'Sehr trocken, kaum Geruch – wirkt wie lange gelagert. Bestelle ich nicht noch einmal.',
    'Hat bei mir gar nicht funktioniert und Kopfschmerzen verursacht.',
  ],
};

/** Small deterministic PRNG (mulberry32). */
function seeded(seed: number): () => number {
  let state = seed >>> 0;
  return () => {
    state = (state + 0x6d2b79f5) >>> 0;
    let t = state;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

function isoDay(date: Date): string {
  return date.toISOString().slice(0, 10);
}

/** ~`count` reviews whose mean is close to `value`; newest first (by reviewed_on). */
function generateReviews(strainId: number, rating: Rating, scrapedAt: string): Review[] {
  if (rating.value === null || rating.count === 0) return [];
  const random = seeded(strainId * 7919);
  const reviews: Review[] = [];
  const end = new Date(scrapedAt).valueOf();
  for (let index = 0; index < rating.count; index += 1) {
    // Rating around the average, clamped to half stars.
    const raw = rating.value + (random() - 0.5) * 2.2;
    const stars = Math.min(5, Math.max(1, Math.round(raw * 2) / 2));
    const whole = Math.min(5, Math.max(1, Math.round(stars))) as 1 | 2 | 3 | 4 | 5;
    const texts = REVIEW_TEXTS[whole] ?? [];
    const daysAgo = Math.floor(random() * 400);
    const reviewedOn = new Date(end - daysAgo * DAY_MS);
    reviews.push({
      id: strainId * 1000 + index + 1,
      author: REVIEW_AUTHORS[Math.floor(random() * REVIEW_AUTHORS.length)] ?? 'Anonym',
      reviewed_on: random() < 0.04 ? null : isoDay(reviewedOn),
      rating: stars,
      verified: random() < 0.7,
      content: random() < 0.08 ? '' : (texts[Math.floor(random() * texts.length)] ?? ''),
      first_seen_at: new Date(end - Math.min(daysAgo, 30) * DAY_MS).toISOString(),
    });
  }
  return reviews;
}

function applyRatings(
  metadata: Metadata,
  strains: StrainsResponse,
  ratings: Record<string, RatingFixture>,
): { metadata: Metadata; strains: StrainsResponse; reviews: Record<string, Review[]> } {
  const reviews: Record<string, Review[]> = {};
  let best = null as { strain: Strain; rating: Rating } | null;
  const list = strains.strains.map((strain) => {
    const fixture = ratings[String(strain.id)] ?? { product_uuid: null, rating: null };
    const next: Strain = {
      ...strain,
      rating: fixture.rating,
      product_uuid: fixture.product_uuid,
      sort: { ...strain.sort, rating: fixture.rating?.value ?? null },
    };
    if (fixture.rating) {
      reviews[String(strain.id)] = generateReviews(
        strain.id,
        fixture.rating,
        fixture.rating.scraped_at,
      );
      const value = fixture.rating.value;
      if (value !== null && fixture.rating.count >= 5) {
        const better =
          !best ||
          value > (best.rating.value ?? 0) ||
          (value === best.rating.value && fixture.rating.count > best.rating.count);
        if (better) best = { strain: next, rating: fixture.rating };
      }
    }
    return next;
  });
  return {
    metadata: { ...metadata, best_rated: best ? toHighlight(best.strain, best.rating) : null },
    strains: { ...strains, strains: list },
    reviews,
  };
}

function toHighlight(strain: Strain, rating: Rating): Highlight {
  const offer = strain.offers[0];
  return {
    price: strain.min_price,
    name: strain.name,
    apotheke: offer?.apotheke ?? '',
    genetik: strain.genetik,
    thc: strain.thc,
    cbd: strain.cbd,
    produkt_url: offer?.produkt_url ?? '',
    strain_id: strain.id,
    pharmacy_id: offer?.pharmacy_id ?? 0,
    rating_value: rating.value,
    review_count: rating.count,
  };
}

async function loadFixtures(dir: string): Promise<Fixtures> {
  const [rawMetadata, rawStrains, history, runs, ratings] = await Promise.all([
    readJson<Metadata>(dir, 'metadata.json'),
    readJson<StrainsResponse>(dir, 'strains.json'),
    readJson<Record<string, HistoryFixture>>(dir, 'history.json'),
    readJson<{ runs: Run[]; total: number }>(dir, 'runs.json'),
    readJson<Record<string, RatingFixture>>(dir, 'reviews.json'),
  ]);
  const { metadata, strains, reviews } = applyRatings(rawMetadata, rawStrains, ratings);
  return shiftTimestamps({ metadata, strains, history, runs, reviews });
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const berlinDay = new Intl.DateTimeFormat('sv-SE', {
  timeZone: 'Europe/Berlin',
  year: 'numeric',
  month: '2-digit',
  day: '2-digit',
});

function toBerlinDay(iso: string): string {
  return berlinDay.format(new Date(iso));
}

function round2(value: number): number {
  return Math.round(value * 100) / 100;
}

function minOf(values: (number | null)[]): number | null {
  const numbers = values.filter((v): v is number => v !== null);
  return numbers.length ? Math.min(...numbers) : null;
}

function maxOf(values: (number | null)[]): number | null {
  const numbers = values.filter((v): v is number => v !== null);
  return numbers.length ? Math.max(...numbers) : null;
}

function avgOf(values: (number | null)[]): number | null {
  const numbers = values.filter((v): v is number => v !== null);
  return numbers.length ? round2(numbers.reduce((a, b) => a + b, 0) / numbers.length) : null;
}

/** Groups run points into Berlin calendar days (same semantics as the backend's bucket=day). */
function bucketByDay(points: HistoryPoint[]): HistoryPoint[] {
  const groups = new Map<string, HistoryPoint[]>();
  for (const point of points) {
    const day = toBerlinDay(point.at);
    const list = groups.get(day) ?? [];
    list.push(point);
    groups.set(day, list);
  }
  return [...groups.entries()].map(([day, list]) => {
    const last = list[list.length - 1]!;
    return {
      run_count: list.length,
      at: day,
      min: minOf(list.map((p) => p.min)),
      avg: avgOf(list.map((p) => p.avg)),
      max: maxOf(list.map((p) => p.max)),
      min_per_thc_gram: minOf(list.map((p) => p.min_per_thc_gram)),
      avg_per_thc_gram: avgOf(list.map((p) => p.avg_per_thc_gram)),
      max_per_thc_gram: maxOf(list.map((p) => p.max_per_thc_gram)),
      offer_count: last.offer_count,
      pharmacy_count: last.pharmacy_count,
    };
  });
}

function bucketPharmacyByDay(points: PharmacySeriesPoint[]): PharmacySeriesPoint[] {
  const byDay = new Map<string, PharmacySeriesPoint>();
  for (const point of points) {
    byDay.set(toBerlinDay(point.at), {
      at: toBerlinDay(point.at),
      price: point.price,
      price_per_thc_gram: point.price_per_thc_gram,
      availability: point.availability,
    });
  }
  return [...byDay.values()];
}

function csvCell(value: string): string {
  return /[",\n]/.test(value) ? `"${value.replace(/"/g, '""')}"` : value;
}

function toCsv(strains: Strain[]): string {
  const lines = [CSV_HEADER];
  for (const strain of strains) {
    for (const offer of strain.offers) {
      lines.push(
        [
          offer.apotheke,
          offer.apotheke_plz,
          offer.apotheke_stadt,
          strain.name,
          strain.bezeichnung,
          strain.genetik,
          strain.thc,
          strain.cbd,
          offer.preis_pro_gramm,
          offer.verfuegbarkeit,
          offer.produkt_url,
        ]
          .map(csvCell)
          .join(','),
      );
    }
  }
  return `${lines.join('\n')}\n`;
}

function pharmaciesFrom(fixtures: Fixtures): Pharmacy[] {
  const byId = new Map<number, Pharmacy>();
  const run = fixtures.strains.run;
  for (const strain of fixtures.strains.strains) {
    for (const offer of strain.offers) {
      const existing = byId.get(offer.pharmacy_id);
      if (existing) {
        existing.offer_count_latest += 1;
        continue;
      }
      byId.set(offer.pharmacy_id, {
        id: offer.pharmacy_id,
        external_id: `00000000-0000-4000-8000-${String(offer.pharmacy_id).padStart(12, '0')}`,
        name: offer.apotheke,
        plz: offer.apotheke_plz,
        city: offer.apotheke_stadt,
        address: '',
        url: `https://greenmedical.health/de/apotheken/${offer.pharmacy_id}`,
        first_seen_at:
          fixtures.runs.runs[fixtures.runs.runs.length - 1]?.started_at ?? run.started_at,
        last_seen_at: run.finished_at ?? run.started_at,
        offer_count_latest: 1,
      });
    }
  }
  return [...byId.values()].sort((a, b) => a.name.localeCompare(b.name, 'de'));
}

// ---------------------------------------------------------------------------
// Request handling
// ---------------------------------------------------------------------------

interface Reply {
  status: number;
  body: string;
  headers?: Record<string, string>;
}

function json(status: number, payload: unknown, headers: Record<string, string> = {}): Reply {
  return {
    status,
    body: JSON.stringify(payload),
    headers: { 'Content-Type': 'application/json; charset=utf-8', ...headers },
  };
}

function error(status: number, code: string, message: string): Reply {
  return json(status, { error: { code, message } });
}

function handle(fixtures: Fixtures, method: string, url: URL, ifNoneMatch: string | null): Reply {
  const path = url.pathname.replace(/\/+$/, '') || '/';

  if (path === '/healthz') return json(200, { status: 'ok' });
  if (path === '/readyz') return json(200, { status: 'ready', db: 'ok' });
  if (!path.startsWith('/api/v1/')) return error(404, 'not_found', `unknown path ${path}`);

  const rest = path.slice('/api/v1'.length);

  if (rest === '/admin/scrape') {
    // ADMIN_TOKEN is empty in the mock → endpoint does not exist.
    return error(404, 'not_found', 'admin endpoint disabled');
  }
  if (method !== 'GET' && method !== 'HEAD') {
    return error(405, 'bad_request', 'method not allowed');
  }

  if (rest === '/metadata') return json(200, fixtures.metadata);

  if (rest === '/strains') {
    const etag = `"run-${fixtures.strains.run.id}"`;
    const headers = { ETag: etag, 'Cache-Control': 'public, max-age=300' };
    if (ifNoneMatch === etag) return { status: 304, body: '', headers };
    return json(200, fixtures.strains, headers);
  }

  const strainMatch = /^\/strains\/(\d+)(\/history|\/reviews)?$/.exec(rest);
  if (strainMatch) {
    const id = Number(strainMatch[1]);
    const strain = fixtures.strains.strains.find((item) => item.id === id);
    if (!strain) return error(404, 'not_found', `strain ${id} not found`);
    const run = fixtures.strains.run;
    if (!strainMatch[2]) {
      const oldest = fixtures.runs.runs[fixtures.runs.runs.length - 1] ?? run;
      const detail: StrainDetail = {
        ...strain,
        first_seen_at: oldest.started_at,
        last_seen_at: run.finished_at ?? run.started_at,
        in_latest_run: true,
        run,
      };
      return json(200, detail);
    }
    if (strainMatch[2] === '/reviews') return reviewsReply(fixtures, strain, url);
    return history(fixtures, id, url);
  }

  if (rest === '/runs') {
    const limit = Math.min(500, Math.max(1, Number(url.searchParams.get('limit') ?? 50) || 50));
    const offset = Math.max(0, Number(url.searchParams.get('offset') ?? 0) || 0);
    const status = url.searchParams.get('status');
    const runs = fixtures.runs.runs.filter((run) => !status || run.status === status);
    return json(200, { runs: runs.slice(offset, offset + limit), total: runs.length });
  }

  const runMatch = /^\/runs\/(\d+)$/.exec(rest);
  if (runMatch) {
    const run = fixtures.runs.runs.find((item) => item.id === Number(runMatch[1]));
    if (!run) return error(404, 'not_found', 'run not found');
    return json(200, { ...run, errors: [] });
  }

  if (rest === '/pharmacies') return json(200, { pharmacies: pharmaciesFrom(fixtures) });

  if (rest === '/export.json') {
    return json(200, fixtures.strains.strains, {
      'Content-Disposition': 'attachment; filename="flowers.json"',
    });
  }
  if (rest === '/export.csv') {
    return {
      status: 200,
      body: toCsv(fixtures.strains.strains),
      headers: {
        'Content-Type': 'text/csv; charset=utf-8',
        'Content-Disposition': 'attachment; filename="greenmedical_flowers.csv"',
      },
    };
  }

  return error(404, 'not_found', `unknown endpoint ${rest}`);
}

function history(fixtures: Fixtures, id: number, url: URL): Reply {
  const now = Date.now();
  const fromParam = url.searchParams.get('from');
  const toParam = url.searchParams.get('to');
  const bucket = url.searchParams.get('bucket') === 'day' ? 'day' : 'run';
  const includePartial = url.searchParams.get('include_partial') !== 'false';
  const withPharmacies = url.searchParams.get('pharmacies') === 'true';

  const to = toParam ? new Date(toParam) : new Date(now);
  const from = fromParam ? new Date(fromParam) : new Date(to.valueOf() - 90 * DAY_MS);
  if (Number.isNaN(from.valueOf()) || Number.isNaN(to.valueOf())) {
    return error(400, 'bad_request', 'invalid from/to');
  }
  if (to.valueOf() - from.valueOf() > 730 * DAY_MS) {
    return error(400, 'bad_request', 'span exceeds 730 days');
  }

  const fixture = fixtures.history[String(id)] ?? { points: [], pharmacies: [] };
  const inRange = (at: string) => {
    const t = new Date(at).valueOf();
    return t >= from.valueOf() && t <= to.valueOf();
  };
  let points = fixture.points.filter(
    (point) => inRange(point.at) && (includePartial || point.status !== 'partial'),
  );
  let pharmacies = fixture.pharmacies.map((pharmacy) => ({
    ...pharmacy,
    points: pharmacy.points.filter((point) => inRange(point.at)),
  }));

  if (bucket === 'day') {
    points = bucketByDay(points);
    pharmacies = pharmacies.map((pharmacy) => ({
      ...pharmacy,
      points: bucketPharmacyByDay(pharmacy.points),
    }));
  }

  const response: History = {
    strain_id: id,
    bucket,
    from: from.toISOString(),
    to: to.toISOString(),
    timezone: 'Europe/Berlin',
    points,
  };
  if (withPharmacies) response.pharmacies = pharmacies;
  return json(200, response);
}

function sortReviews(reviews: Review[], sort: ReviewSort): Review[] {
  const byDate = (a: Review, b: Review) =>
    (b.reviewed_on ?? '').localeCompare(a.reviewed_on ?? '') || b.id - a.id;
  const sorted = [...reviews].sort(byDate);
  switch (sort) {
    case 'oldest':
      return sorted.reverse();
    case 'highest':
      return sorted.sort((a, b) => b.rating - a.rating || byDate(a, b));
    case 'lowest':
      return sorted.sort((a, b) => a.rating - b.rating || byDate(a, b));
    default:
      return sorted;
  }
}

function reviewsReply(fixtures: Fixtures, strain: Strain, url: URL): Reply {
  const limit = Math.min(500, Math.max(1, Number(url.searchParams.get('limit') ?? 50) || 50));
  const offset = Math.max(0, Number(url.searchParams.get('offset') ?? 0) || 0);
  const sortParam = url.searchParams.get('sort') ?? 'newest';
  if (!['newest', 'oldest', 'highest', 'lowest'].includes(sortParam)) {
    return error(400, 'bad_request', 'invalid sort');
  }
  const all = fixtures.reviews[String(strain.id)] ?? [];
  const distribution: ReviewsResponse['summary']['distribution'] = {
    '1': 0,
    '2': 0,
    '3': 0,
    '4': 0,
    '5': 0,
  };
  for (const review of all) {
    const key = String(
      Math.min(5, Math.max(1, Math.round(review.rating))),
    ) as keyof typeof distribution;
    distribution[key] += 1;
  }
  const scrapedAt = strain.rating?.scraped_at ?? null;
  const response: ReviewsResponse = {
    strain_id: strain.id,
    summary: {
      value: strain.rating?.value ?? null,
      count: strain.rating?.count ?? 0,
      scraped_at: scrapedAt,
      distribution,
      verified_count: all.filter((review) => review.verified).length,
      stored_count: all.length,
    },
    history:
      strain.rating && scrapedAt
        ? [{ at: scrapedAt, value: strain.rating.value, count: strain.rating.count }]
        : [],
    reviews: sortReviews(all, sortParam as ReviewSort).slice(offset, offset + limit),
    total: all.length,
  };
  return json(200, response);
}

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

export function mockApiPlugin(options: MockApiOptions): Plugin {
  let fixtures: Promise<Fixtures> | null = null;
  const delayMs = options.delayMs ?? 0;

  return {
    name: 'greenmedical-mock-api',
    apply: 'serve',
    configureServer(server) {
      server.config.logger.info(
        `[mock-api] serving /api/v1/* from ${options.fixturesDir} (set API_PROXY_TARGET to use a real backend)`,
      );
      server.middlewares.use((req: IncomingMessage, res: ServerResponse, next: () => void) => {
        const url = new URL(req.url ?? '/', 'http://localhost');
        if (
          !url.pathname.startsWith('/api/') &&
          url.pathname !== '/healthz' &&
          url.pathname !== '/readyz'
        ) {
          next();
          return;
        }
        fixtures ??= loadFixtures(options.fixturesDir);
        void fixtures
          .then(async (data) => {
            if (delayMs > 0) await new Promise((resolve) => setTimeout(resolve, delayMs));
            const reply = handle(
              data,
              req.method ?? 'GET',
              url,
              req.headers['if-none-match'] ?? null,
            );
            res.statusCode = reply.status;
            for (const [key, value] of Object.entries(reply.headers ?? {})) {
              res.setHeader(key, value);
            }
            res.end(req.method === 'HEAD' ? undefined : reply.body);
          })
          .catch((cause: unknown) => {
            fixtures = null;
            const message = cause instanceof Error ? cause.message : String(cause);
            const reply = error(503, 'no_data', `mock fixtures unavailable: ${message}`);
            res.statusCode = reply.status;
            for (const [key, value] of Object.entries(reply.headers ?? {})) {
              res.setHeader(key, value);
            }
            res.end(reply.body);
          });
      });
    },
  };
}
