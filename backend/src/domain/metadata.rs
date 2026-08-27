//! `build_metadata` and the highlight pickers.

use std::collections::HashSet;

use chrono::{DateTime, Utc};

use super::model::{
    BEST_RATED_MIN_REVIEWS, HighlightDto, MetadataDto, OfferRecord, RunDto, SOURCE_URL, StrainDto,
};

fn highlight(offer: &OfferRecord, price: Option<f64>) -> HighlightDto {
    HighlightDto {
        price,
        name: offer.name.clone(),
        apotheke: offer.apotheke.clone(),
        genetik: offer.genetik.clone(),
        thc: offer.thc.clone(),
        cbd: offer.cbd.clone(),
        produkt_url: offer.produkt_url.clone(),
        strain_id: offer.strain_id,
        pharmacy_id: offer.pharmacy_id,
        rating_value: None,
        review_count: None,
    }
}

/// Highest-rated strain with at least [`BEST_RATED_MIN_REVIEWS`] reviews
/// (ties: more reviews, then lower price). `price` is the strain's `min_price`,
/// the pharmacy is the one of its cheapest offer.
pub fn best_rated(strains: &[StrainDto]) -> Option<HighlightDto> {
    strains
        .iter()
        .filter_map(|s| {
            let rating = s.rating.as_ref()?;
            let value = rating.value?;
            (rating.count >= BEST_RATED_MIN_REVIEWS).then_some((value, rating.count, s))
        })
        .max_by(|a, b| {
            a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)).then_with(|| {
                let price = |s: &StrainDto| s.min_price.unwrap_or(f64::INFINITY);
                price(b.2).total_cmp(&price(a.2))
            })
        })
        .map(|(value, count, strain)| {
            let offer = strain.offers.first();
            HighlightDto {
                price: strain.min_price,
                name: strain.name.clone(),
                apotheke: offer.map(|o| o.apotheke.clone()).unwrap_or_default(),
                genetik: strain.genetik.clone(),
                thc: strain.thc.clone(),
                cbd: strain.cbd.clone(),
                produkt_url: offer.map(|o| o.produkt_url.clone()).unwrap_or_default(),
                strain_id: strain.id,
                pharmacy_id: offer.map(|o| o.pharmacy_id).unwrap_or_default(),
                rating_value: Some(Some(value)),
                review_count: Some(count),
            }
        })
}

/// Pick the offer with the lowest `key` value (first one wins on ties).
pub fn cheapest(
    offers: &[OfferRecord],
    key: impl Fn(&OfferRecord) -> Option<f64>,
) -> Option<HighlightDto> {
    let mut best: Option<(f64, &OfferRecord)> = None;
    for offer in offers {
        let Some(value) = key(offer) else { continue };
        match best {
            Some((current, _)) if current <= value => {}
            _ => best = Some((value, offer)),
        }
    }
    best.map(|(value, offer)| highlight(offer, Some(value)))
}

/// Pick the offer with the highest `value_fn`, breaking ties by the cheapest
/// price (unpriced offers rank as infinitely expensive). First one wins on
/// complete ties, like Python's `max`.
pub fn highest(
    offers: &[OfferRecord],
    value_fn: impl Fn(&OfferRecord) -> Option<f64>,
) -> Option<HighlightDto> {
    let rank = |offer: &OfferRecord, value: f64| {
        (value, -offer.preis_eur_pro_gramm.unwrap_or(f64::INFINITY))
    };
    let mut best: Option<((f64, f64), &OfferRecord)> = None;
    for offer in offers {
        let Some(value) = value_fn(offer) else {
            continue;
        };
        let candidate = rank(offer, value);
        let better = match best {
            None => true,
            Some((current, _)) => {
                candidate.0 > current.0 || (candidate.0 == current.0 && candidate.1 > current.1)
            }
        };
        if better {
            best = Some((candidate, offer));
        }
    }
    best.map(|(_, offer)| highlight(offer, offer.preis_eur_pro_gramm))
}

/// Reward strains high in THC *and* CBD via the product of the two values.
fn combined_cannabinoids(offer: &OfferRecord) -> Option<f64> {
    Some(offer.thc_value? * offer.cbd_value?)
}

/// Build the metadata document for a run.
pub fn build_metadata(
    offers: &[OfferRecord],
    strains: &[StrainDto],
    generated_at: DateTime<Utc>,
    run: RunDto,
) -> MetadataDto {
    let pharmacies: HashSet<&str> = offers
        .iter()
        .filter(|o| !o.apotheke.is_empty())
        .map(|o| o.apotheke.as_str())
        .collect();
    let cheapest_gram = cheapest(offers, |o| o.preis_eur_pro_gramm);

    MetadataDto {
        generated_at,
        source: SOURCE_URL.to_owned(),
        total: offers.len() as i64,
        pharmacy_count: pharmacies.len() as i64,
        strain_count: strains.len() as i64,
        lowest_price: cheapest_gram.as_ref().and_then(|h| h.price),
        cheapest_gram,
        cheapest_thc_gram: cheapest(offers, |o| o.preis_eur_pro_gramm_thc),
        cheapest_cbd_gram: cheapest(offers, |o| o.preis_eur_pro_gramm_cbd),
        highest_thc: highest(offers, |o| o.thc_value),
        highest_cbd: highest(offers, |o| o.cbd_value),
        highest_thc_cbd: highest(offers, combined_cannabinoids),
        best_rated: best_rated(strains),
        run,
        next_run_at: None,
        scrape_running: false,
        schedule: None,
        email_enabled: false,
    }
}

