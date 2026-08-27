//! `group_by_strain`: collapse offers into one record per strain.

use std::collections::{BTreeMap, HashSet};

use super::model::{OfferDto, OfferRecord, SortDto, StrainDto};
use super::text::strain_key;

/// Grouping identity of an offer: `(strain_key(name), strain_key(bezeichnung))`.
pub fn group_key(offer: &OfferRecord) -> (String, String) {
    (strain_key(&offer.name), strain_key(&offer.bezeichnung))
}

fn first_nonempty<'a>(mut values: impl Iterator<Item = &'a str>) -> String {
    values
        .find(|v| !v.is_empty())
        .unwrap_or_default()
        .to_owned()
}

fn first_not_none(values: impl Iterator<Item = Option<f64>>) -> Option<f64> {
    values.flatten().next()
}

fn min_of(values: impl Iterator<Item = Option<f64>>) -> Option<f64> {
    values.flatten().fold(None, |acc, v| match acc {
        Some(current) if current <= v => Some(current),
        _ => Some(v),
    })
}

/// Deduplicate offers into one record per strain (name + Bezeichnung).
///
/// Groups are ordered by their key; each strain lists its offers cheapest
/// first with unpriced offers last (stable within ties). Display fields take
/// the first non-empty value among the members, exactly like the Python port.
pub fn group_by_strain(offers: &[OfferRecord]) -> Vec<StrainDto> {
    let mut groups: BTreeMap<(String, String), Vec<&OfferRecord>> = BTreeMap::new();
    for offer in offers {
        groups.entry(group_key(offer)).or_default().push(offer);
    }

    groups
        .into_values()
        .map(|members| {
            let mut members_sorted = members.clone();
            members_sorted.sort_by(|a, b| {
                let key = |o: &OfferRecord| {
                    (
                        o.preis_eur_pro_gramm.is_none(),
                        o.preis_eur_pro_gramm.unwrap_or(0.0),
                    )
                };
                let (an, av) = key(a);
                let (bn, bv) = key(b);
                an.cmp(&bn).then_with(|| av.total_cmp(&bv))
            });

            let min_price = min_of(members.iter().map(|o| o.preis_eur_pro_gramm));
            let min_thc_price = min_of(members.iter().map(|o| o.preis_eur_pro_gramm_thc));

            let name = first_nonempty(members.iter().map(|o| o.name.as_str()));
            let bezeichnung = first_nonempty(members.iter().map(|o| o.bezeichnung.as_str()));
            let genetik = first_nonempty(members.iter().map(|o| o.genetik.as_str()));
            let thc = first_nonempty(members.iter().map(|o| o.thc.as_str()));
            let cbd = first_nonempty(members.iter().map(|o| o.cbd.as_str()));

            let offer_records = members_sorted
                .iter()
                .map(|o| OfferDto {
                    offer_id: o.offer_id,
                    pharmacy_id: o.pharmacy_id,
                    provider: o.provider,
                    apotheke: o.apotheke.clone(),
                    apotheke_plz: o.apotheke_plz.clone(),
                    apotheke_stadt: o.apotheke_stadt.clone(),
                    preis_pro_gramm: o.preis_pro_gramm.clone(),
                    preis_eur_pro_gramm: o.preis_eur_pro_gramm,
                    preis_eur_pro_gramm_thc: o.preis_eur_pro_gramm_thc,
                    verfuegbarkeit: o.verfuegbarkeit.clone(),
                    produkt_url: o.produkt_url.clone(),
                })
                .collect();

            let offer_search = members
                .iter()
                .flat_map(|o| {
                    [
                        o.apotheke.as_str(),
                        o.apotheke_plz.as_str(),
                        o.apotheke_stadt.as_str(),
                        o.preis_pro_gramm.as_str(),
                        o.verfuegbarkeit.as_str(),
                    ]
                })
                .collect::<Vec<_>>()
                .join(" ");
            let thc_price_search = match min_thc_price {
                None => String::new(),
                Some(p) => format!("{p:.2} €/g thc"),
            };
            let search = [
                name.as_str(),
                bezeichnung.as_str(),
                genetik.as_str(),
                thc.as_str(),
                cbd.as_str(),
                offer_search.as_str(),
                thc_price_search.as_str(),
            ]
            .join(" ")
            .to_lowercase();

            let thc_value = first_not_none(members.iter().map(|o| o.thc_value));
            let cbd_value = first_not_none(members.iter().map(|o| o.cbd_value));
            let pharmacy_count = members
                .iter()
                .filter(|o| !o.apotheke.is_empty())
                .map(|o| o.pharmacy_id)
                .collect::<HashSet<_>>()
                .len() as i64;

            StrainDto {
                id: members[0].strain_id,
                name,
                bezeichnung,
                genetik,
                thc,
                cbd,
                thc_value,
                cbd_value,
                min_price,
                min_price_per_thc_gram: min_thc_price,
                pharmacy_count,
                offers: offer_records,
                sort: SortDto {
                    price: min_price,
                    price_per_thc_gram: min_thc_price,
                    thc: thc_value,
                    cbd: cbd_value,
                    rating: None,
                },
                search,
                trend: None,
                rating: None,
                product_uuid: None,
            }
        })
        .collect()
}

