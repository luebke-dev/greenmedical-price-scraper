//! Serde DTOs. Field names follow `docs/api-contract.md` exactly.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// The 11 CSV columns of the original scraper, in order.
pub const CSV_FIELDNAMES: [&str; 11] = [
    "apotheke",
    "apotheke_plz",
    "apotheke_stadt",
    "name",
    "bezeichnung",
    "genetik",
    "thc",
    "cbd",
    "preis_pro_gramm",
    "verfuegbarkeit",
    "produkt_url",
];

/// Source URL reported in metadata.
pub const SOURCE_URL: &str = "https://greenmedical.health/de/cannabis/flowers";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum RunStatus {
    Running,
    Success,
    Partial,
    Failed,
}

impl RunStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            RunStatus::Running => "running",
            RunStatus::Success => "success",
            RunStatus::Partial => "partial",
            RunStatus::Failed => "failed",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "running" => Some(RunStatus::Running),
            "success" => Some(RunStatus::Success),
            "partial" => Some(RunStatus::Partial),
            "failed" => Some(RunStatus::Failed),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum RunTrigger {
    Schedule,
    Manual,
    Bootstrap,
}

impl RunTrigger {
    pub fn as_str(self) -> &'static str {
        match self {
            RunTrigger::Schedule => "schedule",
            RunTrigger::Manual => "manual",
            RunTrigger::Bootstrap => "bootstrap",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "schedule" => Some(RunTrigger::Schedule),
            "manual" => Some(RunTrigger::Manual),
            "bootstrap" => Some(RunTrigger::Bootstrap),
            _ => None,
        }
    }
}

/// `Run` in the contract.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct RunDto {
    pub id: i64,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub status: RunStatus,
    pub trigger: RunTrigger,
    pub instance: Option<String>,
    pub pharmacies_total: Option<i32>,
    pub pharmacies_scraped: Option<i32>,
    pub pharmacies_failed: Option<i32>,
    pub offer_count: Option<i32>,
    pub http_requests: Option<i32>,
    pub error: Option<String>,
    pub reviews_scraped: Option<i32>,
    pub reviews_failed: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct RunErrorDto {
    pub pharmacy_name: String,
    pub pharmacy_url: String,
    pub stage: String,
    pub message: String,
}

/// `RunDetail` in the contract.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct RunDetailDto {
    #[serde(flatten)]
    pub run: RunDto,
    pub errors: Vec<RunErrorDto>,
}

/// One scraped offer with parsed values and database identities.
///
/// This is the Rust equivalent of a row produced by `read_offers()` in the
/// old build script; every domain function consumes slices of it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct OfferRecord {
    pub offer_id: i64,
    pub pharmacy_id: i64,
    pub strain_id: i64,
    pub apotheke: String,
    pub apotheke_plz: String,
    pub apotheke_stadt: String,
    pub name: String,
    pub bezeichnung: String,
    pub genetik: String,
    pub thc: String,
    pub cbd: String,
    pub preis_pro_gramm: String,
    pub verfuegbarkeit: String,
    pub produkt_url: String,
    pub preis_eur_pro_gramm: Option<f64>,
    pub preis_eur_pro_gramm_thc: Option<f64>,
    pub preis_eur_pro_gramm_cbd: Option<f64>,
    pub thc_value: Option<f64>,
    pub cbd_value: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct OfferDto {
    pub offer_id: i64,
    pub pharmacy_id: i64,
    pub apotheke: String,
    pub apotheke_plz: String,
    pub apotheke_stadt: String,
    pub preis_pro_gramm: String,
    pub preis_eur_pro_gramm: Option<f64>,
    pub preis_eur_pro_gramm_thc: Option<f64>,
    pub verfuegbarkeit: String,
    pub produkt_url: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct SortDto {
    pub price: Option<f64>,
    pub price_per_thc_gram: Option<f64>,
    pub thc: Option<f64>,
    pub cbd: Option<f64>,
    /// `= rating.value` (null when never scraped or no reviews).
    pub rating: Option<f64>,
}

/// `Rating` in the contract: aggregate rating of a strain as last scraped.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct RatingDto {
    pub value: Option<f64>,
    pub count: i32,
    pub scraped_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum TrendDirection {
    Up,
    Down,
    Flat,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct TrendDto {
    pub reference_run_id: i64,
    pub reference_at: DateTime<Utc>,
    pub min_price_then: f64,
    pub delta: f64,
    pub delta_pct: f64,
    pub direction: TrendDirection,
}

/// `Strain` in the contract (a `flowers.json` record plus ids and trend).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct StrainDto {
    pub id: i64,
    pub name: String,
    pub bezeichnung: String,
    pub genetik: String,
    pub thc: String,
    pub cbd: String,
    pub thc_value: Option<f64>,
    pub cbd_value: Option<f64>,
    pub min_price: Option<f64>,
    pub min_price_per_thc_gram: Option<f64>,
    pub pharmacy_count: i64,
    pub offers: Vec<OfferDto>,
    pub sort: SortDto,
    pub search: String,
    pub trend: Option<TrendDto>,
    /// `null` until the product page was scraped for reviews.
    pub rating: Option<RatingDto>,
    pub product_uuid: Option<String>,
}

/// `StrainDetail` in the contract.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct StrainDetailDto {
    #[serde(flatten)]
    pub strain: StrainDto,
    pub first_seen_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub in_latest_run: bool,
    pub run: RunDto,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct HighlightDto {
    pub price: Option<f64>,
    pub name: String,
    pub apotheke: String,
    pub genetik: String,
    pub thc: String,
    pub cbd: String,
    pub produkt_url: String,
    pub strain_id: i64,
    pub pharmacy_id: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rating_value: Option<Option<f64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_count: Option<i32>,
}

/// `Metadata.schedule`: the active scrape schedule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ScheduleDto {
    /// `cron` crate expression (sec min hour dom mon dow).
    #[schema(example = "0 0 * * * *")]
    pub cron: String,
    /// IANA timezone the expression is evaluated in.
    #[schema(example = "Europe/Berlin")]
    pub timezone: String,
}

