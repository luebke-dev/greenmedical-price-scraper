//! Pure parsing of a product page's review section (`docs/api-contract.md`,
//! "Erweiterung: Bewertungen").
//!
//! The aggregate rating is taken from the JSON-LD `aggregateRating` block
//! when present and otherwise from the header spans
//! (`.pdpReviewsHeaderRating .ratingStars span`). Individual reviews are read
//! from every `div.pdpReview`; all of them are on one page (no pagination).

use std::sync::LazyLock;

use chrono::NaiveDate;
use regex::Regex;
use scraper::{ElementRef, Html, Selector};
use sha2::{Digest, Sha256};

use crate::domain::clean_text;

fn sel(css: &str) -> Selector {
    Selector::parse(css).expect("static selector")
}

static SEL_JSON_LD: LazyLock<Selector> =
    LazyLock::new(|| sel(r#"script[type="application/ld+json"]"#));
static SEL_HEADER_SPANS: LazyLock<Selector> =
    LazyLock::new(|| sel(".pdpReviewsHeaderRating .ratingStars span"));
static SEL_MODAL_URL: LazyLock<Selector> = LazyLock::new(|| sel("[data-modal-url]"));
static SEL_REVIEW: LazyLock<Selector> = LazyLock::new(|| sel("div.pdpReview"));
static SEL_REVIEW_NAME: LazyLock<Selector> = LazyLock::new(|| sel(".pdpReviewName span"));
static SEL_REVIEW_STARS: LazyLock<Selector> =
    LazyLock::new(|| sel(".pdpReviewRating .ratingStars i"));
static SEL_REVIEW_DATE: LazyLock<Selector> = LazyLock::new(|| sel(".pdpReviewDate"));
static SEL_REVIEW_CONTENT: LazyLock<Selector> = LazyLock::new(|| sel(".pdpReviewContent p"));

static UUID_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"/de/cannabis/feedback/modal/([0-9a-fA-F-]{36})").expect("static regex")
});
static COUNT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\((\d+)\)").expect("static regex"));

/// Text on the badge that marks a verified purchase.
pub const VERIFIED_BADGE: &str = "Verifizierter Kauf";

/// One review as found on the page.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedReview {
    pub author: String,
    pub reviewed_on: Option<NaiveDate>,
    /// 0.0–5.0 in 0.5 steps (full stars + half stars).
    pub rating: f64,
    pub verified: bool,
    pub content: String,
}

impl ParsedReview {
    /// Hex SHA-256 of `author|reviewed_on|rating|content` (`reviewed_on` as
    /// ISO date or empty, `rating` with one decimal).
    pub fn fingerprint(&self) -> String {
        fingerprint(
            &self.author,
            self.reviewed_on.as_ref(),
            self.rating,
            &self.content,
        )
    }
}

