//! HTTP client retry behaviour and pagination against wiremock.

mod support;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use greenmedical_backend::scrape::client::{HttpErrorKind, ScrapeClient};
use greenmedical_backend::scrape::parse::PharmacyRow;
use greenmedical_backend::scrape::scrape_flowers_for_pharmacy;
use support::{
    GENERIC_STRAIN, MockPharmacy, MockSite, MockTile, SESSION_COOKIE, flowers_page_html,
    test_config,
};
use url::Url;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

/// Responds with a fixed sequence of templates, repeating the last one.
struct Sequence {
    templates: Vec<ResponseTemplate>,
    calls: AtomicUsize,
}

impl Sequence {
    fn new(templates: Vec<ResponseTemplate>) -> Self {
        Self {
            templates,
            calls: AtomicUsize::new(0),
        }
    }
}

impl Respond for Sequence {
    fn respond(&self, _request: &Request) -> ResponseTemplate {
        let index = self.calls.fetch_add(1, Ordering::SeqCst);
        self.templates[index.min(self.templates.len() - 1)].clone()
    }
}

async fn client_for(server: &MockServer, retry_total: u32) -> ScrapeClient {
    let mut config = test_config(&server.uri());
    config.scrape_retry_total = retry_total;
    ScrapeClient::new(&config).unwrap()
}

fn url(server: &MockServer, path: &str) -> Url {
    Url::parse(&format!("{}{path}", server.uri())).unwrap()
}

#[tokio::test]
async fn retries_503_twice_then_succeeds() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/page"))
        .respond_with(Sequence::new(vec![
            ResponseTemplate::new(503),
            ResponseTemplate::new(503),
            ResponseTemplate::new(200).set_body_string("ok"),
        ]))
        .expect(3)
        .mount(&server)
        .await;
    let fetched = client_for(&server, 4)
        .await
        .get_text(url(&server, "/page"))
        .await
        .unwrap();
    assert_eq!(fetched.body, "ok");
    assert_eq!(fetched.attempts, 3);
}

#[tokio::test]
async fn honours_retry_after_on_429() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/limited"))
        .respond_with(Sequence::new(vec![
            ResponseTemplate::new(429).insert_header("Retry-After", "1"),
            ResponseTemplate::new(200).set_body_string("after"),
        ]))
        .expect(2)
        .mount(&server)
        .await;
    let started = Instant::now();
    let fetched = client_for(&server, 4)
        .await
        .get_text(url(&server, "/limited"))
        .await
        .unwrap();
    assert_eq!(fetched.body, "after");
    assert_eq!(fetched.attempts, 2);
    assert!(
        started.elapsed() >= Duration::from_millis(950),
        "{:?}",
        started.elapsed()
    );
}

#[tokio::test]
async fn gives_up_after_retry_total_is_exhausted() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/down"))
        .respond_with(ResponseTemplate::new(503))
        .expect(5)
        .mount(&server)
        .await;
    let err = client_for(&server, 4)
        .await
        .get_text(url(&server, "/down"))
        .await
        .unwrap_err();
    assert_eq!(err.attempts, 5);
    assert!(
        matches!(err.kind, HttpErrorKind::Status(s) if s.as_u16() == 503),
        "{err}"
    );
}

#[tokio::test]
async fn does_not_retry_404() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/missing"))
        .respond_with(ResponseTemplate::new(404))
        .expect(1)
        .mount(&server)
        .await;
    let err = client_for(&server, 4)
        .await
        .get_text(url(&server, "/missing"))
        .await
        .unwrap_err();
    assert_eq!(err.attempts, 1);
    assert!(matches!(err.kind, HttpErrorKind::Status(s) if s.as_u16() == 404));
}

fn pharmacy() -> PharmacyRow {
    PharmacyRow {
        name: "Adler Apotheke".into(),
        url: "https://example.test/adler".into(),
        postal_code: "10115".into(),
        city: "Berlin".into(),
        address: "Str. 1".into(),
    }
}