/// `Metadata` in the contract.
///
/// `next_run_at`, `scrape_running` and `schedule` are live fields: the snapshot
/// stores them as `None`/`false`/`None` and `GET /api/v1/metadata` fills them
/// per request (see `api::handlers::metadata`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct MetadataDto {
    pub generated_at: DateTime<Utc>,
    pub source: String,
    pub total: i64,
    pub pharmacy_count: i64,
    pub strain_count: i64,
    pub lowest_price: Option<f64>,
    pub cheapest_gram: Option<HighlightDto>,
    pub cheapest_thc_gram: Option<HighlightDto>,
    pub cheapest_cbd_gram: Option<HighlightDto>,
    pub highest_thc: Option<HighlightDto>,
    pub highest_cbd: Option<HighlightDto>,
    pub highest_thc_cbd: Option<HighlightDto>,
    /// Highest `rating_value` among strains with at least
    /// [`BEST_RATED_MIN_REVIEWS`] reviews; ties go to the strain with more reviews.
    pub best_rated: Option<HighlightDto>,
    pub run: RunDto,
    /// Next scheduled scrape (RFC 3339 UTC), `null` when the scheduler is disabled.
    #[serde(default)]
    pub next_run_at: Option<DateTime<Utc>>,
    /// A run with status `running` exists (any replica).
    #[serde(default)]
    pub scrape_running: bool,
    /// Active schedule, `null` when the scheduler is disabled.
    #[serde(default)]
    pub schedule: Option<ScheduleDto>,
    /// Whether subscription creation and outbound alert e-mail are available.
    #[serde(default)]
    pub email_enabled: bool,
}

/// Minimum `review_count` for a strain to qualify as `best_rated`.
pub const BEST_RATED_MIN_REVIEWS: i32 = 5;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ReviewDto {
    pub id: i64,
    pub author: String,
    pub reviewed_on: Option<chrono::NaiveDate>,
    pub rating: f64,
    pub verified: bool,
    pub content: String,
    pub first_seen_at: DateTime<Utc>,
}

/// Whole-star histogram of the stored reviews (`rating` rounded half up).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
pub struct RatingDistributionDto {
    #[serde(rename = "1")]
    pub one: i64,
    #[serde(rename = "2")]
    pub two: i64,
    #[serde(rename = "3")]
    pub three: i64,
    #[serde(rename = "4")]
    pub four: i64,
    #[serde(rename = "5")]
    pub five: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ReviewSummaryDto {
    pub value: Option<f64>,
    pub count: i32,
    pub scraped_at: Option<DateTime<Utc>>,
    pub distribution: RatingDistributionDto,
    pub verified_count: i64,
    pub stored_count: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct RatingHistoryPointDto {
    pub at: DateTime<Utc>,
    pub value: Option<f64>,
    pub count: i32,
}

/// `ReviewsResponse` in the contract.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ReviewsResponseDto {
    pub strain_id: i64,
    pub summary: ReviewSummaryDto,
    pub history: Vec<RatingHistoryPointDto>,
    pub reviews: Vec<ReviewDto>,
    pub total: i64,
}

/// `StrainListItem` in the contract: a [`StrainDto`] without `offers`/`search`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct StrainListItemDto {
    pub id: i64,
    pub name: String,
    pub bezeichnung: String,
    pub genetik: String,
    pub thc: String,
    pub cbd: String,
    pub thc_value: Option<f64>,
    pub cbd_value: Option<f64>,
    pub min_price: Option<f64>,
    pub min_price_per_thc_gram: Option<f64>,
    pub pharmacy_count: i64,
    pub sort: SortDto,
    pub trend: Option<TrendDto>,
    pub rating: Option<RatingDto>,
    pub product_uuid: Option<String>,
}