#[cfg(test)]
pub(crate) fn test_run() -> RunDto {
    use super::model::{RunStatus, RunTrigger};
    RunDto {
        id: 1,
        started_at: DateTime::parse_from_rfc3339("2026-08-27T08:00:00Z")
            .unwrap()
            .with_timezone(&Utc),
        finished_at: Some(
            DateTime::parse_from_rfc3339("2026-08-27T08:00:03Z")
                .unwrap()
                .with_timezone(&Utc),
        ),
        status: RunStatus::Success,
        trigger: RunTrigger::Manual,
        instance: Some("test".into()),
        pharmacies_total: Some(1),
        pharmacies_scraped: Some(1),
        pharmacies_failed: Some(0),
        offer_count: Some(1),
        http_requests: Some(3),
        error: None,
        reviews_scraped: None,
        reviews_failed: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::group::group_by_strain;
    use crate::domain::group::test_support::offers_from_csv;
    use std::collections::BTreeSet;

    fn metadata_for(rows: &str) -> MetadataDto {
        let offers = offers_from_csv(rows);
        let strains = group_by_strain(&offers);
        build_metadata(
            &offers,
            &strains,
            test_run().finished_at.unwrap(),
            test_run(),
        )
    }

    fn highlights() -> MetadataDto {
        metadata_for(
            r#"Apo A,10115,Berlin,Sorte X,EMK,Indica,20%,2%,"10,00 €",verfügbar
Apo B,20095,Hamburg,Sorte Y,XYZ,Sativa,30%,1%,"9,00 €",neu
Apo C,50667,Köln,Sorte Z,ABC,Hybrid,15%,8%,"6,00 €",verfügbar"#,
        )
    }

    #[test]
    fn metadata_counts_offers_and_strains() {
        let metadata = metadata_for(
            r#"Apo A,10115,Berlin,Sorte X,EMK,Indica,20%,1%,"9,50 €",verfügbar
Apo B,20095,Hamburg,Sorte X,EMK,Indica,20%,1%,"8,00 €",neu"#,
        );
        assert_eq!(metadata.total, 2);
        assert_eq!(metadata.strain_count, 1);
        assert_eq!(metadata.pharmacy_count, 2);
        assert_eq!(metadata.lowest_price, Some(8.0));
    }

    #[test]
    fn cheapest_per_gram_carries_name_and_pharmacy() {
        let entry = highlights().cheapest_gram.unwrap();
        assert_eq!(
            (entry.price, entry.name.as_str(), entry.apotheke.as_str()),
            (Some(6.0), "Sorte Z", "Apo C")
        );
    }

    #[test]
    fn cheapest_per_gram_thc() {
        // 9.00 / 0.30 = 30.00 €/g THC is cheapest
        let entry = highlights().cheapest_thc_gram.unwrap();
        assert_eq!(
            (entry.price, entry.name.as_str(), entry.apotheke.as_str()),
            (Some(30.0), "Sorte Y", "Apo B")
        );
    }

    #[test]
    fn cheapest_per_gram_cbd() {
        // 6.00 / 0.08 = 75.00 €/g CBD is cheapest
        let entry = highlights().cheapest_cbd_gram.unwrap();
        assert_eq!(
            (entry.price, entry.name.as_str(), entry.apotheke.as_str()),
            (Some(75.0), "Sorte Z", "Apo C")
        );
    }

    #[test]
    fn highest_thc() {
        let entry = highlights().highest_thc.unwrap();
        assert_eq!(
            (
                entry.name.as_str(),
                entry.apotheke.as_str(),
                entry.thc.as_str()
            ),
            ("Sorte Y", "Apo B", "30%")
        );
    }

    #[test]
    fn highest_cbd() {
        let entry = highlights().highest_cbd.unwrap();
        assert_eq!(
            (
                entry.name.as_str(),
                entry.apotheke.as_str(),
                entry.cbd.as_str()
            ),
            ("Sorte Z", "Apo C", "8%")
        );
    }

    #[test]
    fn highest_thc_and_cbd_combined() {
        // product thc*cbd: X=40, Y=30, Z=120 -> Sorte Z (balanced high) wins
        let entry = highlights().highest_thc_cbd.unwrap();
        assert_eq!(
            (entry.name.as_str(), entry.thc.as_str(), entry.cbd.as_str()),
            ("Sorte Z", "15%", "8%")
        );
    }

    #[test]
    fn highest_breaks_ties_by_cheapest_price() {
        let metadata = metadata_for(
            r#"Apo A,1,B,Sorte X,EMK,Indica,20%,1%,"10,00 €",neu
Apo B,2,C,Sorte Y,XYZ,Indica,20%,1%,"7,00 €",neu
Apo C,3,D,Sorte Z,ABC,Indica,20%,1%,kein Preis,neu"#,
        );
        let entry = metadata.highest_thc.unwrap();
        assert_eq!((entry.name.as_str(), entry.price), ("Sorte Y", Some(7.0)));
    }

    #[test]
    fn empty_offers_yield_null_highlights() {
        let metadata = metadata_for("");
        assert_eq!(metadata.total, 0);
        assert!(metadata.cheapest_gram.is_none());
        assert!(metadata.highest_thc_cbd.is_none());
        assert_eq!(metadata.lowest_price, None);
    }

    #[test]
    fn url_flows_into_highlights() {
        let metadata = metadata_for(
            r#"Apo A,10115,Berlin,Sorte X,EMK,Indica,20%,1%,"9,50 €",verfügbar,https://greenmedical.health/p?deliveryTarget=T"#,
        );
        assert!(
            metadata
                .cheapest_gram
                .unwrap()
                .produkt_url
                .ends_with("deliveryTarget=T")
        );
    }

    fn rated(strain: &mut StrainDto, value: Option<f64>, count: i32) {
        strain.rating = Some(crate::domain::RatingDto {
            value,
            count,
            scraped_at: test_run().started_at,
        });
        strain.sort.rating = value;
    }

    #[test]
    fn best_rated_requires_five_reviews_and_breaks_ties_by_count() {
        let offers = offers_from_csv(
            r#"Apo A,1,B,Sorte X,EMK,Indica,20%,1%,"10,00 €",neu
Apo B,2,C,Sorte X,EMK,Indica,20%,1%,"7,00 €",neu
Apo C,3,D,Sorte Y,XYZ,Indica,20%,1%,"5,00 €",neu
Apo D,4,E,Sorte Z,ABC,Indica,20%,1%,"6,00 €",neu"#,
        );
        let mut strains = group_by_strain(&offers);
        assert!(best_rated(&strains).is_none(), "no ratings at all");
        // X: 4.5 with 5 reviews, Y: 5.0 but only 4 reviews, Z: 4.5 with 10 reviews.
        rated(&mut strains[0], Some(4.5), 5);
        rated(&mut strains[1], Some(5.0), 4);
        rated(&mut strains[2], Some(4.5), 10);
        let best = best_rated(&strains).unwrap();
        assert_eq!(best.name, "Sorte Z");
        assert_eq!(best.rating_value, Some(Some(4.5)));
        assert_eq!(best.review_count, Some(10));
        // Z gets the threshold count only: X (5 reviews) is the tie winner now.
        rated(&mut strains[2], Some(4.5), 5);
        rated(&mut strains[0], Some(4.5), 6);
        let best = best_rated(&strains).unwrap();
        assert_eq!(best.name, "Sorte X");
        assert_eq!(best.price, Some(7.0), "price = strain min_price");
        assert_eq!(best.apotheke, "Apo B", "pharmacy of the cheapest offer");
        assert_eq!(best.pharmacy_id, strains[0].offers[0].pharmacy_id);
        // A rating without value (0 reviews) never qualifies.
        rated(&mut strains[0], None, 0);
        rated(&mut strains[2], None, 9);
        assert!(best_rated(&strains).is_none());
        let json = serde_json::to_value(build_metadata(
            &offers,
            &strains,
            test_run().started_at,
            test_run(),
        ))
        .unwrap();
        assert_eq!(json["best_rated"], serde_json::Value::Null);
    }

    #[test]
    fn metadata_shape_matches_contract() {
        let metadata =
            metadata_for(r#"Apo A,10115,Berlin,Sorte X,EMK,Indica,20%,1%,"9,50 €",verfügbar"#);
        let value = serde_json::to_value(&metadata).unwrap();
        let keys: BTreeSet<_> = value.as_object().unwrap().keys().cloned().collect();
        let expected: BTreeSet<String> = [
            "generated_at",
            "source",
            "total",
            "pharmacy_count",
            "strain_count",
            "lowest_price",
            "cheapest_gram",
            "cheapest_thc_gram",
            "cheapest_cbd_gram",
            "highest_thc",
            "highest_cbd",
            "highest_thc_cbd",
            "best_rated",
            "run",
            "next_run_at",
            "scrape_running",
            "schedule",
            "email_enabled",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        assert_eq!(keys, expected);
        let highlight_keys: BTreeSet<_> = value["cheapest_gram"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        let expected_highlight: BTreeSet<String> = [
            "price",
            "name",
            "apotheke",
            "genetik",
            "thc",
            "cbd",
            "produkt_url",
            "strain_id",
            "pharmacy_id",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        assert_eq!(highlight_keys, expected_highlight);
        assert_eq!(value["generated_at"], "2026-08-27T08:00:03Z");
    }
}
