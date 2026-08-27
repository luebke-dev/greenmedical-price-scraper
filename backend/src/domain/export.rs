//! CSV export, byte-compatible with the former `greenmedical_flowers.csv`
//! (Python `csv.DictWriter` defaults: minimal quoting, CRLF line endings).

use super::model::{CSV_FIELDNAMES, OfferRecord};

/// Render offers as CSV in scrape order.
pub fn to_csv(offers: &[OfferRecord]) -> Vec<u8> {
    let mut writer = csv::WriterBuilder::new()
        .terminator(csv::Terminator::CRLF)
        .quote_style(csv::QuoteStyle::Necessary)
        .from_writer(Vec::new());
    writer
        .write_record(CSV_FIELDNAMES)
        .expect("writing to a Vec cannot fail");
    for offer in offers {
        writer
            .write_record([
                offer.apotheke.as_str(),
                offer.apotheke_plz.as_str(),
                offer.apotheke_stadt.as_str(),
                offer.name.as_str(),
                offer.bezeichnung.as_str(),
                offer.genetik.as_str(),
                offer.thc.as_str(),
                offer.cbd.as_str(),
                offer.preis_pro_gramm.as_str(),
                offer.verfuegbarkeit.as_str(),
                offer.produkt_url.as_str(),
            ])
            .expect("writing to a Vec cannot fail");
    }
    writer.into_inner().expect("writing to a Vec cannot fail")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_matches_fieldnames_and_rows_are_quoted_minimally() {
        let offer = OfferRecord {
            apotheke: "Grüne Blüte".into(),
            apotheke_plz: "04416".into(),
            apotheke_stadt: "Markkleeberg".into(),
            name: "Bunatic".into(),
            bezeichnung: "Luana 27/1 Donny B".into(),
            genetik: "Indica".into(),
            thc: "27%".into(),
            cbd: "1%".into(),
            preis_pro_gramm: "5,49 €/g".into(),
            verfuegbarkeit: "Auf Lager".into(),
            produkt_url: "https://greenmedical.health/de/cannabis/flower/x?deliveryTarget=T%3D"
                .into(),
            ..OfferRecord::default()
        };
        let csv = String::from_utf8(to_csv(&[offer])).unwrap();
        let expected = concat!(
            "apotheke,apotheke_plz,apotheke_stadt,name,bezeichnung,genetik,thc,cbd,preis_pro_gramm,verfuegbarkeit,produkt_url\r\n",
            "Grüne Blüte,04416,Markkleeberg,Bunatic,Luana 27/1 Donny B,Indica,27%,1%,\"5,49 €/g\",Auf Lager,https://greenmedical.health/de/cannabis/flower/x?deliveryTarget=T%3D\r\n"
        );
        assert_eq!(csv, expected);
    }

    #[test]
    fn empty_export_has_only_the_header() {
        let csv = String::from_utf8(to_csv(&[])).unwrap();
        assert_eq!(csv, format!("{}\r\n", CSV_FIELDNAMES.join(",")));
    }
}