/// See [`ParsedReview::fingerprint`].
pub fn fingerprint(
    author: &str,
    reviewed_on: Option<&NaiveDate>,
    rating: f64,
    content: &str,
) -> String {
    let date = reviewed_on.map(|d| d.to_string()).unwrap_or_default();
    let payload = format!("{author}|{date}|{rating:.1}|{content}");
    let digest = Sha256::digest(payload.as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// Everything the review scrape extracts from one product page.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ProductReviews {
    pub product_uuid: Option<String>,
    /// `None` when the page has no rating (0 reviews).
    pub rating_value: Option<f64>,
    pub review_count: u32,
    pub reviews: Vec<ParsedReview>,
}

/// `(ratingValue, reviewCount)` from the JSON-LD `aggregateRating`, if any.
fn json_ld_aggregate(document: &Html) -> Option<(Option<f64>, u32)> {
    for script in document.select(&SEL_JSON_LD) {
        let text: String = script.text().collect();
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        if let Some(found) = find_aggregate(&value) {
            return Some(found);
        }
    }
    None
}

fn number(value: &serde_json::Value) -> Option<f64> {
    match value {
        serde_json::Value::Number(n) => n.as_f64(),
        serde_json::Value::String(s) => s.trim().replace(',', ".").parse().ok(),
        _ => None,
    }
}

fn find_aggregate(value: &serde_json::Value) -> Option<(Option<f64>, u32)> {
    match value {
        serde_json::Value::Object(map) => {
            if let Some(agg) = map.get("aggregateRating")
                && let Some(count) = agg.get("reviewCount").and_then(number)
            {
                let rating = agg.get("ratingValue").and_then(number);
                return Some((rating, count.max(0.0) as u32));
            }
            map.values().find_map(find_aggregate)
        }
        serde_json::Value::Array(items) => items.iter().find_map(find_aggregate),
        _ => None,
    }
}

/// `(rating, count)` from the header spans `<span>4.3</span> <span>(124)</span>`.
fn header_aggregate(document: &Html) -> (Option<f64>, u32) {
    let mut rating = None;
    let mut count = 0;
    for span in document.select(&SEL_HEADER_SPANS) {
        let text = clean_text(Some(&span.text().collect::<String>()));
        if let Some(caps) = COUNT_RE.captures(&text) {
            count = caps[1].parse().unwrap_or(0);
        } else if let Ok(value) = text.replace(',', ".").parse::<f64>() {
            rating = Some(value);
        }
    }
    (rating, count)
}

fn product_uuid(document: &Html) -> Option<String> {
    document.select(&SEL_MODAL_URL).find_map(|el| {
        el.value()
            .attr("data-modal-url")
            .and_then(|url| UUID_RE.captures(url))
            .map(|caps| caps[1].to_lowercase())
    })
}

/// Full stars count 1, half stars 0.5.
fn star_rating(review: ElementRef<'_>) -> f64 {
    review
        .select(&SEL_REVIEW_STARS)
        .map(|star| {
            let classes = star.value().attr("class").unwrap_or_default();
            let has = |name: &str| classes.split_ascii_whitespace().any(|c| c == name);
            if has("fullStar") {
                1.0
            } else if has("halfStar") {
                0.5
            } else {
                0.0
            }
        })
        .sum()
}

fn parse_review(review: ElementRef<'_>) -> ParsedReview {
    let author = review
        .select(&SEL_REVIEW_NAME)
        .next()
        .map(|el| clean_text(Some(&el.text().collect::<String>())))
        .unwrap_or_default();
    let reviewed_on = review.select(&SEL_REVIEW_DATE).next().and_then(|el| {
        let text = clean_text(Some(&el.text().collect::<String>()));
        NaiveDate::parse_from_str(&text, "%d.%m.%Y").ok()
    });
    let content = review
        .select(&SEL_REVIEW_CONTENT)
        .map(|p| clean_text(Some(&p.text().collect::<String>())))
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let verified = review.text().any(|t| t.contains(VERIFIED_BADGE));
    ParsedReview {
        author,
        reviewed_on,
        rating: star_rating(review),
        verified,
        content,
    }
}

/// Parse the review section of a product page.
pub fn parse_product_reviews(html: &str) -> ProductReviews {
    let document = Html::parse_document(html);
    let reviews: Vec<ParsedReview> = document.select(&SEL_REVIEW).map(parse_review).collect();
    let (rating_value, review_count) = match json_ld_aggregate(&document) {
        Some(found) => found,
        None => header_aggregate(&document),
    };
    let rating_value = if review_count == 0 {
        None
    } else {
        rating_value
    };
    ProductReviews {
        product_uuid: product_uuid(&document),
        rating_value,
        review_count,
        reviews,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn fixture(name: &str) -> String {
        std::fs::read_to_string(format!(
            "{}/tests/fixtures/{name}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("fixture exists")
    }

    fn review_html(stars: &str, date: &str, content: &str, verified: bool) -> String {
        let badge = if verified {
            r#"<div class="small"><i class="fa-solid fa-badge-check"></i> Verifizierter Kauf</div>"#
        } else {
            ""
        };
        let date_html = if date.is_empty() {
            String::new()
        } else {
            format!(r#"<div class="pdpReviewDate"> {date} </div>"#)
        };
        format!(
            r#"<div class="pdpReview"><div><div class="pdpReviewHeader"><div class="pdpReviewAuthor">
            <div class="pdpReviewName"><div><span class="">Max&nbsp;M.</span></div>
            <div class="pdpReviewRating"><div class="ratingStars">{stars}</div></div></div></div>{date_html}</div></div>
            <div class="pdpReviewContent"><p class="">  {content} </p>{badge}</div></div>"#
        )
    }

    #[test]
    fn fixture_with_reviews_prefers_json_ld_aggregate() {
        // The fixture is trimmed to 5 review blocks but keeps the JSON-LD block
        // (124 reviews) and the header; the aggregate count must come from there.
        let parsed = parse_product_reviews(&fixture("product_with_reviews.html"));
        assert_eq!(
            parsed.product_uuid.as_deref(),
            Some("c822b844-1925-11ef-b5f0-0242ac170003")
        );
        assert_eq!(parsed.rating_value, Some(4.3));
        assert_eq!(parsed.review_count, 124);
        assert_eq!(parsed.reviews.len(), 5);

        let first = &parsed.reviews[0];
        assert_eq!(first.author, "Carlos S.");
        assert_eq!(
            first.reviewed_on,
            Some(NaiveDate::from_ymd_opt(2026, 8, 25).unwrap())
        );
        assert_eq!(first.rating, 4.0);
        assert!(first.verified);
        assert_eq!(first.content, "Bom material");

        // Third review has no text at all.
        let third = &parsed.reviews[2];
        assert_eq!(third.author, "Ivan Z.");
        assert_eq!(third.rating, 5.0);
        assert_eq!(third.content, "");
        assert!(third.verified);
    }

    #[test]
    fn fixture_without_reviews_yields_no_rating() {
        let parsed = parse_product_reviews(&fixture("product_without_reviews.html"));
        assert_eq!(
            parsed.product_uuid.as_deref(),
            Some("f1de4982-e28e-4af4-b3c1-1e4107421385")
        );
        assert_eq!(parsed.rating_value, None);
        assert_eq!(parsed.review_count, 0);
        assert!(parsed.reviews.is_empty());
    }

    #[test]
    fn header_spans_are_the_fallback_without_json_ld() {
        let html = format!(
            r#"<html><body><div class="pdpReviewsHeaderRating"><div class="ratingStars">
            <i class="fullStar"></i><span>3,5</span> <span>(7)</span></div></div>
            <button data-modal-url="/de/cannabis/feedback/modal/AABBCCDD-1925-11EF-B5F0-0242AC170003"></button>
            {}</body></html>"#,
            review_html(
                r#"<i class="fa-solid fa-star fullStar"></i><i class="fa-solid fa-star-half-stroke halfStar"></i><i class="fa-regular fa-star emptyStar"></i>"#,
                "01.02.2026",
                "Halbe   Sterne\u{a0}zählen",
                false
            )
        );
        let parsed = parse_product_reviews(&html);
        assert_eq!(parsed.rating_value, Some(3.5));
        assert_eq!(parsed.review_count, 7);
        assert_eq!(
            parsed.product_uuid.as_deref(),
            Some("aabbccdd-1925-11ef-b5f0-0242ac170003")
        );
        let review = &parsed.reviews[0];
        assert_eq!(review.rating, 1.5);
        assert_eq!(review.author, "Max M.");
        assert_eq!(review.content, "Halbe Sterne zählen");
        assert!(!review.verified);
        assert_eq!(
            review.reviewed_on,
            Some(NaiveDate::from_ymd_opt(2026, 2, 1).unwrap())
        );
    }

    #[test]
    fn missing_or_invalid_date_is_none_and_zero_count_clears_rating() {
        let html = format!(
            r#"<html><body><div class="pdpReviewsHeaderRating"><div class="ratingStars"><span>4.0</span></div></div>{}{}</body></html>"#,
            review_html(r#"<i class="fullStar"></i>"#, "", "ohne Datum", true),
            review_html(
                r#"<i class="fullStar"></i>"#,
                "gestern",
                "kaputtes Datum",
                false
            ),
        );
        let parsed = parse_product_reviews(&html);
        assert_eq!(parsed.review_count, 0);
        assert_eq!(parsed.rating_value, None, "0 reviews means no rating");
        assert_eq!(parsed.product_uuid, None);
        assert_eq!(parsed.reviews.len(), 2);
        assert_eq!(parsed.reviews[0].reviewed_on, None);
        assert!(parsed.reviews[0].verified);
        assert_eq!(parsed.reviews[1].reviewed_on, None);
        assert_eq!(parsed.reviews[1].rating, 1.0);
    }

    #[test]
    fn fingerprint_is_sha256_of_the_pipe_joined_fields() {
        let review = ParsedReview {
            author: "A".into(),
            reviewed_on: Some(NaiveDate::from_ymd_opt(2026, 8, 25).unwrap()),
            rating: 4.5,
            verified: true,
            content: "text".into(),
        };
        let expected: String = Sha256::digest(b"A|2026-08-25|4.5|text")
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(review.fingerprint(), expected);
        assert_eq!(review.fingerprint().len(), 64);
        let undated = ParsedReview {
            reviewed_on: None,
            ..review.clone()
        };
        assert_ne!(undated.fingerprint(), review.fingerprint());
        assert_eq!(
            undated.fingerprint(),
            fingerprint("A", None, 4.5, "text"),
            "verified flag is not part of the fingerprint"
        );
    }

    #[test]
    fn empty_document() {
        let parsed = parse_product_reviews("<html><body></body></html>");
        assert_eq!(parsed, ProductReviews::default());
    }
}