#[tokio::test]
async fn walks_all_pages_and_injects_delivery_target() {
    let server = MockServer::start().await;
    let page1 = flowers_page_html(&[MockTile::new("Sorte A", "A1", "9,50 €/g")], Some((1, 2)));
    let page2 = flowers_page_html(&[MockTile::new("Sorte B", "B1", "8,00 €/g")], Some((2, 2)));
    for (n, body) in [(1, page1), (2, page2)] {
        Mock::given(method("GET"))
            .and(path("/de/cannabis/flowers"))
            .and(query_param("deliveryTarget", "TOKEN"))
            .and(query_param("onlyShowIfAvailable", "1"))
            .and(query_param("page", n.to_string()))
            .respond_with(ResponseTemplate::new(200).set_body_string(body))
            .expect(1)
            .mount(&server)
            .await;
    }
    let config = test_config(&server.uri());
    let client = ScrapeClient::new(&config).unwrap();
    let mut requests = 0;
    let offers = scrape_flowers_for_pharmacy(
        &client,
        &config,
        &pharmacy(),
        "uuid",
        "TOKEN",
        &mut requests,
    )
    .await
    .unwrap();
    assert_eq!(requests, 2);
    let names: Vec<_> = offers.iter().map(|o| o.product.name.as_str()).collect();
    assert_eq!(names, ["Sorte A", "Sorte B"]);
    for offer in &offers {
        assert_eq!(offer.pharmacy.name, "Adler Apotheke");
        assert!(
            offer.product.product_url.contains("deliveryTarget=TOKEN"),
            "{}",
            offer.product.product_url
        );
        assert!(
            !offer.product.product_url.contains('#'),
            "fragment stripped"
        );
    }
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}

/// The live site: `?deliveryTarget=…&page=N` → `302 Location: ?page=N` +
/// `Set-Cookie: PHPSESSID`; the redirected request only yields the pharmacy's
/// tiles when it carries that cookie. Without a cookie jar the client would
/// walk the generic catalogue instead.
#[tokio::test]
async fn follows_session_redirect_with_the_pharmacy_cookie() {
    let site = MockSite::start(vec![MockPharmacy::new(
        "Adler Apotheke",
        "b4bddcc5-dc41-49d8-87df-14a03d561b32",
        vec![
            vec![MockTile::new("Sorte A", "A1", "9,50 €/g")],
            vec![MockTile::new("Sorte B", "B1", "8,00 €/g")],
        ],
    )])
    .await;
    let pharmacy = &site.pharmacies[0];
    let config = test_config(&site.base_url());
    let client = ScrapeClient::new(&config).unwrap();
    let mut requests = 0;
    let offers = scrape_flowers_for_pharmacy(
        &client,
        &config,
        &PharmacyRow {
            name: pharmacy.name.clone(),
            url: format!("{}/de/cannabis/pharmacy/{}", site.base_url(), pharmacy.slug),
            postal_code: pharmacy.postal_code.clone(),
            city: pharmacy.city.clone(),
            address: pharmacy.address.clone(),
        },
        pharmacy.uuid.as_deref().unwrap(),
        &pharmacy.delivery_target(),
        &mut requests,
    )
    .await
    .unwrap();
    // Redirects are followed inside one attempt: still one attempt per page.
    assert_eq!(requests, 2);
    let names: Vec<_> = offers.iter().map(|o| o.product.name.as_str()).collect();
    assert_eq!(names, ["Sorte A", "Sorte B"]);
    assert!(offers.iter().all(|o| o.product.name != GENERIC_STRAIN));

    let received = site.requests().await;
    // 2 selects (302) + 2 redirected page fetches.
    assert_eq!(received.len(), 4);
    let redirected: Vec<_> = received
        .iter()
        .filter(|r| !r.url.query_pairs().any(|(k, _)| k == "deliveryTarget"))
        .collect();
    assert_eq!(redirected.len(), 2);
    for request in redirected {
        assert_eq!(
            request.headers.get("cookie").unwrap().to_str().unwrap(),
            format!("{SESSION_COOKIE}={}", pharmacy.session())
        );
    }
}

#[tokio::test]
async fn stops_when_a_page_has_no_tiles_even_if_pagination_says_more() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/de/cannabis/flowers"))
        .and(query_param("page", "1"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(flowers_page_html(
                &[MockTile::new("Sorte A", "A1", "9,50 €/g")],
                Some((1, 3)),
            )),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/de/cannabis/flowers"))
        .and(query_param("page", "2"))
        .respond_with(ResponseTemplate::new(200).set_body_string("<html><body>leer</body></html>"))
        .mount(&server)
        .await;
    let config = test_config(&server.uri());
    let client = ScrapeClient::new(&config).unwrap();
    let mut requests = 0;
    let offers =
        scrape_flowers_for_pharmacy(&client, &config, &pharmacy(), "uuid", "T", &mut requests)
            .await
            .unwrap();
    assert_eq!(offers.len(), 1);
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}

#[tokio::test]
async fn stops_on_first_page_without_pagination() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/de/cannabis/flowers"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(flowers_page_html(
                &[MockTile::new("Sorte A", "A1", "9,50 €/g")],
                None,
            )),
        )
        .expect(1)
        .mount(&server)
        .await;
    let config = test_config(&server.uri());
    let client = ScrapeClient::new(&config).unwrap();
    let mut requests = 0;
    let offers =
        scrape_flowers_for_pharmacy(&client, &config, &pharmacy(), "uuid", "T", &mut requests)
            .await
            .unwrap();
    assert_eq!(offers.len(), 1);
    assert_eq!(requests, 1);
}
