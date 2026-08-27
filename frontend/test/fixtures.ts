// Shared test data builders.
import type {
  Facets,
  History,
  HistoryPoint,
  Metadata,
  Offer,
  OfferHistoryPage,
  OfferHistoryRow,
  OfferPhaseRow,
  PharmacySeries,
  Rating,
  Review,
  Rule,
  ReviewsResponse,
  Run,
  Strain,
  StrainListItem,
  StrainsPage,
  Subscription,
  Trend,
} from '@/api/types';

export const LATEST_AT = '2026-08-27T20:00:00Z';

export function makeRun(overrides: Partial<Run> = {}): Run {
  return {
    id: 40,
    started_at: '2026-08-27T19:56:20Z',
    finished_at: LATEST_AT,
    status: 'success',
    trigger: 'schedule',
    instance: 'backend-0',
    pharmacies_total: 18,
    pharmacies_scraped: 18,
    pharmacies_failed: 0,
    offer_count: 2000,
    http_requests: 220,
    error: null,
    ...overrides,
  };
}

let offerSeq = 1;

export function makeOffer(overrides: Partial<Offer> = {}): Offer {
  const id = offerSeq++;
  return {
    offer_id: id,
    pharmacy_id: 1,
    apotheke: 'Grüne Blüte',
    apotheke_plz: '04416',
    apotheke_stadt: 'Markkleeberg',
    preis_pro_gramm: '5,49 €/g',
    preis_eur_pro_gramm: 5.49,
    preis_eur_pro_gramm_thc: 20.33,
    verfuegbarkeit: 'Auf Lager',
    produkt_url: `https://greenmedical.health/de/cannabis/flower/${id}`,
    ...overrides,
  };
}

export function makeTrend(overrides: Partial<Trend> = {}): Trend {
  return {
    reference_run_id: 12,
    reference_at: '2026-08-20T19:56:20Z',
    min_price_then: 6.49,
    delta: -0.5,
    delta_pct: -7.7,
    direction: 'down',
    ...overrides,
  };
}

export interface StrainInput extends Partial<Omit<Strain, 'sort' | 'offers'>> {
  price?: number | null;
  /** Shorthand for `rating: { value, count, scraped_at }` (+ `sort.rating`). */
  ratingValue?: number | null;
  reviewCount?: number;
  thcPrice?: number | null;
  thcValue?: number | null;
  cbdValue?: number | null;
  offers?: Offer[];
}

export function makeStrain(input: StrainInput = {}): Strain {
  const {
    price = 5.49,
    thcPrice = 20.33,
    thcValue = 27,
    cbdValue = 1,
    offers = [makeOffer({ preis_eur_pro_gramm: price, preis_eur_pro_gramm_thc: thcPrice })],
    ratingValue,
    reviewCount,
    ...rest
  } = input;
  const rating: Rating | null =
    ratingValue === undefined && reviewCount === undefined
      ? null
      : { value: ratingValue ?? null, count: reviewCount ?? 0, scraped_at: LATEST_AT };
  const id = rest.id ?? 1;
  const name = rest.name ?? `Sorte ${id}`;
  const bezeichnung = rest.bezeichnung ?? `Bezeichnung ${id}`;
  const genetik = rest.genetik ?? 'Indica';
  const thc = rest.thc ?? '27%';
  const cbd = rest.cbd ?? '1%';
  return {
    id,
    name,
    bezeichnung,
    genetik,
    thc,
    cbd,
    thc_value: thcValue,
    cbd_value: cbdValue,
    min_price: price,
    min_price_per_thc_gram: thcPrice,
    pharmacy_count: new Set(offers.map((offer) => offer.apotheke)).size,
    offers,
    sort: {
      price,
      price_per_thc_gram: thcPrice,
      thc: thcValue,
      cbd: cbdValue,
      rating: rating?.value ?? null,
    },
    search: [
      name,
      bezeichnung,
      genetik,
      thc,
      cbd,
      offers.map((o) => `${o.apotheke} ${o.apotheke_stadt}`).join(' '),
    ]
      .join(' ')
      .toLowerCase(),
    trend: null,
    rating,
    product_uuid: null,
    ...rest,
  };
}

/** List item as returned by GET /strains (no offers, no search). */
export function makeListItem(input: StrainInput = {}): StrainListItem {
  const { offers: _offers, search: _search, ...item } = makeStrain(input);
  void _offers;
  void _search;
  return item;
}

export function makeFacets(overrides: Partial<Facets> = {}): Facets {
  return {
    genetik: [
      { value: 'Hybrid', count: 3 },
      { value: 'Indica', count: 5 },
      { value: 'Sativa', count: 2 },
    ],
    price: { min: 5.49, max: 12.35 },
    thc: { min: 18.2, max: 31 },
    cbd: { min: 0.3, max: 12 },
    rating: { min: 3.1, max: 4.9 },
    ...overrides,
  };
}

export function makeStrainsPage(
  strains: StrainListItem[],
  overrides: Partial<StrainsPage> = {},
): StrainsPage {
  return {
    run: makeRun(),
    reference_run: null,
    total: strains.length,
    limit: 50,
    offset: 0,
    facets: makeFacets(),
    strains,
    ...overrides,
  };
}

