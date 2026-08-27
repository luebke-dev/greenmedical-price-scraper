// JSON shapes of the backend API. Kept verbatim in sync with docs/api-contract.md.

export type RunStatus = 'running' | 'success' | 'partial' | 'failed';
export type RunTrigger = 'schedule' | 'manual' | 'bootstrap';
export type Provider = 'greenmedical' | 'ansay';

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
  reviews_scraped?: number | null;
  reviews_failed?: number | null;
}

export interface RunError {
  pharmacy_name: string;
  pharmacy_url: string;
  stage: 'uuid' | 'pages';
  message: string;
}

export interface Offer {
  offer_id: number;
  pharmacy_id: number;
  provider: Provider;
  apotheke: string;
  apotheke_plz: string;
  apotheke_stadt: string;
  preis_pro_gramm: string; // verbatim, z.B. "5,49 €/g"
  preis_eur_pro_gramm: number | null;
  preis_eur_pro_gramm_thc: number | null;
  verfuegbarkeit: string; // "Auf Lager" | "NEU" | …
  produkt_url: string;
}

export interface Trend {
  reference_run_id: number;
  reference_at: string;
  min_price_then: number;
  delta: number; // min_price_now - min_price_then
  delta_pct: number; // delta / min_price_then * 100
  direction: 'up' | 'down' | 'flat'; // flat wenn |delta| < 0.005
}

export interface Rating {
  value: number | null;
  count: number;
  scraped_at: string;
}

export interface Strain {
  id: number; // stabile DB-ID
  name: string;
  bezeichnung: string;
  genetik: string;
  thc: string; // verbatim "27%", "<1%"
  cbd: string;
  thc_value: number | null; // geparst ("<1%" → 0.99)
  cbd_value: number | null;
  min_price: number | null;
  min_price_per_thc_gram: number | null;
  pharmacy_count: number;
  offers: Offer[]; // günstigste zuerst, null-Preise zuletzt
  sort: {
    price: number | null;
    price_per_thc_gram: number | null;
    thc: number | null;
    cbd: number | null;
    rating: number | null; // = rating.value
  };
  search: string; // lowercased Suchtext wie build_site.py
  trend: Trend | null;
  rating: Rating | null; // null = noch nie gescrapt
  product_uuid: string | null;
}

export interface StrainDetail extends Strain {
  first_seen_at: string;
  last_seen_at: string;
  in_latest_run: boolean;
  run: Run; // latest usable run
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
  rating_value?: number | null;
  review_count?: number;
}

export interface Metadata {
  generated_at: string; // = run.finished_at
  source: string; // "https://greenmedical.health/de/cannabis/flowers"
  total: number; // Angebote
  pharmacy_count: number;
  strain_count: number;
  lowest_price: number | null;
  cheapest_gram: Highlight | null;
  cheapest_thc_gram: Highlight | null;
  cheapest_cbd_gram: Highlight | null;
  highest_thc: Highlight | null;
  highest_cbd: Highlight | null;
  highest_thc_cbd: Highlight | null; // max(thc*cbd), Tie-Break günstigster Preis
  best_rated: Highlight | null; // höchster rating_value bei review_count >= 5; price = min_price
  run: Run;
  /** Nächster geplanter Lauf (RFC 3339 UTC); null wenn SCRAPE_ENABLED=false. */
  next_run_at: string | null;
  /** Es existiert ein Lauf mit Status `running` (replikaübergreifend). */
  scrape_running: boolean;
  schedule: ScrapeSchedule | null;
  /** Preisalarm-Erstellung und Mailversand sind serverseitig konfiguriert. */
  email_enabled: boolean;
}

export interface ScrapeSchedule {
  cron: string;
  timezone: string;
}

/** List item of GET /strains: no offers (see GET /strains/{id}) and no search text. */
export type StrainListItem = Omit<Strain, 'offers' | 'search'>;

export interface FacetRange {
  min: number;
  max: number;
}

export interface Facets {
  /** Over ALL strains of the run, alphabetical (de), empty value omitted. */
  genetik: { value: string; count: number }[];
  /** Raw (unrounded) bounds over every strain with a value. */
  price: FacetRange | null;
  thc: FacetRange | null;
  cbd: FacetRange | null;
  rating: FacetRange | null;
}

export interface StrainsPage {
  run: Run;
  reference_run: Run | null;
  /** Hits after filtering. */
  total: number;
  limit: number;
  offset: number;
  /** Independent of the filter (slider bounds / chips). */
  facets: Facets;
  strains: StrainListItem[];
}

