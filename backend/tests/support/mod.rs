//! Shared helpers for integration tests: a wiremock site, state builders and seeding.
#![allow(dead_code)]

use std::sync::Arc;

use chrono::{DateTime, Utc};
use greenmedical_backend::config::Config;
use greenmedical_backend::db::{offers, pharmacies, runs, strains};
use greenmedical_backend::domain::{RunStatus, RunTrigger, strain_key};
use greenmedical_backend::scrape::target::make_delivery_target;
use greenmedical_backend::state::{AppState, SharedState};
use sqlx::PgPool;
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{header, method, path, query_param, query_param_is_missing};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

/// Name of the PHP session cookie the real site uses.
pub const SESSION_COOKIE: &str = "PHPSESSID";
/// Session value the pharmacy list page hands out (no pharmacy selected yet).
pub const FRESH_SESSION: &str = "fresh";
/// Strain shown by the generic catalogue, i.e. when no pharmacy is selected.
/// A scraper without a cookie jar sees only this for every pharmacy.
pub const GENERIC_STRAIN: &str = "Katalog ohne Apotheke";

/// Test configuration: no delays, no backoff, mock base URL.
pub fn test_config(base_url: &str) -> Config {
    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://greenmedical:greenmedical@localhost:5432/greenmedical".into()
    });
    Config::parse_from_args([
        "greenmedical-backend",
        "--database-url",
        &database_url,
        "--scrape-base-url",
        base_url,
        "--scrape-pharmacy-delay",
        "0ms",
        "--scrape-page-delay",
        "0ms",
        "--scrape-backoff-factor",
        "0",
        "--instance-name",
        "test-instance",
        // sqlx::test loads a local .env (which may set ADMIN_TOKEN); pin the endpoint off.
        "--admin-token",
        "",
        "--log-format",
        "pretty",
    ])
    .expect("valid test config")
}

pub fn test_state_with(pool: PgPool, config: Config) -> SharedState {
    let state = AppState::new(config, pool, CancellationToken::new());
    state.ready.store(true, std::sync::atomic::Ordering::SeqCst);
    state
}

pub fn test_state(pool: PgPool, base_url: &str) -> SharedState {
    test_state_with(pool, test_config(base_url))
}