export function makePhaseRow(overrides: Partial<OfferPhaseRow> = {}): OfferPhaseRow {
  return {
    pharmacy_id: 1,
    pharmacy: 'Grüne Blüte',
    city: 'Markkleeberg',
    price: 6.49,
    price_per_thc_gram: 27.04,
    availability: 'Auf Lager',
    from: '2026-08-26T08:00:00Z',
    to: null,
    runs: 3,
    delisted: false,
    ...overrides,
  };
}

export function makeOfferRow(overrides: Partial<OfferHistoryRow> = {}): OfferHistoryRow {
  return {
    at: '2026-08-27T08:00:00Z',
    run_id: 40,
    pharmacy_id: 1,
    pharmacy: 'Grüne Blüte',
    city: 'Markkleeberg',
    price: 6.49,
    price_per_thc_gram: 27.04,
    availability: 'Auf Lager',
    ...overrides,
  };
}

export function makeOfferHistoryPage(
  rows: OfferHistoryRow[] | OfferPhaseRow[],
  overrides: Partial<OfferHistoryPage> = {},
): OfferHistoryPage {
  return {
    strain_id: 7,
    bucket: 'run',
    mode: 'changes',
    from: '2026-07-28T20:00:00Z',
    to: LATEST_AT,
    total: rows.length,
    limit: 50,
    offset: 0,
    rows,
    ...overrides,
  };
}

export function makeMetadata(overrides: Partial<Metadata> = {}): Metadata {
  const highlight = {
    price: 5.49,
    name: 'Bunatic',
    apotheke: 'Grüne Blüte',
    genetik: 'Indica',
    thc: '27%',
    cbd: '1%',
    produkt_url: 'https://greenmedical.health/de/cannabis/flower/bunatic',
    strain_id: 7,
    pharmacy_id: 1,
  };
  return {
    generated_at: LATEST_AT,
    source: 'https://greenmedical.health/de/cannabis/flowers',
    total: 2021,
    pharmacy_count: 18,
    strain_count: 912,
    lowest_price: 5.49,
    cheapest_gram: highlight,
    cheapest_thc_gram: { ...highlight, price: 20.33 },
    cheapest_cbd_gram: { ...highlight, price: 549, name: 'CBD Sorte', strain_id: 8 },
    highest_thc: { ...highlight, thc: '31%' },
    highest_cbd: { ...highlight, cbd: '12%' },
    highest_thc_cbd: { ...highlight, thc: '20%', cbd: '10%' },
    best_rated: { ...highlight, name: 'Bunatic', rating_value: 4.7, review_count: 13 },
    run: makeRun(),
    next_run_at: '2026-08-27T21:00:00Z',
    scrape_running: false,
    schedule: { cron: '0 0 * * * *', timezone: 'Europe/Berlin' },
    ...overrides,
  };
}

export function makePoint(overrides: Partial<HistoryPoint> = {}): HistoryPoint {
  return {
    run_id: 1,
    at: '2026-08-20T20:00:00Z',
    status: 'success',
    min: 5.49,
    avg: 6.0,
    max: 6.99,
    min_per_thc_gram: 20.33,
    avg_per_thc_gram: 22.0,
    max_per_thc_gram: 25.9,
    offer_count: 3,
    pharmacy_count: 3,
    ...overrides,
  };
}

export function makeHistory(
  points: HistoryPoint[],
  pharmacies?: PharmacySeries[],
  overrides: Partial<History> = {},
): History {
  const history: History = {
    strain_id: 1,
    bucket: 'run',
    from: '2026-07-28T20:00:00Z',
    to: LATEST_AT,
    timezone: 'Europe/Berlin',
    points,
    ...overrides,
  };
  if (pharmacies) history.pharmacies = pharmacies;
  return history;
}

let reviewSeq = 1;

export function makeReview(overrides: Partial<Review> = {}): Review {
  const id = reviewSeq++;
  return {
    id,
    author: `Autor ${id}`,
    reviewed_on: '2026-08-25',
    rating: 4,
    verified: true,
    content: `Bewertung ${id}`,
    first_seen_at: LATEST_AT,
    ...overrides,
  };
}

export function makeReviewsResponse(
  reviews: Review[],
  overrides: Partial<ReviewsResponse> = {},
  summary: Partial<ReviewsResponse['summary']> = {},
): ReviewsResponse {
  return {
    strain_id: 7,
    summary: {
      value: 4.3,
      count: reviews.length,
      scraped_at: LATEST_AT,
      distribution: { '1': 0, '2': 0, '3': 0, '4': 0, '5': 0 },
      verified_count: reviews.filter((review) => review.verified).length,
      stored_count: reviews.length,
      ...summary,
    },
    history: [],
    reviews,
    total: reviews.length,
    ...overrides,
  };
}

export function makeRule(overrides: Partial<Rule> = {}): Rule {
  return {
    id: 1,
    kind: 'strain_price_below',
    strain_id: 7,
    strain_name: 'OG Kush',
    threshold: 6,
    created_at: LATEST_AT,
    ...overrides,
  };
}

export function makeSubscription(overrides: Partial<Subscription> = {}): Subscription {
  return {
    email: 'test@example.de',
    confirmed: true,
    rules: [makeRule()],
    created_at: LATEST_AT,
    ...overrides,
  };
}
