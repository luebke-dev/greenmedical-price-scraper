//! DrAnsay JSON catalogue importer. Only pharmacies with an explicitly enabled
//! `shipping` delivery method and actually available products are included.

use std::collections::HashSet;

use anyhow::Context as _;
use futures::{StreamExt as _, stream};
use serde::Deserialize;
use serde_json::json;
use tracing::{info, warn};
use url::Url;

use crate::config::Config;
use crate::domain::{Provider, RunErrorDto};
use crate::scrape::client::ScrapeClient;
use crate::scrape::parse::{PharmacyRow, Product};
use crate::scrape::{ScrapedOffer, SiteScrape};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Vendor {
    id: String,
    name: String,
    address: Option<Address>,
    #[serde(default)]
    delivery_methods: Vec<DeliveryMethod>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Address {
    address_line_one: Option<String>,
    address_line_two: Option<String>,
    city: Option<String>,
    postal_code: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct DeliveryMethod {
    kind: String,
    enabled: bool,
}

#[derive(Debug, Deserialize)]
struct VendorsResponse {
    items: Vec<Vendor>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProductsResponse {
    products: Vec<AnsayProduct>,
    #[serde(default)]
    has_more: bool,
    #[serde(default)]
    page: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AnsayProduct {
    id: String,
    name: String,
    kind: Option<String>,
    thc: Option<f64>,
    cbd: Option<f64>,
    genetics: Option<String>,
    #[serde(default)]
    is_out_of_stock: bool,
    #[serde(default)]
    vendor_prices: Vec<VendorPrice>,
}

#[derive(Debug, Deserialize)]
struct VendorPrice {
    vendor: serde_json::Value,
    price: i64,
    #[serde(default)]
    stock: i64,
    #[serde(default)]
    available: bool,
}

fn vendor_id(value: &serde_json::Value) -> Option<String> {
    value
        .as_str()
        .map(str::to_owned)
        .or_else(|| value.as_i64().map(|id| id.to_string()))
}

fn shipping_enabled(vendor: &Vendor) -> bool {
    vendor
        .delivery_methods
        .iter()
        .any(|method| method.kind == "shipping" && method.enabled)
}

fn decimal_label(cents: i64) -> String {
    format!("{:.2} €/g", cents as f64 / 100.0).replace('.', ",")
}

fn percent_label(value: Option<f64>) -> String {
    value
        .map(|v| {
            if v.fract() == 0.0 {
                format!("{v:.0}%")
            } else {
                format!("{v}%")
            }
        })
        .unwrap_or_default()
}

fn slug(value: &str) -> String {
    let mut out = String::new();
    let mut separator = false;
    for ch in value.chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() {
            if separator && !out.is_empty() {
                out.push('-');
            }
            out.push(ch);
            separator = false;
        } else {
            separator = true;
        }
    }
    out
}

fn pharmacy_row(base: &Url, vendor: &Vendor) -> PharmacyRow {
    let address = vendor.address.as_ref();
    let street = address
        .into_iter()
        .flat_map(|a| [&a.address_line_one, &a.address_line_two])
        .filter_map(Option::as_deref)
        .filter(|s| !s.trim().is_empty())
        .collect::<Vec<_>>()
        .join(", ");
    let mut url = base.join("/products").expect("valid base URL");
    url.query_pairs_mut()
        .append_pair("vendorId", &vendor.id)
        .append_pair("deliveryMethod", "shipping");
    PharmacyRow {
        name: vendor.name.clone(),
        url: url.to_string(),
        postal_code: address
            .and_then(|a| a.postal_code.clone())
            .unwrap_or_default(),
        city: address.and_then(|a| a.city.clone()).unwrap_or_default(),
        address: street,
    }
}

fn product_url(base: &Url, vendor_id: &str, product: &AnsayProduct) -> String {
    let name = format!(
        "{} {}",
        product.name,
        product.kind.as_deref().unwrap_or_default()
    );
    let mut url = base
        .join(&format!("/product/{}/{id}", slug(&name), id = product.id))
        .expect("valid base URL");
    url.query_pairs_mut()
        .append_pair("vendorId", vendor_id)
        .append_pair("deliveryMethod", "shipping");
    url.to_string()
}

fn convert_products(base: &Url, vendor: &Vendor, products: Vec<AnsayProduct>) -> Vec<ScrapedOffer> {
    let pharmacy = pharmacy_row(base, vendor);
    let mut seen = HashSet::new();
    products
        .into_iter()
        .filter_map(|product| {
            if product.is_out_of_stock || !seen.insert(product.id.clone()) {
                return None;
            }
            let offer = product.vendor_prices.iter().find(|price| {
                vendor_id(&price.vendor).as_deref() == Some(vendor.id.as_str())
                    && price.available
                    && price.stock > 0
            })?;
            Some(ScrapedOffer {
                provider: Provider::Ansay,
                pharmacy: pharmacy.clone(),
                pharmacy_uuid: format!("ansay:{}", vendor.id),
                product: Product {
                    name: product.name.clone(),
                    designation: product.kind.clone().unwrap_or_default(),
                    genetics: product
                        .genetics
                        .as_deref()
                        .unwrap_or_default()
                        .replace('-', " "),
                    thc: percent_label(product.thc),
                    cbd: percent_label(product.cbd),
                    price_per_gram: decimal_label(offer.price),
                    availability: "Auf Lager".into(),
                    product_url: product_url(base, &vendor.id, &product),
                },
            })
        })
        .collect()
}

async fn fetch_vendor_products(
    client: ScrapeClient,
    base: Url,
    vendor: Vendor,
    request_delay: std::time::Duration,
) -> anyhow::Result<(Vendor, Vec<AnsayProduct>, u32)> {
    let mut products = Vec::new();
    let mut page = 0u32;
    let mut requests = 0u32;
    loop {
        let mut url = base.join("/api/products")?;
        url.query_pairs_mut()
            .append_pair("page", &page.to_string())
            // The endpoint returns HTTP 500 above 500 (verified against the
            // production shop); most pharmacies still fit in one page.
            .append_pair("pageSize", "500")
            .append_pair("vendorId", &vendor.id)
            .append_pair("deliveryMethod", "shipping")
            .append_pair("sort", "price")
            .append_pair("order", "asc")
            .append_pair("filters", "{}");
        let fetched = client.get_text(url).await?;
        requests += fetched.attempts;
        let response: ProductsResponse =
            serde_json::from_str(&fetched.body).context("invalid DrAnsay products response")?;
        products.extend(response.products);
        // Keep the buffered slot occupied briefly so completing requests do
        // not turn into a burst against the next pharmacies.
        tokio::time::sleep(request_delay).await;
        if !response.has_more {
            break;
        }
        page = response.page + 1;
    }
    Ok((vendor, products, requests))
}

pub async fn scrape_ansay(client: &ScrapeClient, config: &Config) -> anyhow::Result<SiteScrape> {
    let base = config.ansay_base_url.clone();
    let mut landing = base.join("/products")?;
    landing
        .query_pairs_mut()
        .append_pair("vendorId", "all-shipping")
        .append_pair("deliveryMethod", "shipping");
    let primed = client.get_text(landing.clone()).await?;

    let vendors_url = base.join("/api/vendors-filtered")?;
    let fetched = client
        .post_json_text(
            vendors_url,
            &landing,
            &json!({
                "page": 0,
                "pageSize": 1000,
                "filter": { "deliveryMethodKinds": ["shipping"] }
            }),
        )
        .await?;
    let response: VendorsResponse =
        serde_json::from_str(&fetched.body).context("invalid DrAnsay vendors response")?;
    let vendors: Vec<_> = response
        .items
        .into_iter()
        .filter(shipping_enabled)
        .collect();
    info!(
        count = vendors.len(),
        "found DrAnsay pharmacies with shipping"
    );

    let mut result = SiteScrape {
        pharmacies_total: vendors.len() as u32,
        pharmacies_resolved: vendors.len() as u32,
        http_requests: primed.attempts + fetched.attempts,
        ..SiteScrape::default()
    };
    let concurrency = config.ansay_concurrency.max(1);
    let outcomes = stream::iter(vendors.into_iter().map(|vendor| {
        fetch_vendor_products(
            client.clone(),
            base.clone(),
            vendor,
            config.scrape_page_delay,
        )
    }))
    .buffer_unordered(concurrency)
    .collect::<Vec<_>>()
    .await;

    for outcome in outcomes {
        match outcome {
            Ok((vendor, products, requests)) => {
                result.http_requests += requests;
                let offers = convert_products(&base, &vendor, products);
                result.pharmacies_scraped += 1;
                result.offers.extend(offers);
            }
            Err(err) => {
                result.pharmacies_failed += 1;
                warn!(%err, "DrAnsay pharmacy failed");
                result.errors.push(RunErrorDto {
                    pharmacy_name: "DrAnsay pharmacy".into(),
                    pharmacy_url: base.to_string(),
                    stage: "pages".into(),
                    message: err.to_string(),
                });
            }
        }
    }
    info!(offers = result.offers.len(), "DrAnsay scrape finished");
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipping_and_duplicate_filter_are_strict() {
        let vendor: Vendor = serde_json::from_value(json!({
            "id": "1040", "name": "3 Königen Apotheke",
            "address": {"addressLineOne":"Hauptstr. 1", "city":"Köln", "postalCode":"50667"},
            "deliveryMethods": [{"kind":"shipping", "enabled":true}]
        }))
        .unwrap();
        let products: ProductsResponse = serde_json::from_value(json!({
            "products": [
                {"id":"7", "name":"Z Kush", "kind":"slouu 30/1", "thc":30, "cbd":1,
                 "vendorPrices":[{"vendor":1040,"price":363,"stock":20,"available":true}]},
                {"id":"7", "name":"Z Kush", "vendorPrices":[{"vendor":1040,"price":363,"stock":20,"available":true}]},
                {"id":"8", "name":"Leer", "vendorPrices":[{"vendor":1040,"price":400,"stock":0,"available":true}]}
            ], "hasMore": false, "page": 0
        })).unwrap();
        let offers = convert_products(
            &Url::parse("https://shop.dransay.com").unwrap(),
            &vendor,
            products.products,
        );
        assert_eq!(offers.len(), 1);
        assert_eq!(offers[0].product.price_per_gram, "3,63 €/g");
        assert!(
            offers[0]
                .product
                .product_url
                .contains("deliveryMethod=shipping")
        );
    }
}