/// ASCII-only slug so wiremock path matchers see exactly what the client requests.
pub fn ascii_slug(value: &str) -> String {
    value
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

#[derive(Debug, Clone)]
pub struct MockTile {
    pub name: String,
    pub bezeichnung: String,
    pub genetik: String,
    pub thc: String,
    pub cbd: String,
    pub price: String,
    pub availability: String,
    pub slug: String,
}

impl MockTile {
    pub fn new(name: &str, bezeichnung: &str, price: &str) -> Self {
        Self {
            name: name.into(),
            bezeichnung: bezeichnung.into(),
            genetik: "Indica".into(),
            thc: "20%".into(),
            cbd: "1%".into(),
            price: price.into(),
            availability: "Auf Lager".into(),
            slug: format!("{}-{}", ascii_slug(bezeichnung), ascii_slug(name)),
        }
    }

    pub fn thc(mut self, thc: &str) -> Self {
        self.thc = thc.into();
        self
    }

    pub fn cbd(mut self, cbd: &str) -> Self {
        self.cbd = cbd.into();
        self
    }

    pub fn genetik(mut self, genetik: &str) -> Self {
        self.genetik = genetik.into();
        self
    }

    pub fn availability(mut self, availability: &str) -> Self {
        self.availability = availability.into();
        self
    }

    fn html(&self) -> String {
        let price = if self.price.is_empty() {
            String::new()
        } else {
            format!(
                r#"<span class="productGridTilePriceAmount bold">{}</span>"#,
                self.price
            )
        };
        format!(
            r#"<article class="productGridTile" data-test-title="{bez}">
  <div class="productGridTileStatusBar text-uppercase">
    <div class="productGridTileStatusAvailability text-truncate">{avail}</div>
  </div>
  <div class="flowerTileBadges">
    <div class="flowerTileBadge flowerTileBadgeThc">THC <span class="bold">{thc}</span></div>
    <div class="flowerTileBadge flowerTileBadgeCbd">CBD <span class="bold">{cbd}</span></div>
    <div class="flowerTileBadge flowerTileBadgeStrain text-uppercase text-truncate">{genetik}</div>
  </div>
  <div class="productGridTileTitleWrapper">
    <h2 class="bold text-truncate" title="{name}">{name}</h2>
    <div class="productGridTileTitle">
      <div class="text-uppercase text-truncate">Bezeichnung</div>
      <div class="bold text-truncate" title="{bez}">{bez}</div>
    </div>
    <div class="productGridTilePrice"><div class="productGridPricePharmacy">{price}</div></div>
    <a class="productGridTileReviewButton" href="/de/cannabis/flower/{slug}#reviews">Bewertungen</a>
  </div>
  <a class="productGridTileStretchedLink" href="/de/cannabis/flower/{slug}"></a>
</article>"#,
            bez = self.bezeichnung,
            avail = self.availability,
            thc = self.thc,
            cbd = self.cbd,
            genetik = self.genetik,
            name = self.name,
            price = price,
            slug = self.slug,
        )
    }
}

#[derive(Debug, Clone)]
pub struct MockPharmacy {
    pub name: String,
    pub plz: String,
    pub city: String,
    pub address: String,
    pub slug: String,
    pub uuid: Option<String>,
    pub pages: Vec<Vec<MockTile>>,
    /// Detail page status override (e.g. 500 → uuid stage failure).
    pub detail_status: u16,
    /// Flowers page status override (e.g. 500 → pages stage failure).
    pub pages_status: u16,
}

impl MockPharmacy {
    pub fn new(name: &str, uuid: &str, pages: Vec<Vec<MockTile>>) -> Self {
        Self {
            name: name.into(),
            plz: "10115".into(),
            city: "Berlin".into(),
            address: "Teststraße 1".into(),
            slug: ascii_slug(name),
            uuid: Some(uuid.into()),
            pages,
            detail_status: 200,
            pages_status: 200,
        }
    }

    pub fn city(mut self, plz: &str, city: &str) -> Self {
        self.plz = plz.into();
        self.city = city.into();
        self
    }

    pub fn without_uuid(mut self) -> Self {
        self.uuid = None;
        self
    }

    pub fn detail_status(mut self, status: u16) -> Self {
        self.detail_status = status;
        self
    }

    pub fn pages_status(mut self, status: u16) -> Self {
        self.pages_status = status;
        self
    }

    pub fn delivery_target(&self) -> String {
        make_delivery_target(self.uuid.as_deref().unwrap_or(""))
    }

    /// PHP session value that stands for "this pharmacy is selected".
    pub fn session(&self) -> String {
        format!("sess-{}", self.slug)
    }
}

fn set_cookie(template: ResponseTemplate, session: &str) -> ResponseTemplate {
    template.insert_header(
        "Set-Cookie",
        format!("{SESSION_COOKIE}={session}; Path=/; HttpOnly").as_str(),
    )
}

/// The real site answers a flowers request carrying `deliveryTarget` with
/// `302 Location: /de/cannabis/flowers?page=N` and stores the selected
/// pharmacy in the PHP session (`Set-Cookie: PHPSESSID=…`). Only a client
/// that sends that cookie on the redirected request gets the pharmacy's
/// tiles; everyone else gets the generic catalogue.
struct SelectPharmacy {
    session: String,
}

impl Respond for SelectPharmacy {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let page = request
            .url
            .query_pairs()
            .find(|(key, _)| key == "page")
            .map(|(_, value)| value.into_owned())
            .unwrap_or_else(|| "1".into());
        set_cookie(
            ResponseTemplate::new(302).insert_header(
                "Location",
                format!("/de/cannabis/flowers?page={page}").as_str(),
            ),
            &self.session,
        )
    }
}

