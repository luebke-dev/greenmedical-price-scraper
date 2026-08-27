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
  Facets,
  Metadata,
  OfferHistoryPage,
  OfferHistoryRow,
  OfferPhaseRow,
  Pharmacy,
  Rating,
  Review,
  ReviewSort,
  ReviewsResponse,
  PharmacySeries,
  PharmacySeriesPoint,
  Rule,
  RuleInput,
  RuleKind,
  Run,
  Strain,
  StrainDetail,
  StrainListItem,
  StrainsPage,
  Subscription,
  SubscriptionCreate,
} from '../src/api/types';

/** Shape of dev/fixtures/strains.json (the former full list incl. offers/search). */
interface StrainsFixture {
  run: Run;
  reference_run: Run | null;
  strains: Strain[];
}

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
  strains: StrainsFixture;
  history: Record<string, HistoryFixture>;
  runs: { runs: Run[]; total: number };
  reviews: Record<string, Review[]>;
}

/** Next full hour (UTC) as RFC 3339 – the mock's stand-in for the hourly schedule. */
function nextFullHour(now: Date = new Date()): string {
  const next = new Date(now);
  next.setUTCMinutes(0, 0, 0);
  next.setUTCHours(next.getUTCHours() + 1);
  return next.toISOString();
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
      next_run_at: nextFullHour(),
      scrape_running: false,
      schedule: { cron: '0 0 * * * *', timezone: 'Europe/Berlin' },
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
  strains: StrainsFixture,
  ratings: Record<string, RatingFixture>,
): { metadata: Metadata; strains: StrainsFixture; reviews: Record<string, Review[]> } {
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
    readJson<StrainsFixture>(dir, 'strains.json'),
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
        provider: offer.provider,
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

function handle(
  fixtures: Fixtures,
  method: string,
  url: URL,
  ifNoneMatch: string | null,
  body: string,
): Reply {
  const path = url.pathname.replace(/\/+$/, '') || '/';

  if (path === '/healthz') return json(200, { status: 'ok' });
  if (path === '/readyz') return json(200, { status: 'ready', db: 'ok' });
  if (!path.startsWith('/api/v1/')) return error(404, 'not_found', `unknown path ${path}`);

  const rest = path.slice('/api/v1'.length);

  if (rest === '/admin/scrape') {
    // ADMIN_TOKEN is empty in the mock → endpoint does not exist.
    return error(404, 'not_found', 'admin endpoint disabled');
  }
  if (rest === '/_mock/subscriptions') return json(200, subscriptions.listing());
  if (rest.startsWith('/subscriptions'))
    return subscriptions.handle(fixtures, method, rest, url, body);
  if (method !== 'GET' && method !== 'HEAD') {
    return error(405, 'bad_request', 'method not allowed');
  }

  if (rest === '/metadata') {
    // `?__running=1` simulates a scrape in progress (banner spinner + polling).
    const running = url.searchParams.get('__running') === '1';
    return json(200, {
      ...fixtures.metadata,
      next_run_at: nextFullHour(),
      scrape_running: running,
    });
  }

  if (rest === '/strains') return strainsPage(fixtures, url, ifNoneMatch);

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
  const offerHistoryMatch = /^\/strains\/(\d+)\/offer-history$/.exec(rest);
  if (offerHistoryMatch) {
    const id = Number(offerHistoryMatch[1]);
    if (!fixtures.strains.strains.some((item) => item.id === id)) {
      return error(404, 'not_found', `strain ${id} not found`);
    }
    return offerHistory(fixtures, id, url);
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

// ---------------------------------------------------------------------------
// GET /strains: server-side filter/sort/pagination + facets (mirrors the backend contract)
// ---------------------------------------------------------------------------

const deCollator = new Intl.Collator('de', { numeric: true, sensitivity: 'base' });

const STRAIN_SORT_KEYS = new Set([
  'price',
  'price_per_thc_gram',
  'thc',
  'cbd',
  'pharmacy_count',
  'rating',
  'name',
  'bezeichnung',
  'genetik',
]);

function numberParam(url: URL, name: string): number | null {
  const raw = url.searchParams.get(name);
  if (raw === null || raw.trim() === '') return null;
  const value = Number(raw);
  return Number.isFinite(value) ? value : null;
}

/** Inclusive bounds; strains without a value only pass while BOTH bounds are absent. */
function inBounds(value: number | null, min: number | null, max: number | null): boolean {
  if (min === null && max === null) return true;
  if (value === null) return false;
  return (min === null || value >= min) && (max === null || value <= max);
}

function strainSortValue(strain: Strain, key: string): number | string | null {
  switch (key) {
    case 'price':
    case 'price_per_thc_gram':
    case 'thc':
    case 'cbd':
    case 'rating':
      return strain.sort[key];
    case 'pharmacy_count':
      return strain.pharmacy_count;
    case 'name':
    case 'bezeichnung':
    case 'genetik':
      return strain[key] || '';
    default:
      return null;
  }
}

function compareStrains(a: Strain, b: Strain, key: string, dir: 1 | -1): number {
  const left = strainSortValue(a, key);
  const right = strainSortValue(b, key);
  let result: number;
  if (typeof left === 'string' || typeof right === 'string') {
    result = deCollator.compare(String(left ?? ''), String(right ?? '')) * dir;
  } else if (left === null && right === null) {
    result = 0;
  } else if (left === null) {
    result = 1; // nulls always last
  } else if (right === null) {
    result = -1;
  } else {
    result = (left - right) * dir;
  }
  return result || a.id - b.id;
}

function facetRange(values: (number | null)[]): { min: number; max: number } | null {
  let min = Number.POSITIVE_INFINITY;
  let max = Number.NEGATIVE_INFINITY;
  for (const value of values) {
    if (value === null || !Number.isFinite(value)) continue;
    if (value < min) min = value;
    if (value > max) max = value;
  }
  return min === Number.POSITIVE_INFINITY ? null : { min, max };
}

function buildFacets(strains: Strain[]): Facets {
  const genetik = new Map<string, { value: string; count: number }>();
  for (const strain of strains) {
    const value = strain.genetik?.trim() ?? '';
    if (!value) continue;
    const key = value.toLowerCase();
    const entry = genetik.get(key);
    if (entry) entry.count += 1;
    else genetik.set(key, { value, count: 1 });
  }
  return {
    genetik: [...genetik.values()].sort((a, b) => deCollator.compare(a.value, b.value)),
    price: facetRange(strains.map((strain) => strain.sort.price)),
    thc: facetRange(strains.map((strain) => strain.sort.thc)),
    cbd: facetRange(strains.map((strain) => strain.sort.cbd)),
    rating: facetRange(strains.map((strain) => strain.sort.rating)),
  };
}

function toListItem(strain: Strain): StrainListItem {
  const { offers: _offers, search: _search, ...item } = strain;
  void _offers;
  void _search;
  return item;
}

function fnv1a(text: string): string {
  let hash = 0x811c9dc5;
  for (let index = 0; index < text.length; index += 1) {
    hash ^= text.charCodeAt(index);
    hash = Math.imul(hash, 0x01000193) >>> 0;
  }
  return hash.toString(16).padStart(8, '0');
}

function strainsPage(fixtures: Fixtures, url: URL, ifNoneMatch: string | null): Reply {
  const sort = url.searchParams.get('sort') ?? 'price';
  const dir = url.searchParams.get('dir') ?? 'asc';
  if (!STRAIN_SORT_KEYS.has(sort)) return error(400, 'bad_request', `invalid sort ${sort}`);
  if (dir !== 'asc' && dir !== 'desc') return error(400, 'bad_request', `invalid dir ${dir}`);
  const limitRaw = url.searchParams.get('limit');
  const limit = limitRaw === null ? 50 : Number(limitRaw);
  if (!Number.isInteger(limit) || limit < 1 || limit > 500) {
    return error(400, 'bad_request', 'limit must be 1–500');
  }
  const offsetRaw = url.searchParams.get('offset');
  const offset = offsetRaw === null ? 0 : Number(offsetRaw);
  if (!Number.isInteger(offset) || offset < 0) return error(400, 'bad_request', 'offset >= 0');

  const q = (url.searchParams.get('q') ?? '').trim().toLowerCase();
  const genetik = new Set(
    (url.searchParams.get('genetik') ?? '')
      .split(',')
      .map((item) => item.trim().toLowerCase())
      .filter((item) => item !== ''),
  );
  const bounds = {
    price: [numberParam(url, 'price_min'), numberParam(url, 'price_max')] as const,
    thc: [numberParam(url, 'thc_min'), numberParam(url, 'thc_max')] as const,
    cbd: [numberParam(url, 'cbd_min'), numberParam(url, 'cbd_max')] as const,
  };
  const ratingMin = numberParam(url, 'rating_min');

  const all = fixtures.strains.strains;
  const hits = all
    .filter((strain) => {
      if (q && !strain.search.includes(q)) return false;
      if (genetik.size > 0 && !genetik.has((strain.genetik ?? '').toLowerCase())) return false;
      if (!inBounds(strain.sort.price, ...bounds.price)) return false;
      if (!inBounds(strain.sort.thc, ...bounds.thc)) return false;
      if (!inBounds(strain.sort.cbd, ...bounds.cbd)) return false;
      if (ratingMin !== null && (strain.sort.rating === null || strain.sort.rating < ratingMin)) {
        return false;
      }
      return true;
    })
    .sort((a, b) => compareStrains(a, b, sort, dir === 'asc' ? 1 : -1));

  const normalized = [...url.searchParams.entries()]
    .filter(([key]) => key !== '_')
    .sort(([a], [b]) => a.localeCompare(b))
    .map(([key, value]) => `${key}=${value}`)
    .join('&');
  const run = fixtures.strains.run;
  const reference = fixtures.strains.reference_run;
  const etag = `"run-${run.id}${reference ? `-r${reference.id}` : ''}-${fnv1a(normalized)}"`;
  const headers = { ETag: etag, 'Cache-Control': 'public, max-age=300' };
  if (ifNoneMatch === etag) return { status: 304, body: '', headers };

  const page: StrainsPage = {
    run,
    reference_run: reference,
    total: hits.length,
    limit,
    offset,
    facets: buildFacets(all),
    strains: hits.slice(offset, offset + limit).map(toListItem),
  };
  return json(200, page, headers);
}

// ---------------------------------------------------------------------------
// GET /strains/{id}/offer-history
// ---------------------------------------------------------------------------

interface ResolvedRange {
  from: Date;
  to: Date;
  bucket: 'run' | 'day';
}

function parseRange(url: URL): ResolvedRange | Reply {
  const fromParam = url.searchParams.get('from');
  const toParam = url.searchParams.get('to');
  const bucket = url.searchParams.get('bucket') === 'day' ? 'day' : 'run';
  const to = toParam ? new Date(toParam) : new Date();
  const from = fromParam ? new Date(fromParam) : new Date(to.valueOf() - 90 * DAY_MS);
  if (Number.isNaN(from.valueOf()) || Number.isNaN(to.valueOf())) {
    return error(400, 'bad_request', 'invalid from/to');
  }
  if (to.valueOf() - from.valueOf() > 730 * DAY_MS) {
    return error(400, 'bad_request', 'span exceeds 730 days');
  }
  return { from, to, bucket };
}

function isReply(value: ResolvedRange | Reply): value is Reply {
  return 'status' in value;
}

/** Points and per-pharmacy series of one strain inside the range, bucketed like /history. */
function resolvedSeries(
  fixtures: Fixtures,
  id: number,
  range: ResolvedRange,
  includePartial: boolean,
) {
  const fixture = fixtures.history[String(id)] ?? { points: [], pharmacies: [] };
  const inRange = (at: string) => {
    const t = new Date(at).valueOf();
    return t >= range.from.valueOf() && t <= range.to.valueOf();
  };
  let points = fixture.points.filter(
    (point) => inRange(point.at) && (includePartial || point.status !== 'partial'),
  );
  let pharmacies = fixture.pharmacies.map((pharmacy) => ({
    ...pharmacy,
    points: pharmacy.points.filter((point) => inRange(point.at)),
  }));
  if (range.bucket === 'day') {
    points = bucketByDay(points);
    pharmacies = pharmacies.map((pharmacy) => ({
      ...pharmacy,
      points: bucketPharmacyByDay(pharmacy.points),
    }));
  }
  return { points, pharmacies };
}

const phaseCollator = new Intl.Collator('de', { sensitivity: 'base' });

/** mode=all: one row per (bucket, pharmacy) with an offer; `at` desc, pharmacy asc. */
function offerRows(pharmacies: PharmacySeries[]): OfferHistoryRow[] {
  const rows: OfferHistoryRow[] = [];
  for (const series of pharmacies) {
    for (const point of series.points) {
      if (point.price === null && !point.availability) continue;
      const row: OfferHistoryRow = {
        at: point.at,
        pharmacy_id: series.pharmacy_id,
        pharmacy: series.name,
        city: series.city,
        price: point.price,
        price_per_thc_gram: point.price_per_thc_gram,
        availability: point.availability,
      };
      if (point.run_id !== undefined) row.run_id = point.run_id;
      rows.push(row);
    }
  }
  return rows.sort((a, b) =>
    a.at < b.at ? 1 : a.at > b.at ? -1 : phaseCollator.compare(a.pharmacy, b.pharmacy),
  );
}

/**
 * mode=changes: one row per pharmacy and consecutive stretch of runs with the same price+status.
 * A pharmacy missing from a run (while the strain itself was seen) starts a delisted phase;
 * runs before the pharmacy first listed the strain are ignored. `from` desc, pharmacy asc.
 */
function offerPhases(points: HistoryPoint[], pharmacies: PharmacySeries[]): OfferPhaseRow[] {
  const runs = [...new Set(points.map((point) => point.at))].sort();
  if (runs.length === 0) return [];
  const latest = runs[runs.length - 1]!;
  const rows: OfferPhaseRow[] = [];

  for (const series of pharmacies) {
    const byAt = new Map(series.points.map((point) => [point.at, point]));
    let current: (OfferPhaseRow & { stateKey: string }) | null = null;
    let seen = false;
    for (const at of runs) {
      const point = byAt.get(at);
      const listed = point !== undefined && (point.price !== null || point.availability !== '');
      if (!listed && !seen) continue;
      seen = true;
      const stateKey = listed ? `${point.price ?? ''}|${point.availability}` : 'delisted';
      if (current && current.stateKey === stateKey) {
        current.to = at === latest ? null : at;
        current.runs += 1;
        continue;
      }
      if (current) rows.push(stripStateKey(current));
      current = {
        stateKey,
        pharmacy_id: series.pharmacy_id,
        pharmacy: series.name,
        city: series.city,
        price: listed ? point.price : null,
        price_per_thc_gram: listed ? point.price_per_thc_gram : null,
        availability: listed ? point.availability : '',
        from: at,
        to: at === latest ? null : at,
        runs: 1,
        delisted: !listed,
      };
    }
    if (current) rows.push(stripStateKey(current));
  }

  return rows.sort((a, b) =>
    a.from < b.from ? 1 : a.from > b.from ? -1 : phaseCollator.compare(a.pharmacy, b.pharmacy),
  );
}

function stripStateKey(row: OfferPhaseRow & { stateKey: string }): OfferPhaseRow {
  const { stateKey: _omit, ...rest } = row;
  void _omit;
  return rest;
}

function offerHistory(fixtures: Fixtures, id: number, url: URL): Reply {
  const range = parseRange(url);
  if (isReply(range)) return range;
  const modeParam = url.searchParams.get('mode') ?? 'changes';
  if (modeParam !== 'changes' && modeParam !== 'all') {
    return error(400, 'bad_request', 'invalid mode');
  }
  const limit = Number(url.searchParams.get('limit') ?? 50);
  const offset = Number(url.searchParams.get('offset') ?? 0);
  if (!Number.isInteger(limit) || limit < 1 || limit > 500) {
    return error(400, 'bad_request', 'limit must be 1–500');
  }
  if (!Number.isInteger(offset) || offset < 0) return error(400, 'bad_request', 'offset >= 0');
  const pharmacyId = numberParam(url, 'pharmacy_id');

  const { points, pharmacies } = resolvedSeries(fixtures, id, range, true);
  const selected =
    pharmacyId === null ? pharmacies : pharmacies.filter((p) => p.pharmacy_id === pharmacyId);
  const rows = modeParam === 'all' ? offerRows(selected) : offerPhases(points, selected);

  const response: OfferHistoryPage = {
    strain_id: id,
    bucket: range.bucket,
    mode: modeParam,
    from: range.from.toISOString(),
    to: range.to.toISOString(),
    total: rows.length,
    limit,
    offset,
    rows: rows.slice(offset, offset + limit),
  };
  return json(200, response);
}

function history(fixtures: Fixtures, id: number, url: URL): Reply {
  const range = parseRange(url);
  if (isReply(range)) return range;
  const includePartial = url.searchParams.get('include_partial') !== 'false';
  const withPharmacies = url.searchParams.get('pharmacies') === 'true';
  const { points, pharmacies } = resolvedSeries(fixtures, id, range, includePartial);

  const response: History = {
    strain_id: id,
    bucket: range.bucket,
    from: range.from.toISOString(),
    to: range.to.toISOString(),
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
// Preisalarm-Abos (in-memory; tokens are logged and listed under /_mock/subscriptions)
// ---------------------------------------------------------------------------

const RULE_KINDS: readonly RuleKind[] = [
  'strain_available',
  'strain_price_below',
  'any_price_below',
  'thc_above',
  'new_strain',
  'strain_price_change',
];
const RULE_NEEDS: Record<RuleKind, { strain: boolean; threshold: boolean }> = {
  strain_available: { strain: true, threshold: false },
  strain_price_below: { strain: true, threshold: true },
  any_price_below: { strain: false, threshold: true },
  thc_above: { strain: false, threshold: true },
  new_strain: { strain: false, threshold: false },
  strain_price_change: { strain: true, threshold: false },
};
const EMAIL_RE = /^[^\s@]+@[^\s@]+\.[^\s@]{2,}$/;
/** Mirrors SUBSCRIPTION_RATE_LIMIT=5/1h (single "IP" in the mock). */
const RATE_LIMIT = 5;
const RATE_WINDOW_MS = 60 * 60_000;

interface Subscriber {
  id: number;
  email: string;
  confirmed_at: string | null;
  confirm_token: string;
  manage_token: string;
  created_at: string;
  rules: Rule[];
}

function readBody(req: IncomingMessage): Promise<string> {
  return new Promise((resolve, reject) => {
    const chunks: Buffer[] = [];
    req.on('data', (chunk: Buffer) => chunks.push(chunk));
    req.on('end', () => resolve(Buffer.concat(chunks).toString('utf8')));
    req.on('error', reject);
  });
}

function token(): string {
  // 32 random bytes, base64url like the backend.
  const bytes = new Uint8Array(32);
  for (let i = 0; i < bytes.length; i += 1) bytes[i] = Math.floor(Math.random() * 256);
  return Buffer.from(bytes).toString('base64url');
}

class SubscriptionStore {
  private subscribers: Subscriber[] = [];
  private seq = 1;
  private ruleSeq = 1;
  private createdAt: number[] = [];

  listing(): unknown {
    return this.subscribers.map((s) => ({
      email: s.email,
      confirmed: s.confirmed_at !== null,
      confirm_url: `/abo/bestaetigen?token=${s.confirm_token}`,
      manage_url: `/abo/verwalten?token=${s.manage_token}`,
      rules: s.rules,
    }));
  }

  handle(fixtures: Fixtures, method: string, rest: string, url: URL, body: string): Reply {
    const noStore = { 'Cache-Control': 'no-store' };
    const withNoStore = (reply: Reply): Reply => ({
      ...reply,
      headers: { ...reply.headers, ...noStore },
    });
    if (rest === '/subscriptions' && method === 'POST') {
      return withNoStore(this.create(fixtures, parseJson(body)));
    }
    if (rest === '/subscriptions/confirm' && method === 'POST') {
      const parsed = parseJson(body) as { token?: unknown } | null;
      const value = typeof parsed?.token === 'string' ? parsed.token : '';
      const subscriber = this.subscribers.find((s) => s.confirm_token === value);
      if (!subscriber) return withNoStore(error(404, 'not_found', 'unknown token'));
      subscriber.confirmed_at ??= new Date().toISOString();
      return withNoStore(json(200, toSubscription(subscriber)));
    }
    if (rest === '/subscriptions/manage') {
      const value = url.searchParams.get('token') ?? '';
      const subscriber = this.subscribers.find((s) => s.manage_token === value);
      if (!subscriber) return withNoStore(error(404, 'not_found', 'unknown token'));
      if (method === 'GET' || method === 'HEAD') {
        return withNoStore(json(200, toSubscription(subscriber)));
      }
      if (method === 'PUT') {
        const parsed = parseJson(body) as { rules?: unknown } | null;
        const rules = validateRules(fixtures, parsed?.rules);
        if (typeof rules === 'string') return withNoStore(error(400, 'bad_request', rules));
        subscriber.rules = rules.map((rule) => this.toRule(fixtures, rule));
        return withNoStore(json(200, toSubscription(subscriber)));
      }
      if (method === 'DELETE') {
        this.subscribers = this.subscribers.filter((s) => s !== subscriber);
        return { status: 204, body: '', headers: noStore };
      }
    }
    return error(405, 'bad_request', 'method not allowed');
  }

  private create(fixtures: Fixtures, parsed: unknown): Reply {
    const payload = (parsed ?? {}) as Partial<SubscriptionCreate>;
    if (typeof payload.website === 'string' && payload.website.trim() !== '') {
      return json(202, { status: 'confirmation_sent' });
    }
    const email = typeof payload.email === 'string' ? payload.email.trim() : '';
    if (!EMAIL_RE.test(email)) return error(400, 'bad_request', 'ungültige E-Mail-Adresse');
    const rules = validateRules(fixtures, payload.rules);
    if (typeof rules === 'string') return error(400, 'bad_request', rules);

    const now = Date.now();
    this.createdAt = this.createdAt.filter((at) => now - at < RATE_WINDOW_MS);
    if (this.createdAt.length >= RATE_LIMIT) {
      return error(429, 'rate_limited', 'zu viele Anfragen, bitte später erneut versuchen');
    }
    this.createdAt.push(now);

    let subscriber = this.subscribers.find((s) => s.email.toLowerCase() === email.toLowerCase());
    if (!subscriber) {
      subscriber = {
        id: this.seq++,
        email,
        confirmed_at: null,
        confirm_token: token(),
        manage_token: token(),
        created_at: new Date().toISOString(),
        rules: [],
      };
      this.subscribers.push(subscriber);
    }
    for (const rule of rules) {
      const exists = subscriber.rules.some(
        (r) =>
          r.kind === rule.kind && r.strain_id === rule.strain_id && r.threshold === rule.threshold,
      );
      if (!exists) subscriber.rules.push(this.toRule(fixtures, rule));
    }
    if (subscriber.confirmed_at === null) {
      console.info(
        `[mock] confirm token for ${email}: ${subscriber.confirm_token} → /abo/bestaetigen?token=${subscriber.confirm_token}`,
      );
    }
    console.info(
      `[mock] manage token for ${email}: ${subscriber.manage_token} → /abo/verwalten?token=${subscriber.manage_token}`,
    );
    return json(202, { status: 'confirmation_sent' });
  }

  private toRule(fixtures: Fixtures, input: RuleInput): Rule {
    const rule: Rule = {
      id: this.ruleSeq++,
      kind: input.kind,
      created_at: new Date().toISOString(),
    };
    if (input.strain_id !== undefined) {
      rule.strain_id = input.strain_id;
      rule.strain_name =
        fixtures.strains.strains.find((s) => s.id === input.strain_id)?.name ?? null;
    }
    if (input.threshold !== undefined) rule.threshold = input.threshold;
    return rule;
  }
}

function parseJson(body: string): unknown {
  try {
    return body ? (JSON.parse(body) as unknown) : null;
  } catch {
    return null;
  }
}

/** Backend-like validation: 1–20 rules, fields per kind. Returns the error message or the rules. */
function validateRules(fixtures: Fixtures, raw: unknown): RuleInput[] | string {
  if (!Array.isArray(raw) || raw.length === 0) return 'mindestens eine Regel erforderlich';
  if (raw.length > 20) return 'höchstens 20 Regeln';
  const rules: RuleInput[] = [];
  for (const [index, item] of raw.entries()) {
    const entry = (item ?? {}) as Partial<RuleInput>;
    const kind = entry.kind;
    if (typeof kind !== 'string' || !RULE_KINDS.includes(kind)) {
      return `Regel ${index + 1}: unbekannte Art`;
    }
    const needs = RULE_NEEDS[kind];
    const rule: RuleInput = { kind: kind };
    if (needs.strain) {
      if (typeof entry.strain_id !== 'number' || !Number.isInteger(entry.strain_id)) {
        return `Regel ${index + 1}: strain_id fehlt`;
      }
      if (!fixtures.strains.strains.some((s) => s.id === entry.strain_id)) {
        return `Regel ${index + 1}: Sorte ${entry.strain_id} unbekannt`;
      }
      rule.strain_id = entry.strain_id;
    } else if (entry.strain_id !== undefined) {
      return `Regel ${index + 1}: strain_id nicht erlaubt`;
    }
    if (needs.threshold) {
      if (typeof entry.threshold !== 'number' || !(entry.threshold > 0)) {
        return `Regel ${index + 1}: threshold muss größer 0 sein`;
      }
      rule.threshold = Math.round(entry.threshold * 100) / 100;
    } else if (entry.threshold !== undefined) {
      return `Regel ${index + 1}: threshold nicht erlaubt`;
    }
    rules.push(rule);
  }
  return rules;
}

function toSubscription(subscriber: Subscriber): Subscription {
  return {
    email: subscriber.email,
    confirmed: subscriber.confirmed_at !== null,
    rules: subscriber.rules,
    created_at: subscriber.created_at,
  };
}

const subscriptions = new SubscriptionStore();

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
        void Promise.all([fixtures, readBody(req)])
          .then(async ([data, body]) => {
            if (delayMs > 0) await new Promise((resolve) => setTimeout(resolve, delayMs));
            const reply = handle(
              data,
              req.method ?? 'GET',
              url,
              req.headers['if-none-match'] ?? null,
              body,
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
