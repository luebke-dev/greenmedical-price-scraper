//! Scraping: HTTP client, HTML parsing, site orchestration and run lifecycle.

pub mod ansay;
pub mod client;
pub mod parse;
pub mod reviews;
pub mod run;
pub mod target;

use tracing::{info, warn};
use url::Url;

use crate::config::Config;
use crate::domain::{Provider, RunErrorDto};
use crate::scrape::client::{HttpError, ScrapeClient};
use crate::scrape::parse::{PharmacyRow, Product};
use crate::scrape::target::{make_delivery_target, with_delivery_target};

pub use run::{RunHandle, StartError, execute_run, start_run};

/// A product tile attached to the pharmacy it was scraped from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScrapedOffer {
    pub provider: Provider,
    pub pharmacy: PharmacyRow,
    pub pharmacy_uuid: String,
    pub product: Product,
}

/// Result of walking the whole site once.
#[derive(Debug, Clone, Default)]
pub struct SiteScrape {
    pub pharmacies_total: u32,
    pub pharmacies_resolved: u32,
    /// Listed pharmacies whose detail page carries no `pharmacyAvailability`
    /// UUID. They are skipped like in the Python scraper: not an error, not a
    /// failure and not part of the success-ratio denominator.
    pub pharmacies_skipped: u32,
    pub pharmacies_scraped: u32,
    pub pharmacies_failed: u32,
    pub offers: Vec<ScrapedOffer>,
    pub errors: Vec<RunErrorDto>,
    pub http_requests: u32,
}