impl From<&StrainDto> for StrainListItemDto {
    fn from(s: &StrainDto) -> Self {
        Self {
            id: s.id,
            name: s.name.clone(),
            bezeichnung: s.bezeichnung.clone(),
            genetik: s.genetik.clone(),
            thc: s.thc.clone(),
            cbd: s.cbd.clone(),
            thc_value: s.thc_value,
            cbd_value: s.cbd_value,
            min_price: s.min_price,
            min_price_per_thc_gram: s.min_price_per_thc_gram,
            pharmacy_count: s.pharmacy_count,
            sort: s.sort.clone(),
            trend: s.trend.clone(),
            rating: s.rating.clone(),
            product_uuid: s.product_uuid.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct GenetikFacetDto {
    pub value: String,
    pub count: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct RangeDto {
    pub min: f64,
    pub max: f64,
}

/// `Facets` in the contract: computed over all strains of the run, independent of filters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct FacetsDto {
    pub genetik: Vec<GenetikFacetDto>,
    pub price: Option<RangeDto>,
    pub thc: Option<RangeDto>,
    pub cbd: Option<RangeDto>,
    pub rating: Option<RangeDto>,
}

/// `StrainsPage` in the contract.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct StrainsPageDto {
    pub run: RunDto,
    pub reference_run: Option<RunDto>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
    pub facets: FacetsDto,
    pub strains: Vec<StrainListItemDto>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum OfferHistoryMode {
    #[default]
    Changes,
    All,
}

/// `OfferHistoryRow` in the contract (`mode=all`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct OfferHistoryRowDto {
    pub at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<i64>,
    pub pharmacy_id: i64,
    pub pharmacy: String,
    pub city: String,
    pub price: Option<f64>,
    pub price_per_thc_gram: Option<f64>,
    pub availability: String,
}

/// `OfferPhaseRow` in the contract (`mode=changes`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct OfferPhaseRowDto {
    pub pharmacy_id: i64,
    pub pharmacy: String,
    pub city: String,
    pub price: Option<f64>,
    pub price_per_thc_gram: Option<f64>,
    pub availability: String,
    pub from: String,
    pub to: Option<String>,
    pub runs: i64,
    pub delisted: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(untagged)]
pub enum OfferHistoryRows {
    Changes(Vec<OfferPhaseRowDto>),
    All(Vec<OfferHistoryRowDto>),
}

/// `OfferHistoryPage` in the contract.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct OfferHistoryPageDto {
    pub strain_id: i64,
    pub bucket: HistoryBucket,
    pub mode: OfferHistoryMode,
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
    pub rows: OfferHistoryRows,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct PharmacyDto {
    pub id: i64,
    pub external_id: String,
    pub name: String,
    pub plz: String,
    pub city: String,
    pub address: String,
    pub url: String,
    pub first_seen_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub offer_count_latest: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct RunsResponseDto {
    pub runs: Vec<RunDto>,
    pub total: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum HistoryBucket {
    Run,
    Day,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct HistoryPointDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_count: Option<i64>,
    pub at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<RunStatus>,
    pub min: Option<f64>,
    pub avg: Option<f64>,
    pub max: Option<f64>,
    pub min_per_thc_gram: Option<f64>,
    pub avg_per_thc_gram: Option<f64>,
    pub max_per_thc_gram: Option<f64>,
    pub offer_count: i64,
    pub pharmacy_count: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct PharmacySeriesPointDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<i64>,
    pub at: String,
    pub price: Option<f64>,
    pub price_per_thc_gram: Option<f64>,
    pub availability: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct PharmacySeriesDto {
    pub pharmacy_id: i64,
    pub name: String,
    pub city: String,
    pub points: Vec<PharmacySeriesPointDto>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct HistoryDto {
    pub strain_id: i64,
    pub bucket: HistoryBucket,
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
    pub timezone: String,
    pub points: Vec<HistoryPointDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pharmacies: Option<Vec<PharmacySeriesDto>>,
}

/// `RuleKind` in the contract: what a subscription rule reacts to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RuleKind {
    /// Sorte hat im letzten Lauf wieder mindestens ein Angebot.
    StrainAvailable,
    /// `min_price` der Sorte ist unter den Schwellwert gefallen.
    StrainPriceBelow,
    /// Irgendeine Sorte ist unter den Schwellwert gefallen (Ereignis je Sorte).
    AnyPriceBelow,
    /// Neu gelistete Sorte mit `thc_value` über dem Schwellwert.
    ThcAbove,
    /// Jede neu gelistete Sorte.
    NewStrain,
    /// `min_price` der Sorte hat sich gegenüber dem vorherigen Lauf geändert.
    StrainPriceChange,
}

impl RuleKind {
    pub const ALL: [RuleKind; 6] = [
        RuleKind::StrainAvailable,
        RuleKind::StrainPriceBelow,
        RuleKind::AnyPriceBelow,
        RuleKind::ThcAbove,
        RuleKind::NewStrain,
        RuleKind::StrainPriceChange,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            RuleKind::StrainAvailable => "strain_available",
            RuleKind::StrainPriceBelow => "strain_price_below",
            RuleKind::AnyPriceBelow => "any_price_below",
            RuleKind::ThcAbove => "thc_above",
            RuleKind::NewStrain => "new_strain",
            RuleKind::StrainPriceChange => "strain_price_change",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.as_str() == value)
    }

    /// Whether the rule refers to one specific strain (`strain_id` required).
    pub fn needs_strain(self) -> bool {
        matches!(
            self,
            RuleKind::StrainAvailable | RuleKind::StrainPriceBelow | RuleKind::StrainPriceChange
        )
    }

    /// Whether the rule carries a threshold (€/g or % THC).
    pub fn needs_threshold(self) -> bool {
        matches!(
            self,
            RuleKind::StrainPriceBelow | RuleKind::AnyPriceBelow | RuleKind::ThcAbove
        )
    }

    /// German label used as the group heading in notification e-mails.
    pub fn label_de(self) -> &'static str {
        match self {
            RuleKind::StrainAvailable => "Sorte wieder verfügbar",
            RuleKind::StrainPriceBelow => "Preis der Sorte unter Schwellwert",
            RuleKind::AnyPriceBelow => "Preis unter Schwellwert",
            RuleKind::ThcAbove => "Neue Sorte mit THC über Schwellwert",
            RuleKind::NewStrain => "Neue Sorte",
            RuleKind::StrainPriceChange => "Preisänderung",
        }
    }
}

/// `RuleInput` in the contract.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct RuleInputDto {
    pub kind: RuleKind,
    /// Pflicht bei `strain_available`, `strain_price_below`, `strain_price_change`; sonst verboten.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strain_id: Option<i64>,
    /// €/g bei `strain_price_below`/`any_price_below`, % bei `thc_above`; sonst verboten.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub threshold: Option<f64>,
}

/// `Rule` in the contract.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct RuleDto {
    pub id: i64,
    pub kind: RuleKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strain_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub threshold: Option<f64>,
    /// Anzeigename der Sorte (`name`), `null` bei Regeln ohne Sorte.
    pub strain_name: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// `SubscriptionCreate` in the contract.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct SubscriptionCreateDto {
    #[schema(example = "max@example.org")]
    pub email: String,
    /// 1–20 Regeln.
    pub rules: Vec<RuleInputDto>,
    /// Honeypot: muss leer bleiben (Bots füllen es aus → 202 ohne Aktion).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub website: Option<String>,
}

/// Body of `PUT /subscriptions/manage`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct RulesUpdateDto {
    /// 1–20 Regeln; ersetzt alle bisherigen Regeln.
    pub rules: Vec<RuleInputDto>,
}

/// Body of `POST /subscriptions/confirm`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ConfirmDto {
    /// `confirm_token` aus der Bestätigungsmail.
    pub token: String,
}

/// `Subscription` in the contract.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct SubscriptionDto {
    pub email: String,
    pub confirmed: bool,
    pub rules: Vec<RuleDto>,
    pub created_at: DateTime<Utc>,
}

/// Response of `POST /subscriptions` (always, to avoid e-mail enumeration).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct SubscriptionAcceptedDto {
    #[schema(example = "confirmation_sent")]
    pub status: String,
}
