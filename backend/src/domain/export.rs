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
                offer.pharmacy.as_str(),
                offer.pharmacy_postal_code.as_str(),
                offer.pharmacy_city.as_str(),
                offer.name.as_str(),
                offer.designation.as_str(),
                offer.genetics.as_str(),
                offer.thc.as_str(),
                offer.cbd.as_str(),
                offer.price_per_gram.as_str(),
                offer.availability.as_str(),
                offer.product_url.as_str(),
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
            pharmacy: "Grüne Blüte".into(),
            pharmacy_postal_code: "04416".into(),
            pharmacy_city: "Markkleeberg".into(),
            name: "Bunatic".into(),
            designation: "Luana 27/1 Donny B".into(),
            genetics: "Indica".into(),
            thc: "27%".into(),
            cbd: "1%".into(),
            price_per_gram: "5,49 €/g".into(),
            availability: "Auf Lager".into(),
            product_url: "https://greenmedical.health/de/cannabis/flower/x?deliveryTarget=T%3D"
                .into(),
            ..OfferRecord::default()
        };
        let csv = String::from_utf8(to_csv(&[offer])).unwrap();
        let expected = concat!(
            "pharmacy,pharmacy_postal_code,pharmacy_city,name,designation,genetics,thc,cbd,price_per_gram,availability,product_url\r\n",
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