#[derive(Debug, thiserror::Error)]
pub enum ScrapeError {
    /// The HTTP client for the run could not be built; aborts the whole run.
    #[error("http client could not be built: {0}")]
    Client(#[from] reqwest::Error),
    /// The pharmacy list itself could not be fetched; aborts the whole run.
    #[error("pharmacy list could not be fetched: {0}")]
    PharmacyList(HttpError),
    /// A page of one pharmacy could not be fetched; that pharmacy is skipped.
    #[error("{0}")]
    Http(#[from] HttpError),
    #[error("invalid URL: {0}")]
    Url(#[from] url::ParseError),
}

impl ScrapeError {
    /// HTTP attempts consumed by the failed request, for `http_requests` accounting.
    pub fn attempts(&self) -> u32 {
        match self {
            ScrapeError::PharmacyList(http) | ScrapeError::Http(http) => http.attempts,
            ScrapeError::Client(_) | ScrapeError::Url(_) => 0,
        }
    }
}

fn pharmacy_list_url(base: &Url) -> Result<Url, url::ParseError> {
    base.join("/de/cannabis/pharmacy/")
}

fn flowers_url(base: &Url, delivery_target: &str, page: u32) -> Result<Url, url::ParseError> {
    let mut url = base.join("/de/cannabis/flowers")?;
    url.query_pairs_mut()
        .append_pair("deliveryTarget", delivery_target)
        .append_pair("onlyShowIfAvailable", "1")
        .append_pair("page", &page.to_string());
    Ok(url)
}

/// Scrape all flower pages of one pharmacy. Stops on an empty page or when
/// the pagination says the last page was reached.
pub async fn scrape_flowers_for_pharmacy(
    client: &ScrapeClient,
    config: &Config,
    pharmacy: &PharmacyRow,
    pharmacy_uuid: &str,
    delivery_target: &str,
    http_requests: &mut u32,
) -> Result<Vec<ScrapedOffer>, ScrapeError> {
    let mut products = Vec::new();
    let mut page = 1u32;
    loop {
        let url = flowers_url(&config.scrape_base_url, delivery_target, page)?;
        let fetched = client.get_text(url).await?;
        *http_requests += fetched.attempts;
        let parsed = parse::parse_flowers_page(&fetched.body, &config.scrape_base_url);
        if parsed.products.is_empty() {
            break;
        }
        for mut product in parsed.products {
            if !product.produkt_url.is_empty() {
                product.produkt_url = with_delivery_target(&product.produkt_url, delivery_target);
            }
            products.push(ScrapedOffer {
                provider: Provider::Greenmedical,
                pharmacy: pharmacy.clone(),
                pharmacy_uuid: pharmacy_uuid.to_owned(),
                product,
            });
        }
        let Some((current, total)) = parsed.pagination else {
            break;
        };
        if current >= total {
            break;
        }
        page += 1;
        tokio::time::sleep(config.scrape_page_delay).await;
    }
    Ok(products)
}

/// Walk the site: pharmacy list → UUID per pharmacy → flower pages per pharmacy.
///
/// Individual pharmacy failures are recorded in `errors` and skipped; only a
/// failure to fetch the pharmacy list aborts the whole scrape.
pub async fn scrape_site(
    client: &ScrapeClient,
    config: &Config,
) -> Result<SiteScrape, ScrapeError> {
    let mut result = SiteScrape::default();
    let base = &config.scrape_base_url;

    info!("fetching pharmacies with live stock");
    let list = client
        .get_text(pharmacy_list_url(base)?)
        .await
        .map_err(ScrapeError::PharmacyList)?;
    result.http_requests += list.attempts;
    let pharmacies = parse::parse_pharmacies(&list.body, base);
    result.pharmacies_total = pharmacies.len() as u32;
    info!(count = pharmacies.len(), "found pharmacies with live stock");

    // Resolve UUIDs / delivery targets.
    let mut targets: Vec<(PharmacyRow, String, String)> = Vec::new();
    for (index, pharmacy) in pharmacies.iter().enumerate() {
        let url = match Url::parse(&pharmacy.url) {
            Ok(url) => url,
            Err(err) => {
                warn!(pharmacy = %pharmacy.name, %err, "invalid pharmacy url, skipping");
                result.errors.push(RunErrorDto {
                    pharmacy_name: pharmacy.name.clone(),
                    pharmacy_url: pharmacy.url.clone(),
                    stage: "uuid".into(),
                    message: format!("invalid url: {err}"),
                });
                continue;
            }
        };
        match client.get_text(url).await {
            Ok(fetched) => {
                result.http_requests += fetched.attempts;
                match parse::parse_pharmacy_uuid(&fetched.body) {
                    Some(uuid) => {
                        info!(index = index + 1, total = pharmacies.len(), pharmacy = %pharmacy.name, "UUID found");
                        targets.push((pharmacy.clone(), uuid.clone(), make_delivery_target(&uuid)));
                    }
                    None => {
                        warn!(index = index + 1, total = pharmacies.len(), pharmacy = %pharmacy.name, "no UUID, skipping");
                        result.pharmacies_skipped += 1;
                    }
                }
            }
            Err(err) => {
                result.http_requests += err.attempts;
                warn!(index = index + 1, total = pharmacies.len(), pharmacy = %pharmacy.name, %err, "failed to fetch UUID, skipping");
                result.errors.push(RunErrorDto {
                    pharmacy_name: pharmacy.name.clone(),
                    pharmacy_url: pharmacy.url.clone(),
                    stage: "uuid".into(),
                    message: err.to_string(),
                });
            }
        }
        tokio::time::sleep(config.scrape_pharmacy_delay).await;
    }
    result.pharmacies_resolved = targets.len() as u32;
    info!(
        count = targets.len(),
        "pharmacies with valid UUIDs, starting scrape"
    );

    for (index, (pharmacy, uuid, delivery_target)) in targets.iter().enumerate() {
        info!(index = index + 1, total = targets.len(), pharmacy = %pharmacy.name, "scraping pharmacy");
        match scrape_flowers_for_pharmacy(
            client,
            config,
            pharmacy,
            uuid,
            delivery_target,
            &mut result.http_requests,
        )
        .await
        {
            Ok(products) => {
                info!(pharmacy = %pharmacy.name, count = products.len(), "flowers found");
                result.pharmacies_scraped += 1;
                result.offers.extend(products);
                // Politeness delay only after a successful pharmacy (a failed
                // one already spent its retry backoff), like the Python scraper.
                tokio::time::sleep(config.scrape_page_delay).await;
            }
            Err(err) => {
                result.http_requests += err.attempts();
                warn!(pharmacy = %pharmacy.name, %err, "failed to scrape pharmacy, skipping");
                result.errors.push(RunErrorDto {
                    pharmacy_name: pharmacy.name.clone(),
                    pharmacy_url: pharmacy.url.clone(),
                    stage: "pages".into(),
                    message: err.to_string(),
                });
            }
        }
    }

    // Every error row is one failed pharmacy (stage `uuid` or `pages`);
    // pharmacies without a UUID are counted in `pharmacies_skipped` instead.
    result.pharmacies_failed = result.errors.len() as u32;
    if result.pharmacies_failed > 0 {
        warn!(
            failed = result.pharmacies_failed,
            "pharmacies could not be scraped and were skipped"
        );
    }
    if config.ansay_enabled {
        match ansay::scrape_ansay(client, config).await {
            Ok(ansay) => {
                result.pharmacies_total += ansay.pharmacies_total;
                result.pharmacies_resolved += ansay.pharmacies_resolved;
                result.pharmacies_scraped += ansay.pharmacies_scraped;
                result.pharmacies_failed += ansay.pharmacies_failed;
                result.http_requests += ansay.http_requests;
                result.errors.extend(ansay.errors);
                result.offers.extend(ansay.offers);
            }
            Err(err) => {
                warn!(%err, "DrAnsay source failed; keeping GreenMedical results");
                result.pharmacies_total += 1;
                result.pharmacies_failed += 1;
                result.errors.push(RunErrorDto {
                    pharmacy_name: "DrAnsay".into(),
                    pharmacy_url: config.ansay_base_url.to_string(),
                    stage: "pages".into(),
                    message: err.to_string(),
                });
            }
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flowers_url_carries_delivery_target_and_page() {
        let base = Url::parse("https://greenmedical.health").unwrap();
        let url = flowers_url(&base, "TOKEN=", 2).unwrap();
        assert_eq!(
            url.as_str(),
            "https://greenmedical.health/de/cannabis/flowers?deliveryTarget=TOKEN%3D&onlyShowIfAvailable=1&page=2"
        );
        assert_eq!(
            pharmacy_list_url(&base).unwrap().as_str(),
            "https://greenmedical.health/de/cannabis/pharmacy/"
        );
    }
}