/// Tiles of the generic catalogue (no pharmacy selected).
pub fn generic_catalogue() -> Vec<MockTile> {
    vec![MockTile::new(GENERIC_STRAIN, "Generisch 0/0", "1,00 €/g")]
}

pub fn flowers_page_html(tiles: &[MockTile], pagination: Option<(usize, usize)>) -> String {
    let tiles_html: Vec<String> = tiles.iter().map(MockTile::html).collect();
    let pagination_html = pagination
        .map(|(c, t)| {
            format!(
                r#"<div class="paginationContainer"><a class="btn">Zurück</a><div class="mx-5">{c} / {t}</div><a class="btn">Weiter</a></div>"#
            )
        })
        .unwrap_or_default();
    format!(
        "<!DOCTYPE html><html><body><div class=\"productGrid\">\n{}\n</div>{}</body></html>",
        tiles_html.join("\n"),
        pagination_html
    )
}

/// A wiremock stand-in for greenmedical.health.
pub struct MockSite {
    pub server: MockServer,
    pub pharmacies: Vec<MockPharmacy>,
}

impl MockSite {
    pub async fn start(pharmacies: Vec<MockPharmacy>) -> Self {
        let server = MockServer::start().await;
        let site = Self { server, pharmacies };
        site.mount_all().await;
        site
    }

    pub fn base_url(&self) -> String {
        self.server.uri()
    }

    fn list_html(&self) -> String {
        let rows: Vec<String> = self
            .pharmacies
            .iter()
            .map(|p| {
                format!(
                    "<tr><td><a href=/de/cannabis/pharmacy/{}>{}</a></td><td>{}</td><td>{}</td><td>{}</td><td></td><td></td></tr>",
                    p.slug, p.name, p.plz, p.city, p.address
                )
            })
            .collect();
        format!(
            "<!DOCTYPE html><html><body><h3>Partnerapotheken</h3><table><thead><tr><th>Apothekenname</th><th>PLZ</th><th>Stadt</th><th>Adresse</th><th>Versand</th><th>Abholung</th></tr></thead><tbody>{}</tbody></table>\
             <h3>Weitere</h3><table><tr><th>x</th></tr><tr><td><a href=/de/cannabis/pharmacy/ignored>Ignored</a></td><td>1</td><td>2</td><td>3</td></tr></table></body></html>",
            rows.join("")
        )
    }

    async fn mount_list_with_priority(&self, template: ResponseTemplate, priority: u8) {
        Mock::given(method("GET"))
            .and(path("/de/cannabis/pharmacy/"))
            .respond_with(template)
            .with_priority(priority)
            .mount(&self.server)
            .await;
    }

    /// Override the pharmacy list page (failures, delays); wins over the default mock.
    pub async fn mount_list(&self, template: ResponseTemplate) {
        self.mount_list_with_priority(template, 1).await;
    }