export type HistoryBucket = 'run' | 'day';
export interface HistoryPoint {
  run_id?: number; // nur bucket=run
  run_count?: number; // nur bucket=day
  at: string; // run: RFC3339; day: "YYYY-MM-DD" (Europe/Berlin)
  status?: RunStatus; // nur bucket=run
  min: number | null;
  avg: number | null;
  max: number | null;
  min_per_thc_gram: number | null;
  avg_per_thc_gram: number | null;
  max_per_thc_gram: number | null;
  offer_count: number;
  pharmacy_count: number;
}
export interface PharmacySeriesPoint {
  run_id?: number;
  at: string;
  price: number | null;
  price_per_thc_gram: number | null;
  availability: string;
}
export interface PharmacySeries {
  pharmacy_id: number;
  name: string;
  city: string;
  points: PharmacySeriesPoint[];
}
export interface History {
  strain_id: number;
  bucket: HistoryBucket;
  from: string;
  to: string;
  timezone: string;
  points: HistoryPoint[]; // aufsteigend
  pharmacies?: PharmacySeries[]; // nur bei ?pharmacies=true
}

export type OfferHistoryMode = 'changes' | 'all';

/** mode=all: one row per (bucket, pharmacy) with an offer. */
export interface OfferHistoryRow {
  at: string;
  run_id?: number;
  pharmacy_id: number;
  pharmacy: string;
  city: string;
  price: number | null;
  price_per_thc_gram: number | null;
  availability: string;
}

/** mode=changes: one row per pharmacy and consecutive stretch of runs with the same price+status. */
export interface OfferPhaseRow {
  pharmacy_id: number;
  pharmacy: string;
  city: string;
  price: number | null;
  price_per_thc_gram: number | null;
  availability: string;
  from: string;
  /** null ⇒ still holds in the last bucket of the range. */
  to: string | null;
  runs: number;
  delisted: boolean;
}

export interface OfferHistoryPage {
  strain_id: number;
  bucket: HistoryBucket;
  mode: OfferHistoryMode;
  from: string;
  to: string;
  total: number;
  limit: number;
  offset: number;
  rows: OfferHistoryRow[] | OfferPhaseRow[];
}

export interface Pharmacy {
  id: number;
  external_id: string;
  provider: Provider;
  name: string;
  plz: string;
  city: string;
  address: string;
  url: string;
  first_seen_at: string;
  last_seen_at: string;
  offer_count_latest: number;
}
export interface RunsResponse {
  runs: Run[];
  total: number;
}
export interface RunDetail extends Run {
  errors: RunError[];
}

export type ReviewSort = 'newest' | 'oldest' | 'highest' | 'lowest';

export interface Review {
  id: number;
  author: string;
  reviewed_on: string | null; // "YYYY-MM-DD"
  rating: number; // 0.0–5.0, halbe Sterne möglich
  verified: boolean;
  content: string;
  first_seen_at: string;
}

export interface ReviewDistribution {
  '1': number;
  '2': number;
  '3': number;
  '4': number;
  '5': number;
}

export interface ReviewSummary {
  value: number | null;
  count: number;
  scraped_at: string | null;
  distribution: ReviewDistribution; // gerundet auf ganze Sterne
  verified_count: number;
  stored_count: number;
}

export interface RatingHistoryPoint {
  at: string;
  value: number | null;
  count: number;
}

export interface ReviewsResponse {
  strain_id: number;
  summary: ReviewSummary;
  history: RatingHistoryPoint[]; // aufsteigend, max 400
  reviews: Review[];
  total: number;
}

export interface ApiErrorBody {
  error: {
    /** not_found | bad_request | unauthorized | conflict | no_data | internal */
    code: string;
    message: string;
  };
}

// --- Preisalarm-Abos --------------------------------------------------------

export type RuleKind =
  | 'strain_available'
  | 'strain_price_below'
  | 'any_price_below'
  | 'thc_above'
  | 'new_strain'
  | 'strain_price_change';

export interface RuleInput {
  kind: RuleKind;
  strain_id?: number;
  threshold?: number;
}

export interface Rule extends RuleInput {
  id: number;
  strain_name?: string | null;
  created_at: string;
}

export interface SubscriptionCreate {
  email: string;
  rules: RuleInput[];
  /** Honeypot, must stay empty. */
  website?: string;
}

export interface Subscription {
  email: string;
  confirmed: boolean;
  rules: Rule[];
  created_at: string;
}

export interface SubscriptionCreated {
  status: 'confirmation_sent';
}
