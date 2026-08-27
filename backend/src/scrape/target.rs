//! Delivery-target encoding and URL rewriting (`make_delivery_target`, `with_delivery_target`).

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use url::Url;

/// Encode a pharmacy UUID as the site's `deliveryTarget` parameter.
pub fn make_delivery_target(uuid: &str) -> String {
    STANDARD.encode(format!("pharmacy:|:{uuid}"))
}

/// Percent-encode like Python's `urllib.parse.quote_plus`.
fn quote_plus(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Add or replace the `deliveryTarget` query parameter so the link opens at
/// this pharmacy. Mirrors `dict(parse_qsl(...))` + `urlencode`: duplicate
/// keys keep the last value at the first position, blank values are dropped.
pub fn with_delivery_target(url: &str, delivery_target: &str) -> String {
    let Ok(mut parsed) = Url::parse(url) else {
        return url.to_owned();
    };

    let mut pairs: Vec<(String, String)> = Vec::new();
    if let Some(query) = parsed.query() {
        for (key, value) in url::form_urlencoded::parse(query.as_bytes()) {
            if value.is_empty() {
                continue;
            }
            match pairs.iter_mut().find(|(k, _)| *k == key) {
                Some(existing) => existing.1 = value.into_owned(),
                None => pairs.push((key.into_owned(), value.into_owned())),
            }
        }
    }
    match pairs.iter_mut().find(|(k, _)| k == "deliveryTarget") {
        Some(existing) => existing.1 = delivery_target.to_owned(),
        None => pairs.push(("deliveryTarget".to_owned(), delivery_target.to_owned())),
    }

    let encoded = pairs
        .iter()
        .map(|(k, v)| format!("{}={}", quote_plus(k), quote_plus(v)))
        .collect::<Vec<_>>()
        .join("&");
    parsed.set_query(Some(&encoded));
    // The tile links point at "#reviews"; the fragment only scrolls to the review section and
    // is not wanted on the stored product URL.
    parsed.set_fragment(None);
    parsed.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;

    #[test]
    fn make_delivery_target_round_trips_uuid() {
        let uuid = "abc123-def456";
        let encoded = make_delivery_target(uuid);
        let decoded = STANDARD.decode(&encoded).unwrap();
        assert_eq!(
            String::from_utf8(decoded).unwrap(),
            format!("pharmacy:|:{uuid}")
        );
    }

    #[test]
    fn make_delivery_target_matches_real_site_value() {
        assert_eq!(
            make_delivery_target("b4bddcc5-dc41-49d8-87df-14a03d561b32"),
            "cGhhcm1hY3k6fDpiNGJkZGNjNS1kYzQxLTQ5ZDgtODdkZi0xNGEwM2Q1NjFiMzI="
        );
    }

    #[test]
    fn appends_delivery_target() {
        let base = "https://greenmedical.health/de/cannabis/flowers/x";
        assert_eq!(
            with_delivery_target(base, "TOKEN"),
            format!("{base}?deliveryTarget=TOKEN")
        );
    }

    #[test]
    fn replaces_existing_delivery_target() {
        let url = with_delivery_target(
            "https://greenmedical.health/p?deliveryTarget=old&foo=bar",
            "NEW",
        );
        assert!(url.contains("deliveryTarget=NEW"));
        assert!(!url.contains("deliveryTarget=old"));
        assert!(url.contains("foo=bar"));
        assert_eq!(
            url,
            "https://greenmedical.health/p?deliveryTarget=NEW&foo=bar"
        );
    }

    #[test]
    fn encodes_padding_and_strips_fragment() {
        let target = make_delivery_target("b4bddcc5-dc41-49d8-87df-14a03d561b32");
        let url = with_delivery_target(
            "https://greenmedical.health/de/cannabis/flower/luana_27_1_donny_b-bunatic#reviews",
            &target,
        );
        assert_eq!(
            url,
            "https://greenmedical.health/de/cannabis/flower/luana_27_1_donny_b-bunatic?deliveryTarget=cGhhcm1hY3k6fDpiNGJkZGNjNS1kYzQxLTQ5ZDgtODdkZi0xNGEwM2Q1NjFiMzI%3D"
        );
    }

    #[test]
    fn drops_blank_values_and_dedupes_like_parse_qsl() {
        let url = with_delivery_target("https://gm.test/p?a=1&b=&a=2&c=x+y", "T");
        assert_eq!(url, "https://gm.test/p?a=2&c=x+y&deliveryTarget=T");
    }

    #[test]
    fn invalid_url_is_returned_unchanged() {
        assert_eq!(with_delivery_target("not a url", "T"), "not a url");
    }
}