    /// Mount the site. The flowers pages follow the real session flow:
    /// `?deliveryTarget=…&page=N` → 302 + `Set-Cookie` → `?page=N` served by
    /// cookie. Without the right cookie every flowers request yields the
    /// generic catalogue, exactly like the live site.
    async fn mount_all(&self) {
        // The first page visited starts a session, like PHP does.
        self.mount_list_with_priority(
            set_cookie(
                ResponseTemplate::new(200).set_body_string(self.list_html()),
                FRESH_SESSION,
            ),
            5,
        )
        .await;

        // Generic catalogue: no `deliveryTarget` and no (matching) session cookie.
        Mock::given(method("GET"))
            .and(path("/de/cannabis/flowers"))
            .and(query_param_is_missing("deliveryTarget"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(flowers_page_html(&generic_catalogue(), Some((1, 1)))),
            )
            .with_priority(9)
            .mount(&self.server)
            .await;

        for pharmacy in &self.pharmacies {
            let detail = match (&pharmacy.uuid, pharmacy.detail_status) {
                (_, status) if status != 200 => ResponseTemplate::new(status),
                (Some(uuid), _) => ResponseTemplate::new(200).set_body_string(format!(
                    "<html><body><div class=\"liveStock\"><a class=\"btn\" href=/de/cannabis/products?pharmacyAvailability={uuid}>Livebestand Übersicht</a></div></body></html>"
                )),
                (None, _) => ResponseTemplate::new(200)
                    .set_body_string("<html><body><a href=\"/de/cannabis/flowers\">Blüten</a></body></html>"),
            };
            Mock::given(method("GET"))
                .and(path(format!("/de/cannabis/pharmacy/{}", pharmacy.slug)))
                .respond_with(detail)
                .mount(&self.server)
                .await;

            let Some(_) = &pharmacy.uuid else { continue };
            let target = pharmacy.delivery_target();
            let session = pharmacy.session();
            let cookie = format!("{SESSION_COOKIE}={session}");

            // Selecting the pharmacy: 302 to `?page=N` plus the session cookie.
            Mock::given(method("GET"))
                .and(path("/de/cannabis/flowers"))
                .and(query_param("deliveryTarget", target.clone()))
                .and(query_param("onlyShowIfAvailable", "1"))
                .respond_with(SelectPharmacy {
                    session: session.clone(),
                })
                .with_priority(2)
                .mount(&self.server)
                .await;

            let total = pharmacy.pages.len();
            for (index, tiles) in pharmacy.pages.iter().enumerate() {
                let template = if pharmacy.pages_status != 200 {
                    ResponseTemplate::new(pharmacy.pages_status)
                } else {
                    ResponseTemplate::new(200)
                        .set_body_string(flowers_page_html(tiles, Some((index + 1, total))))
                };
                Mock::given(method("GET"))
                    .and(path("/de/cannabis/flowers"))
                    .and(query_param_is_missing("deliveryTarget"))
                    .and(query_param("page", (index + 1).to_string()))
                    .and(header("cookie", cookie.as_str()))
                    .respond_with(template)
                    .with_priority(2)
                    .mount(&self.server)
                    .await;
            }
            // Any other page of this pharmacy's session is empty.
            Mock::given(method("GET"))
                .and(path("/de/cannabis/flowers"))
                .and(query_param_is_missing("deliveryTarget"))
                .and(header("cookie", cookie.as_str()))
                .respond_with(
                    ResponseTemplate::new(200).set_body_string(flowers_page_html(&[], None)),
                )
                .with_priority(8)
                .mount(&self.server)
                .await;
        }
    }

    /// Serve the product page of a tile (`/de/cannabis/flower/<slug>`).
    /// Without a mounted page wiremock answers 404, i.e. phase 2 counts a failure.
    pub async fn mount_product(&self, tile: &MockTile, template: ResponseTemplate) {
        Mock::given(method("GET"))
            .and(path(format!("/de/cannabis/flower/{}", tile.slug)))
            .respond_with(template)
            .with_priority(1)
            .mount(&self.server)
            .await;
    }

    /// Product page requests (phase 2) seen so far.
    pub async fn product_requests(&self) -> Vec<Request> {
        self.requests()
            .await
            .into_iter()
            .filter(|r| r.url.path().starts_with("/de/cannabis/flower/"))
            .collect()
    }

    /// All requests the mock server has seen so far.
    pub async fn requests(&self) -> Vec<Request> {
        self.server
            .received_requests()
            .await
            .expect("request recording enabled")
    }
}

/// Two pharmacies, three strains, one strain shared, one page each.
pub fn default_site() -> Vec<MockPharmacy> {
    vec![
        MockPharmacy::new(
            "Grüne Blüte",
            "b4bddcc5-dc41-49d8-87df-14a03d561b32",
            vec![
                vec![
                    MockTile::new("Bunatic", "Luana 27/1 Donny B", "5,49 €/g").thc("27%"),
                    MockTile::new("OG Kush", "Cannamedical CM 24/1", "6,49 €/g")
                        .thc("24%")
                        .genetik("Hybrid Sativa Dominant"),
                ],
                vec![
                    MockTile::new("Cosmic Cream", "Pedanios 31/1 COS-CA", "6,29 €/g")
                        .thc("31%")
                        .cbd("<1%"),
                ],
            ],
        )
        .city("04416", "Markkleeberg"),
        MockPharmacy::new(
            "Asavita",
            "11111111-2222-3333-4444-555555555555",
            vec![vec![
                MockTile::new("OG Kush", "Cannamedical CM 24/1", "5,99 €/g")
                    .thc("24%")
                    .availability("NEU"),
            ]],
        )
        .city("10365", "Berlin"),
    ]
}

/// One review on a mock product page.
#[derive(Debug, Clone)]
pub struct MockReview {
    pub author: String,
    pub date: String,
    /// Full stars (0–5); `half` adds a half star.
    pub stars: u8,
    pub half: bool,
    pub verified: bool,
    pub content: String,
}

impl MockReview {
    pub fn new(author: &str, date: &str, stars: u8, content: &str) -> Self {
        Self {
            author: author.into(),
            date: date.into(),
            stars,
            half: false,
            verified: true,
            content: content.into(),
        }
    }

