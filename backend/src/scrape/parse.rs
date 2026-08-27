//! Pure HTML parsing, ported from `scraper.py` (BeautifulSoup semantics).

use std::sync::LazyLock;

use regex::Regex;
use scraper::{CaseSensitivity, ElementRef, Html, Selector};
use url::Url;

static UUID_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"pharmacyAvailability=([a-f0-9-]+)").expect("static regex"));
static PAGINATION_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(\d+)\s*/\s*(\d+)").expect("static regex"));

fn sel(css: &str) -> Selector {
    Selector::parse(css).expect("static selector")
}

static SEL_TABLE: LazyLock<Selector> = LazyLock::new(|| sel("table"));
static SEL_TR: LazyLock<Selector> = LazyLock::new(|| sel("tr"));
static SEL_TD: LazyLock<Selector> = LazyLock::new(|| sel("td"));
static SEL_A: LazyLock<Selector> = LazyLock::new(|| sel("a"));
static SEL_A_HREF: LazyLock<Selector> = LazyLock::new(|| sel("a[href]"));
static SEL_PAGINATION: LazyLock<Selector> = LazyLock::new(|| sel("div.paginationContainer"));
static SEL_TILE: LazyLock<Selector> = LazyLock::new(|| sel("article.productGridTile"));
static SEL_H2: LazyLock<Selector> = LazyLock::new(|| sel("h2"));
static SEL_BOLD_SPAN: LazyLock<Selector> = LazyLock::new(|| sel("span.bold"));
static SEL_BADGE_THC: LazyLock<Selector> =
    LazyLock::new(|| sel(r#"div[class*="flowerTileBadgeThc"]"#));
static SEL_BADGE_CBD: LazyLock<Selector> =
    LazyLock::new(|| sel(r#"div[class*="flowerTileBadgeCbd"]"#));
static SEL_STRAIN: LazyLock<Selector> =
    LazyLock::new(|| sel(r#"div[class*="flowerTileBadgeStrain"]"#));
static SEL_UPPERCASE_DIV: LazyLock<Selector> = LazyLock::new(|| sel("div.text-uppercase"));
static SEL_PRICE: LazyLock<Selector> =
    LazyLock::new(|| sel(r#"span[class*="productGridTilePriceAmount"]"#));
static SEL_AVAILABILITY: LazyLock<Selector> =
    LazyLock::new(|| sel(r#"div[class*="productGridTileStatusAvailability"]"#));

/// A row of the pharmacy table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PharmacyRow {
    pub name: String,
    pub url: String,
    pub postal_code: String,
    pub city: String,
    pub address: String,
}

/// A product tile, before pharmacy fields are attached.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Product {
    pub name: String,
    pub designation: String,
    pub genetics: String,
    pub thc: String,
    pub cbd: String,
    pub price_per_gram: String,
    pub availability: String,
    pub product_url: String,
}

/// BeautifulSoup `get_text(strip=True)`: every text node stripped and concatenated.
pub fn text_strip(element: ElementRef<'_>) -> String {
    element
        .text()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .concat()
}

fn join_url(base: &Url, href: &str) -> String {
    match base.join(href) {
        Ok(url) => url.to_string(),
        Err(_) => href.to_owned(),
    }
}

/// Parse the pharmacies with live stock: first table only, rows need ≥ 4
/// cells and a link in the first cell.
pub fn parse_pharmacies(html: &str, base_url: &Url) -> Vec<PharmacyRow> {
    let document = Html::parse_document(html);
    let mut pharmacies = Vec::new();
    let Some(table) = document.select(&SEL_TABLE).next() else {
        return pharmacies;
    };
    for row in table.select(&SEL_TR).skip(1) {
        let cells: Vec<ElementRef<'_>> = row.select(&SEL_TD).collect();
        if cells.len() < 4 {
            continue;
        }
        let Some(link) = cells[0].select(&SEL_A).next() else {
            continue;
        };
        let href = link.attr("href").unwrap_or_default();
        let url = if href.starts_with("http") {
            href.to_owned()
        } else {
            join_url(base_url, href)
        };
        pharmacies.push(PharmacyRow {
            name: text_strip(link),
            url,
            postal_code: text_strip(cells[1]),
            city: text_strip(cells[2]),
            address: text_strip(cells[3]),
        });
    }
    pharmacies
}

/// Extract the pharmacy UUID from the first `pharmacyAvailability=<uuid>` link.
pub fn parse_pharmacy_uuid(html: &str) -> Option<String> {
    let document = Html::parse_document(html);
    document
        .select(&SEL_A_HREF)
        .filter_map(|a| a.attr("href"))
        .find_map(|href| UUID_RE.captures(href).map(|c| c[1].to_owned()))
}

/// Read `(current, total)` from the "n / m" pagination container, if present.
pub fn parse_pagination(document: &Html) -> Option<(u32, u32)> {
    let container = document.select(&SEL_PAGINATION).next()?;
    let text = text_strip(container);
    let caps = PAGINATION_RE.captures(&text)?;
    Some((caps[1].parse().ok()?, caps[2].parse().ok()?))
}

/// Extract a THC/CBD badge value (`badge` selects the badge div), preferring the bold span.
pub fn extract_badge_value(tile: ElementRef<'_>, badge: &Selector) -> String {
    let Some(badge) = tile.select(badge).next() else {
        return String::new();
    };
    match badge.select(&SEL_BOLD_SPAN).next() {
        Some(bold) => text_strip(bold),
        None => text_strip(badge),
    }
}

/// Find the product detail link: anchor around the title, anchor inside the
/// title, then the first anchor of the tile.
pub fn extract_product_url(
    tile: ElementRef<'_>,
    h2: Option<ElementRef<'_>>,
    base_url: &Url,
) -> String {
    let mut href = "";
    if let Some(h2) = h2 {
        let parent_anchor = h2
            .ancestors()
            .filter_map(ElementRef::wrap)
            .find(|el| el.value().name() == "a");
        let anchor = parent_anchor.or_else(|| h2.select(&SEL_A_HREF).next());
        if let Some(value) = anchor.and_then(|a| a.attr("href")) {
            href = value;
        }
    }
    if href.is_empty()
        && let Some(anchor) = tile.select(&SEL_A_HREF).next()
        && let Some(value) = anchor.attr("href")
    {
        href = value;
    }
    if href.is_empty() {
        String::new()
    } else {
        join_url(base_url, href)
    }
}

/// Extract product data from a single tile.
pub fn extract_product(tile: ElementRef<'_>, base_url: &Url) -> Product {
    let h2 = tile.select(&SEL_H2).next();
    let name = h2.map(text_strip).unwrap_or_default();

    let thc = extract_badge_value(tile, &SEL_BADGE_THC);
    let cbd = extract_badge_value(tile, &SEL_BADGE_CBD);
    let genetics = tile
        .select(&SEL_STRAIN)
        .next()
        .map(text_strip)
        .unwrap_or_default();

    // GreenMedical's "Bezeichnung" label div, then the next sibling div with class "bold".
    let mut designation = String::new();
    for div in tile.select(&SEL_UPPERCASE_DIV) {
        if text_strip(div).to_lowercase().contains("bezeichnung") {
            let next_bold = div.next_siblings().filter_map(ElementRef::wrap).find(|el| {
                el.value().name() == "div"
                    && el.value().has_class("bold", CaseSensitivity::CaseSensitive)
            });
            if let Some(bold) = next_bold {
                designation = text_strip(bold);
            }
            break;
        }
    }

    let price_per_gram = tile
        .select(&SEL_PRICE)
        .next()
        .map(text_strip)
        .unwrap_or_default();
    let availability = tile
        .select(&SEL_AVAILABILITY)
        .next()
        .map(text_strip)
        .unwrap_or_default();

    Product {
        name,
        designation,
        genetics,
        thc,
        cbd,
        price_per_gram,
        availability,
        product_url: extract_product_url(tile, h2, base_url),
    }
}

/// Parsed flowers listing page: tiles in document order plus pagination.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FlowersPage {
    pub products: Vec<Product>,
    pub pagination: Option<(u32, u32)>,
}

/// Parse one flowers listing page.
pub fn parse_flowers_page(html: &str, base_url: &Url) -> FlowersPage {
    let document = Html::parse_document(html);
    let products = document
        .select(&SEL_TILE)
        .map(|tile| extract_product(tile, base_url))
        .collect();
    FlowersPage {
        products,
        pagination: parse_pagination(&document),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn base() -> Url {
        Url::parse("https://greenmedical.health").unwrap()
    }

    fn with_tile<T>(html: &str, f: impl FnOnce(ElementRef<'_>) -> T) -> T {
        let document = Html::parse_fragment(html);
        let tile = document.select(&sel("article")).next().expect("article");
        f(tile)
    }

    #[test]
    fn badge_prefers_bold_span() {
        let html = r#"<article><div class="flowerTileBadgeThc">THC <span class="bold">20%</span></div></article>"#;
        assert_eq!(
            with_tile(html, |t| extract_badge_value(t, &SEL_BADGE_THC)),
            "20%"
        );
    }

    #[test]
    fn badge_falls_back_to_full_text() {
        let html = r#"<article><div class="flowerTileBadgeCbd">CBD 1%</div></article>"#;
        assert_eq!(
            with_tile(html, |t| extract_badge_value(t, &SEL_BADGE_CBD)),
            "CBD 1%"
        );
    }

    #[test]
    fn missing_badge_returns_empty_string() {
        assert_eq!(
            with_tile("<article></article>", |t| extract_badge_value(
                t,
                &SEL_BADGE_THC
            )),
            ""
        );
    }

    #[test]
    fn extracts_all_fields() {
        let html = r#"
            <article class="productGridTile">
              <a href="/de/cannabis/flowers/test-bluete"><h2>Test Blüte 20/1</h2></a>
              <div class="flowerTileBadgeThc">THC <span class="bold">20%</span></div>
              <div class="flowerTileBadgeCbd">CBD <span class="bold">1%</span></div>
              <div class="flowerTileBadgeStrain">Indica</div>
              <div class="text-uppercase">Bezeichnung</div>
              <div class="bold">EMK</div>
              <span class="productGridTilePriceAmount">9,50 €</span>
              <div class="productGridTileStatusAvailability">verfügbar</div>
            </article>
        "#;
        let product = with_tile(html, |t| extract_product(t, &base()));
        assert_eq!(
            product,
            Product {
                name: "Test Blüte 20/1".into(),
                designation: "EMK".into(),
                genetics: "Indica".into(),
                thc: "20%".into(),
                cbd: "1%".into(),
                price_per_gram: "9,50 €".into(),
                availability: "verfügbar".into(),
                product_url: "https://greenmedical.health/de/cannabis/flowers/test-bluete".into(),
            }
        );
    }

    #[test]
    fn missing_fields_default_to_empty() {
        let product = with_tile(r#"<article class="productGridTile"></article>"#, |t| {
            extract_product(t, &base())
        });
        assert_eq!(product, Product::default());
    }

    #[test]
    fn product_url_prefers_anchor_around_title() {
        let html = r#"<article><a href="/de/cannabis/flowers/x"><h2>Name</h2></a><a href="/other">more</a></article>"#;
        let url = with_tile(html, |t| {
            let h2 = t.select(&SEL_H2).next();
            extract_product_url(t, h2, &base())
        });
        assert_eq!(url, "https://greenmedical.health/de/cannabis/flowers/x");
    }

    #[test]
    fn product_url_falls_back_to_first_anchor() {
        let html = r#"<article><h2>Name</h2><a href="/de/cannabis/flowers/y">link</a></article>"#;
        let url = with_tile(html, |t| {
            let h2 = t.select(&SEL_H2).next();
            extract_product_url(t, h2, &base())
        });
        assert_eq!(url, "https://greenmedical.health/de/cannabis/flowers/y");
    }

    /// Port of `TestProduktUrl` on the production path. Python's `urljoin`
    /// kept inner whitespace verbatim; `Url::join` yields a valid URL with
    /// `%20` instead (browsers normalise the same way). Surrounding
    /// whitespace is stripped by the URL parser, like `.strip()` before.
    #[test]
    fn product_url_is_trimmed_and_inner_whitespace_percent_encoded() {
        let html =
            r#"<article><a href="  /de/cannabis/flowers/x y?a=b c  "><h2>Name</h2></a></article>"#;
        let url = with_tile(html, |t| {
            let h2 = t.select(&SEL_H2).next();
            extract_product_url(t, h2, &base())
        });
        assert_eq!(
            url,
            "https://greenmedical.health/de/cannabis/flowers/x%20y?a=b%20c"
        );
        assert!(!url.starts_with(' ') && !url.ends_with(' '));
    }

    #[test]
    fn product_url_no_anchor_returns_empty() {
        let url = with_tile("<article><h2>Name</h2></article>", |t| {
            let h2 = t.select(&SEL_H2).next();
            extract_product_url(t, h2, &base())
        });
        assert_eq!(url, "");
    }

    #[test]
    fn pagination_reads_current_and_total() {
        let doc = Html::parse_document(r#"<div class="paginationContainer">Seite 2 / 7</div>"#);
        assert_eq!(parse_pagination(&doc), Some((2, 7)));
    }

    #[test]
    fn pagination_missing_container_returns_none() {
        let doc = Html::parse_document("<div>kein Pager</div>");
        assert_eq!(parse_pagination(&doc), None);
    }

    #[test]
    fn pagination_container_without_ratio_returns_none() {
        let doc = Html::parse_document(r#"<div class="paginationContainer">weiter</div>"#);
        assert_eq!(parse_pagination(&doc), None);
    }

    #[test]
    fn pagination_reads_nested_real_markup() {
        let doc = Html::parse_document(
            r#"<div class="paginationContainer"><a class="btn">Zurück</a><div class="mx-5">
                1 / 7
            </div><a href="?page=2">Weiter</a></div>"#,
        );
        assert_eq!(parse_pagination(&doc), Some((1, 7)));
    }

    #[test]
    fn pharmacy_uuid_is_found_in_links() {
        let html = r#"<html><body><a href="/x">nope</a>
            <a href=/de/cannabis/products?pharmacyAvailability=b4bddcc5-dc41-49d8-87df-14a03d561b32>Livebestand</a></body></html>"#;
        assert_eq!(
            parse_pharmacy_uuid(html).as_deref(),
            Some("b4bddcc5-dc41-49d8-87df-14a03d561b32")
        );
        assert_eq!(
            parse_pharmacy_uuid("<html><body><a href='/x'>x</a></body></html>"),
            None
        );
    }

    #[test]
    fn pharmacies_first_table_only_with_link_and_four_cells() {
        let html = r#"<table><tr><th>h</th></tr>
            <tr><td><a href="/de/cannabis/pharmacy/a">Apo A</a></td><td>10115</td><td>Berlin</td><td>Str. 1</td></tr>
            <tr><td>No link</td><td>1</td><td>2</td><td>3</td></tr>
            <tr><td><a href="/short">Short</a></td><td>1</td><td>2</td></tr>
            <tr><td><a href="https://other.test/b">Apo B</a></td><td>20095</td><td>Hamburg</td><td>Str.&nbsp;2</td></tr>
            </table>
            <table><tr><td><a href="/c">Apo C</a></td><td>1</td><td>2</td><td>3</td></tr></table>"#;
        let rows = parse_pharmacies(html, &base());
        assert_eq!(
            rows,
            vec![
                PharmacyRow {
                    name: "Apo A".into(),
                    url: "https://greenmedical.health/de/cannabis/pharmacy/a".into(),
                    postal_code: "10115".into(),
                    city: "Berlin".into(),
                    address: "Str. 1".into(),
                },
                PharmacyRow {
                    name: "Apo B".into(),
                    url: "https://other.test/b".into(),
                    postal_code: "20095".into(),
                    city: "Hamburg".into(),
                    address: "Str.\u{a0}2".into(),
                },
            ]
        );
    }

    #[test]
    fn flowers_page_collects_tiles_and_pagination() {
        let html = r#"<html><body>
          <article class="productGridTile"><a href="/de/cannabis/flowers/a"><h2>Sorte A</h2></a>
            <span class="productGridTilePriceAmount">9,50 €</span></article>
          <article class="productGridTile"><a href="/de/cannabis/flowers/b"><h2>Sorte B</h2></a></article>
          <div class="paginationContainer">1 / 2</div>
        </body></html>"#;
        let page = parse_flowers_page(html, &base());
        let names: Vec<_> = page.products.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["Sorte A", "Sorte B"]);
        assert_eq!(page.pagination, Some((1, 2)));
        assert_eq!(page.products[0].price_per_gram, "9,50 €");
    }
}