#[cfg(test)]
pub(crate) mod test_support {
    //! Helpers mirroring `_write_csv` + `read_offers` from the Python tests.

    use crate::domain::model::OfferRecord;
    use crate::domain::text::{calculate_thc_price, clean_text, parse_decimal, parse_percent};

    /// Build offer records from CSV-like rows (`csv` crate, same columns as the
    /// old scraper, trailing columns may be omitted).
    pub fn offers_from_csv(rows: &str) -> Vec<OfferRecord> {
        let header = crate::domain::model::CSV_FIELDNAMES.join(",");
        let text = format!("{header}\n{}\n", rows.trim());
        let mut reader = csv::ReaderBuilder::new()
            .flexible(true)
            .from_reader(text.as_bytes());
        let mut offers = Vec::new();
        for (index, record) in reader.records().enumerate() {
            let record = record.expect("valid csv");
            let field = |i: usize| record.get(i);
            let price_label = clean_text(field(8));
            let thc = clean_text(field(6));
            let cbd = clean_text(field(7));
            let price = parse_decimal(&price_label);
            let thc_value = parse_percent(&thc);
            let cbd_value = parse_percent(&cbd);
            offers.push(OfferRecord {
                offer_id: index as i64 + 1,
                pharmacy_id: 0,
                provider: crate::domain::Provider::Greenmedical,
                strain_id: 0,
                apotheke: clean_text(field(0)),
                apotheke_plz: clean_text(field(1)),
                apotheke_stadt: clean_text(field(2)),
                name: clean_text(field(3)),
                bezeichnung: clean_text(field(4)),
                genetik: clean_text(field(5)),
                thc,
                cbd,
                preis_pro_gramm: price_label,
                verfuegbarkeit: clean_text(field(9)),
                produkt_url: field(10).unwrap_or_default().trim().to_owned(),
                preis_eur_pro_gramm: price,
                preis_eur_pro_gramm_thc: calculate_thc_price(price, thc_value),
                preis_eur_pro_gramm_cbd: calculate_thc_price(price, cbd_value),
                thc_value,
                cbd_value,
            });
        }
        offers
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::offers_from_csv;
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn same_strain_across_pharmacies_is_deduplicated() {
        let offers = offers_from_csv(
            r#"Apo A,10115,Berlin,Sorte X,EMK,Indica,20%,1%,"9,50 €",verfügbar
Apo B,20095,Hamburg,Sorte X,EMK,Indica,20%,1%,"8,00 €",neu
Apo C,50667,Köln,Sorte Y,XYZ,Sativa,18%,1%,"7,00 €",verfügbar"#,
        );
        let strains = group_by_strain(&offers);
        assert_eq!(strains.len(), 2);
        let x = strains.iter().find(|s| s.name == "Sorte X").unwrap();
        assert_eq!(x.pharmacy_count, 2);
        assert_eq!(x.offers.len(), 2);
        // cheapest offer first
        let names: Vec<_> = x.offers.iter().map(|o| o.apotheke.as_str()).collect();
        assert_eq!(names, ["Apo B", "Apo A"]);
        assert_eq!(x.min_price, Some(8.0));
        // 8.00 / (20/100) = 40.00 €/g THC
        assert_eq!(x.min_price_per_thc_gram, Some(40.0));
    }

    #[test]
    fn grouping_is_case_insensitive() {
        let offers = offers_from_csv(
            r#"Apo A,10115,Berlin,Sorte X,EMK,Indica,20%,1%,"9,50 €",verfügbar
Apo B,20095,Hamburg,sorte x,emk,Indica,20%,1%,"8,00 €",neu"#,
        );
        let strains = group_by_strain(&offers);
        assert_eq!(strains.len(), 1);
        assert_eq!(strains[0].pharmacy_count, 2);
    }

    #[test]
    fn search_index_includes_pharmacies() {
        let offers = offers_from_csv(
            r#"Adler Apotheke,10115,Berlin,Sorte X,EMK,Indica,20%,1%,"9,50 €",verfügbar"#,
        );
        let strain = group_by_strain(&offers).remove(0);
        assert!(strain.search.contains("adler apotheke"));
        assert!(strain.search.contains("berlin"));
        assert!(strain.search.contains("47.50 €/g thc"));
    }

    #[test]
    fn unpriced_offers_sort_last_and_groups_sort_by_key() {
        let offers = offers_from_csv(
            r#"Apo A,1,B,Zeta,Z1,Indica,20%,1%,kein Preis,neu
Apo B,2,C,Zeta,Z1,Indica,20%,1%,"5,00 €",neu
Apo C,3,D,Alpha,A1,Sativa,10%,1%,"9,00 €",neu"#,
        );
        let strains = group_by_strain(&offers);
        assert_eq!(strains[0].name, "Alpha");
        assert_eq!(strains[1].name, "Zeta");
        let zeta = &strains[1];
        assert_eq!(zeta.offers[0].apotheke, "Apo B");
        assert_eq!(zeta.offers[1].preis_eur_pro_gramm, None);
        assert_eq!(zeta.min_price, Some(5.0));
    }

    #[test]
    fn strain_record_shape_matches_contract() {
        let offers =
            offers_from_csv(r#"Apo A,10115,Berlin,Sorte X,EMK,Indica,20%,1%,"9,50 €",verfügbar"#);
        let strain = group_by_strain(&offers).remove(0);
        let value = serde_json::to_value(&strain).unwrap();
        let keys: BTreeSet<_> = value.as_object().unwrap().keys().cloned().collect();
        let expected: BTreeSet<String> = [
            "id",
            "name",
            "bezeichnung",
            "genetik",
            "thc",
            "cbd",
            "min_price",
            "min_price_per_thc_gram",
            "pharmacy_count",
            "offers",
            "sort",
            "search",
            // contract additions
            "thc_value",
            "cbd_value",
            "trend",
            "rating",
            "product_uuid",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        assert_eq!(keys, expected);

        let offer_keys: BTreeSet<_> = value["offers"][0]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        let expected_offer: BTreeSet<String> = [
            "apotheke",
            "apotheke_plz",
            "apotheke_stadt",
            "preis_pro_gramm",
            "preis_eur_pro_gramm",
            "preis_eur_pro_gramm_thc",
            "verfuegbarkeit",
            "produkt_url",
            "offer_id",
            "pharmacy_id",
            "provider",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        assert_eq!(offer_keys, expected_offer);

        let sort_keys: BTreeSet<_> = value["sort"].as_object().unwrap().keys().cloned().collect();
        let expected_sort: BTreeSet<String> =
            ["price", "price_per_thc_gram", "thc", "cbd", "rating"]
                .into_iter()
                .map(String::from)
                .collect();
        assert_eq!(sort_keys, expected_sort);
    }

    #[test]
    fn url_flows_into_offers() {
        let offers = offers_from_csv(
            r#"Apo A,10115,Berlin,Sorte X,EMK,Indica,20%,1%,"9,50 €",verfügbar,https://greenmedical.health/p?deliveryTarget=T"#,
        );
        let strains = group_by_strain(&offers);
        assert_eq!(
            strains[0].offers[0].produkt_url,
            "https://greenmedical.health/p?deliveryTarget=T"
        );
    }
}
