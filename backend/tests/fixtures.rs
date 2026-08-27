//! Parsing of the HTML fixtures captured from the real site.

use greenmedical_backend::scrape::parse::{
    PharmacyRow, parse_flowers_page, parse_pharmacies, parse_pharmacy_uuid,
};
use pretty_assertions::assert_eq;
use url::Url;

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/tests/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("fixture exists")
}

fn base() -> Url {
    Url::parse("https://greenmedical.health").unwrap()
}

#[test]
fn pharmacies_fixture_uses_first_table_and_skips_invalid_rows() {
    let rows = parse_pharmacies(&fixture("pharmacies.html"), &base());
    assert_eq!(
        rows,
        vec![
            PharmacyRow {
                name: "Grüne Blüte".into(),
                url: "https://greenmedical.health/de/cannabis/pharmacy/gruene-bluete".into(),
                postal_code: "04416".into(),
                city: "Markkleeberg".into(),
                address: "Magdeborner Str. 14".into(),
            },
            PharmacyRow {
                name: "Asavita".into(),
                url: "https://greenmedical.health/de/cannabis/pharmacy/asavita".into(),
                postal_code: "10365".into(),
                city: "Berlin".into(),
                address: "Frankfurter\u{a0}Allee 241".into(),
            },
            PharmacyRow {
                name: "Sky Cannabis Kudamm".into(),
                url: "https://greenmedical.health/de/cannabis/pharmacy/sky_cannabis_kudamm".into(),
                postal_code: "10711".into(),
                city: "Berlin".into(),
                address: "Kurfürstendamm 139".into(),
            },
        ]
    );
}

#[test]
fn pharmacy_detail_fixture_yields_uuid() {
    assert_eq!(
        parse_pharmacy_uuid(&fixture("pharmacy_detail.html")).as_deref(),
        Some("b4bddcc5-dc41-49d8-87df-14a03d561b32")
    );
}

#[test]
fn flowers_page1_fixture_yields_two_tiles_and_pagination() {
    let page = parse_flowers_page(&fixture("flowers_page1.html"), &base());
    assert_eq!(page.pagination, Some((1, 2)));
    assert_eq!(page.products.len(), 2);
    let first = &page.products[0];
    assert_eq!(first.name, "Bunatic");
    assert_eq!(first.designation, "Luana 27/1 Donny B");
    assert_eq!(first.genetics, "Indica");
    assert_eq!(first.thc, "27%");
    assert_eq!(first.cbd, "1%");
    // The live page uses a non-breaking space; clean_text() normalises it on persist.
    assert_eq!(first.price_per_gram, "5,49\u{a0}€/g");
    assert_eq!(first.availability, "Auf Lager");
    // The title is not wrapped in an anchor on the real site, so the first
    // anchor of the tile (the review link) wins, like in the old CSV.
    assert_eq!(
        first.product_url,
        "https://greenmedical.health/de/cannabis/flower/luana_27_1_donny_b-bunatic#reviews"
    );
    let second = &page.products[1];
    assert_eq!(second.name, "OG Kush");
    assert_eq!(second.designation, "Cannamedical CM 24/1");
    assert_eq!(second.genetics, "Hybrid Sativa Dominant");
    assert_eq!(second.price_per_gram, "6,49\u{a0}€/g");
}

#[test]
fn flowers_page2_fixture_is_last_page() {
    let page = parse_flowers_page(&fixture("flowers_page2.html"), &base());
    assert_eq!(page.pagination, Some((2, 2)));
    assert_eq!(page.products.len(), 1);
    let product = &page.products[0];
    assert_eq!(product.name, "Electric Honeydew (EHD)");
    assert_eq!(product.designation, "Pedanios 26/1 EHD-CA");
    assert_eq!(product.genetics, "Sativa");
    assert_eq!(product.thc, "26%");
    assert_eq!(product.cbd, "<1%");
    assert_eq!(product.price_per_gram, "6,89\u{a0}€/g");
}

#[test]
fn empty_flowers_fixture_has_no_tiles() {
    let page = parse_flowers_page(&fixture("flowers_empty.html"), &base());
    assert!(page.products.is_empty());
    assert_eq!(page.pagination, None);
}
