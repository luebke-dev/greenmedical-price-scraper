//! Offer history rows per pharmacy: flat (`mode=all`) and phases (`mode=changes`).
//!
//! `phases` is a 1:1 port of `offerHistoryPhases` in `frontend/src/lib/history.ts`:
//! the set of buckets is every bucket in which the strain had any offer; per
//! pharmacy, consecutive buckets with the same price + availability form one
//! phase, a bucket without an offer of that pharmacy starts a `delisted` phase,
//! and buckets before the pharmacy first listed the strain are ignored.

use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap};

use super::collate;
use super::model::{OfferHistoryRowDto, OfferPhaseRowDto, PharmacySeriesDto};

/// All buckets (sorted ascending by `at`) in which the strain had any offer.
fn buckets(series: &[PharmacySeriesDto]) -> BTreeSet<String> {
    series
        .iter()
        .flat_map(|s| s.points.iter().map(|p| p.at.clone()))
        .collect()
}

fn by_pharmacy(a: &str, b: &str) -> Ordering {
    collate::compare_str(a, b)
}

/// One row per (bucket, pharmacy) with an offer; `at` desc, pharmacy asc.
pub fn all_rows(series: &[PharmacySeriesDto]) -> Vec<OfferHistoryRowDto> {
    let mut rows: Vec<OfferHistoryRowDto> = series
        .iter()
        .flat_map(|s| {
            s.points.iter().map(|p| OfferHistoryRowDto {
                at: p.at.clone(),
                run_id: p.run_id,
                pharmacy_id: s.pharmacy_id,
                pharmacy: s.name.clone(),
                city: s.city.clone(),
                price: p.price,
                price_per_thc_gram: p.price_per_thc_gram,
                availability: p.availability.clone(),
            })
        })
        .collect();
    rows.sort_by(|a, b| {
        b.at.cmp(&a.at)
            .then_with(|| by_pharmacy(&a.pharmacy, &b.pharmacy))
            .then_with(|| a.pharmacy_id.cmp(&b.pharmacy_id))
    });
    rows
}

#[derive(PartialEq)]
enum StateKey {
    Listed(Option<f64>, String),
    Delisted,
}

/// Phases per pharmacy; `from` desc, pharmacy asc.
pub fn phases(series: &[PharmacySeriesDto]) -> Vec<OfferPhaseRowDto> {
    let buckets = buckets(series);
    let Some(latest) = buckets.iter().next_back().cloned() else {
        return Vec::new();
    };
    let mut rows = Vec::new();

    for s in series {
        let by_at: HashMap<&str, _> = s.points.iter().map(|p| (p.at.as_str(), p)).collect();
        let mut current: Option<(StateKey, OfferPhaseRowDto)> = None;
        let mut seen = false;
        for at in &buckets {
            let point = by_at.get(at.as_str()).copied();
            let listed = point.is_some_and(|p| p.price.is_some() || !p.availability.is_empty());
            if !listed && !seen {
                continue; // runs before the pharmacy first listed the strain
            }
            seen = true;
            let state = match point {
                Some(p) if listed => StateKey::Listed(p.price, p.availability.clone()),
                _ => StateKey::Delisted,
            };
            let to = (at != &latest).then(|| at.clone());
            if let Some((key, row)) = current.as_mut()
                && *key == state
            {
                row.to = to;
                row.runs += 1;
                continue;
            }
            if let Some((_, row)) = current.take() {
                rows.push(row);
            }
            let point = point.filter(|_| listed);
            current = Some((
                state,
                OfferPhaseRowDto {
                    pharmacy_id: s.pharmacy_id,
                    pharmacy: s.name.clone(),
                    city: s.city.clone(),
                    price: point.and_then(|p| p.price),
                    price_per_thc_gram: point.and_then(|p| p.price_per_thc_gram),
                    availability: point.map(|p| p.availability.clone()).unwrap_or_default(),
                    from: at.clone(),
                    to,
                    runs: 1,
                    delisted: point.is_none(),
                },
            ));
        }
        if let Some((_, row)) = current {
            rows.push(row);
        }
    }

    rows.sort_by(|a, b| {
        b.from
            .cmp(&a.from)
            .then_with(|| by_pharmacy(&a.pharmacy, &b.pharmacy))
            .then_with(|| a.pharmacy_id.cmp(&b.pharmacy_id))
    });
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::PharmacySeriesPointDto;

    fn point(at: &str, price: Option<f64>) -> PharmacySeriesPointDto {
        PharmacySeriesPointDto {
            run_id: None,
            at: at.into(),
            price,
            price_per_thc_gram: price.map(|p| p * 5.0),
            availability: if price.is_some() {
                "Auf Lager".into()
            } else {
                String::new()
            },
        }
    }

    fn series(id: i64, name: &str, points: Vec<PharmacySeriesPointDto>) -> PharmacySeriesDto {
        PharmacySeriesDto {
            pharmacy_id: id,
            name: name.into(),
            city: "Berlin".into(),
            points,
        }
    }

    #[test]
    fn phases_merge_delist_and_ignore_leading_gaps() {
        let s = vec![
            series(
                1,
                "Zeta",
                vec![
                    point("t1", Some(6.0)),
                    point("t2", Some(6.0)),
                    point("t4", Some(7.0)),
                ],
            ),
            series(
                2,
                "Äpfel",
                vec![point("t3", Some(5.0)), point("t4", Some(5.0))],
            ),
        ];
        let rows = phases(&s);
        let summary: Vec<(String, String, Option<String>, i64, bool)> = rows
            .iter()
            .map(|r| {
                (
                    r.pharmacy.clone(),
                    r.from.clone(),
                    r.to.clone(),
                    r.runs,
                    r.delisted,
                )
            })
            .collect();
        let expected: Vec<(String, String, Option<String>, i64, bool)> = vec![
            ("Zeta".into(), "t4".into(), None, 1, false),
            ("Äpfel".into(), "t3".into(), None, 2, false),
            ("Zeta".into(), "t3".into(), Some("t3".into()), 1, true),
            ("Zeta".into(), "t1".into(), Some("t2".into()), 2, false),
        ];
        assert_eq!(summary, expected);
        // delisted rows carry no price/availability
        assert_eq!(rows[2].price, None);
        assert_eq!(rows[2].availability, "");
    }

    #[test]
    fn all_rows_sorted_at_desc_pharmacy_asc() {
        let s = vec![
            series(
                1,
                "zeta",
                vec![point("t1", Some(6.0)), point("t2", Some(6.0))],
            ),
            series(2, "Alpha", vec![point("t2", Some(5.0))]),
        ];
        let rows = all_rows(&s);
        let keys: Vec<(&str, &str)> = rows
            .iter()
            .map(|r| (r.at.as_str(), r.pharmacy.as_str()))
            .collect();
        assert_eq!(keys, [("t2", "Alpha"), ("t2", "zeta"), ("t1", "zeta")]);
    }

    #[test]
    fn empty_series_yield_no_rows() {
        assert!(phases(&[]).is_empty());
        assert!(all_rows(&[]).is_empty());
    }
}