    pub fn half(mut self) -> Self {
        self.half = true;
        self
    }

    pub fn unverified(mut self) -> Self {
        self.verified = false;
        self
    }

    fn html(&self) -> String {
        let mut stars = String::new();
        for _ in 0..self.stars {
            stars.push_str(r#"<i class="fa-solid fa-star fullStar"></i>"#);
        }
        if self.half {
            stars.push_str(r#"<i class="fa-solid fa-star-half-stroke halfStar"></i>"#);
        }
        let badge = if self.verified {
            r#"<div class="small color-primary-200 ps-1"><i class="fa-solid fa-badge-check"></i> Verifizierter Kauf</div>"#
        } else {
            ""
        };
        format!(
            r#"<div class="pdpReview"><div><div class="pdpReviewHeader"><div class="pdpReviewAuthor">
  <div class="pdpReviewBadge"><span class="">X</span></div>
  <div class="pdpReviewName"><div><span class="">{author}</span></div>
    <div class="pdpReviewRating"><div class="ratingStars">{stars}</div></div></div></div>
  <div class="pdpReviewDate"> {date} </div></div></div>
  <div class="pdpReviewContent"><p class=""> {content} </p>{badge}</div></div>"#,
            author = self.author,
            date = self.date,
            content = self.content,
        )
    }
}

/// A product page in the layout of the real site: JSON-LD aggregate rating
/// (omitted for 0 reviews), header spans, feedback-modal UUID and the reviews.
pub fn product_page_html(
    uuid: &str,
    rating: Option<f64>,
    count: u32,
    reviews: &[MockReview],
) -> String {
    let json_ld = match rating {
        Some(value) => format!(
            r#"<script type="application/ld+json">{{"@context":"https://schema.org","@graph":[{{"@type":"Product","name":"X","aggregateRating":{{"@type":"AggregateRating","ratingValue":"{value}","reviewCount":"{count}"}}}}]}}</script>"#
        ),
        None => String::new(),
    };
    let header = match rating {
        Some(value) => format!("<span>{value}</span> <span>({count})</span>"),
        None => String::new(),
    };
    let reviews_html: Vec<String> = reviews.iter().map(MockReview::html).collect();
    format!(
        r#"<!DOCTYPE html><html><head>{json_ld}</head><body>
<section id="reviews"><div class="pdpReviewsHeader"><h4>Bewertungen</h4>
<div class="pdpReviewsHeaderRating"><div class="ratingStars"><i class="fa-solid fa-star fullStar"></i>{header}</div>
<button data-behavior="openAjaxModal" data-modal-url="/de/cannabis/feedback/modal/{uuid}">Jetzt Bewerten</button></div></div>
<div class="pdpReviewsContainer">{}</div></section></body></html>"#,
        reviews_html.join("\n")
    )
}

/// One seeded offer for `seed_run`.
#[derive(Debug, Clone)]
pub struct SeedOffer {
    pub pharmacy: (&'static str, &'static str), // (external_id, name)
    pub strain: (&'static str, &'static str),   // (name, bezeichnung)
    pub price: Option<f64>,
    pub thc: &'static str,
}

impl SeedOffer {
    pub fn new(
        pharmacy: (&'static str, &'static str),
        strain: (&'static str, &'static str),
        price: f64,
    ) -> Self {
        Self {
            pharmacy,
            strain,
            price: Some(price),
            thc: "20%",
        }
    }
}

/// Seed a finished run with explicit timestamps directly in the database.
pub async fn seed_run(
    pool: &PgPool,
    started_at: DateTime<Utc>,
    status: RunStatus,
    seeds: &[SeedOffer],
) -> i64 {
    let run_id = runs::insert_running_at(pool, RunTrigger::Schedule, "seed", started_at)
        .await
        .unwrap();
    let mut tx = pool.begin().await.unwrap();
    let pharmacy_inputs: Vec<_> = seeds
        .iter()
        .map(|s| pharmacies::PharmacyInput {
            external_id: s.pharmacy.0.into(),
            name: s.pharmacy.1.into(),
            plz: "10115".into(),
            city: "Berlin".into(),
            address: "Teststraße 1".into(),
            url: format!("https://example.test/{}", s.pharmacy.0),
        })
        .collect();
    let pharmacy_ids = pharmacies::upsert_many(&mut *tx, &pharmacy_inputs)
        .await
        .unwrap();
    let strain_inputs: Vec<_> = seeds
        .iter()
        .map(|s| strains::StrainInput {
            name_key: strain_key(s.strain.0),
            bezeichnung_key: strain_key(s.strain.1),
            name: s.strain.0.into(),
            bezeichnung: s.strain.1.into(),
            genetik: "Indica".into(),
            thc_label: s.thc.into(),
            cbd_label: "1%".into(),
        })
        .collect();
    let strain_ids = strains::upsert_many(&mut *tx, &strain_inputs)
        .await
        .unwrap();
    let inserts: Vec<_> = seeds
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let thc = greenmedical_backend::domain::parse_percent(s.thc);
            offers::OfferInsert {
                pharmacy_id: pharmacy_ids[s.pharmacy.0],
                strain_id: strain_ids[&(strain_key(s.strain.0), strain_key(s.strain.1))],
                position: i as i32,
                genetik: "Indica".into(),
                thc_label: s.thc.into(),
                cbd_label: "1%".into(),
                price_label: s
                    .price
                    .map(|p| format!("{p:.2} €/g").replace('.', ","))
                    .unwrap_or_default(),
                availability: "Auf Lager".into(),
                product_url: format!("https://example.test/p/{}", i),
                price_eur: s.price,
                thc_pct: thc,
                cbd_pct: Some(1.0),
                price_per_thc_g: greenmedical_backend::domain::calculate_thc_price(s.price, thc),
                price_per_cbd_g: greenmedical_backend::domain::calculate_thc_price(
                    s.price,
                    Some(1.0),
                ),
            }
        })
        .collect();
    offers::insert_many(&mut *tx, run_id, &inserts)
        .await
        .unwrap();
    runs::finish_at(
        &mut *tx,
        run_id,
        status,
        runs::RunCounts {
            pharmacies_total: pharmacy_ids.len() as i32,
            pharmacies_scraped: pharmacy_ids.len() as i32,
            pharmacies_failed: 0,
            offer_count: inserts.len() as i32,
            http_requests: 3,
        },
        started_at + chrono::Duration::seconds(30),
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    run_id
}

pub fn shared(state: &SharedState) -> SharedState {
    Arc::clone(state)
}
