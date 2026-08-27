//! Text and number helpers (`parse_decimal`, `parse_percent`, `clean_text`, ...).

use std::sync::LazyLock;

use regex::Regex;

/// First decimal number in a label, e.g. `"5,49 €/g"` → `5,49`.
static NUMBER_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"([0-9]+(?:[.,][0-9]+)?)").expect("static regex"));

/// Collapse all whitespace (including non-breaking spaces) to single spaces and trim.
pub fn clean_text(value: Option<&str>) -> String {
    value
        .unwrap_or_default()
        .replace('\u{a0}', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Normalised grouping key for strain names: `lower(clean_text(value))`, exactly
/// as the API contract defines it (no Unicode normalisation, so the key order
/// matches the former `build_site.py` grouping order).
pub fn strain_key(value: &str) -> String {
    clean_text(Some(value)).to_lowercase()
}

/// Parse the first number in a price label (`"5,49 €/g"` → `5.49`).
pub fn parse_decimal(value: &str) -> Option<f64> {
    let normalised = value.replace('\u{a0}', " ");
    let caps = NUMBER_RE.captures(&normalised)?;
    caps[1].replace(',', ".").parse().ok()
}

/// Parse a percentage label. `"<1%"` becomes `0.99`, `"<0%"` stays `0`.
pub fn parse_percent(value: &str) -> Option<f64> {
    let stripped = value.trim();
    if stripped.is_empty() {
        return None;
    }
    let caps = NUMBER_RE.captures(stripped)?;
    let parsed: f64 = caps[1].replace(',', ".").parse().ok()?;
    if stripped.starts_with('<') {
        Some((parsed - 0.01).max(0.0))
    } else {
        Some(parsed)
    }
}

/// Round to two decimals the way Python's `round(x, 2)` does: on the exact
/// decimal expansion of the double, ties to even.
pub fn round2(value: f64) -> f64 {
    format!("{value:.2}").parse().unwrap_or(value)
}

/// Price per gram of the cannabinoid: `round2(price / (percent / 100))`.
pub fn calculate_thc_price(price: Option<f64>, percent: Option<f64>) -> Option<f64> {
    let price = price?;
    let percent = percent?;
    if percent <= 0.0 {
        return None;
    }
    Some(round2(price / (percent / 100.0)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case("9,50 €", 9.5)]
    #[case("8.00", 8.0)]
    #[case("12", 12.0)]
    #[case("1\u{a0}234,5", 1.0)] // first number wins; nbsp normalised to space
    #[case("ab 7,25 €/g", 7.25)]
    fn parse_decimal_parses_first_number(#[case] value: &str, #[case] expected: f64) {
        assert_eq!(parse_decimal(value), Some(expected));
    }

    #[rstest]
    #[case("")]
    #[case("   ")]
    #[case("kein Preis")]
    #[case("€")]
    fn parse_decimal_returns_none_without_number(#[case] value: &str) {
        assert_eq!(parse_decimal(value), None);
    }

    #[rstest]
    #[case("20%", 20.0)]
    #[case("18,5 %", 18.5)]
    #[case("1", 1.0)]
    fn parse_percent_plain(#[case] value: &str, #[case] expected: f64) {
        assert_eq!(parse_percent(value), Some(expected));
    }

    #[test]
    fn parse_percent_less_than_prefix_subtracts_epsilon() {
        let value = parse_percent("<1%").unwrap();
        assert!((value - 0.99).abs() < 1e-9, "{value}");
    }

    #[test]
    fn parse_percent_less_than_never_negative() {
        assert_eq!(parse_percent("<0%"), Some(0.0));
    }

    #[rstest]
    #[case("")]
    #[case("   ")]
    #[case("n/a")]
    fn parse_percent_returns_none_without_number(#[case] value: &str) {
        assert_eq!(parse_percent(value), None);
    }

    #[test]
    fn thc_price_basic_division() {
        // 9.50 €/g at 20% THC -> 47.50 €/g THC
        assert_eq!(calculate_thc_price(Some(9.5), Some(20.0)), Some(47.5));
    }

    #[test]
    fn thc_price_rounds_to_two_decimals() {
        assert_eq!(calculate_thc_price(Some(8.0), Some(18.0)), Some(44.44));
    }

    #[rstest]
    #[case(None, Some(20.0))]
    #[case(Some(9.5), None)]
    #[case(Some(9.5), Some(0.0))]
    #[case(Some(9.5), Some(-5.0))]
    fn thc_price_returns_none_for_invalid_inputs(
        #[case] price: Option<f64>,
        #[case] thc: Option<f64>,
    ) {
        assert_eq!(calculate_thc_price(price, thc), None);
    }

    #[rstest]
    #[case(Some("  hello   world "), "hello world")]
    #[case(Some("non\u{a0}breaking"), "non breaking")]
    #[case(None, "")]
    #[case(Some(""), "")]
    #[case(Some("\t\n  spaced \n"), "spaced")]
    fn clean_text_normalises_whitespace(#[case] value: Option<&str>, #[case] expected: &str) {
        assert_eq!(clean_text(value), expected);
    }

    #[rstest]
    #[case("Sorte X", "sorte x")]
    #[case("  Sorte\u{a0}X ", "sorte x")]
    #[case("ﬁne", "ﬁne")] // no NFKC: the key is plain lower(clean_text())
    #[case("Mo\u{b4}s Sunset", "mo\u{b4}s sunset")]
    fn strain_key_normalises(#[case] value: &str, #[case] expected: &str) {
        assert_eq!(strain_key(value), expected);
    }

    #[rstest]
    #[case(0.125, 0.12)] // ties to even, like Python
    #[case(0.375, 0.38)]
    #[case(44.4444, 44.44)]
    #[case(2.675, 2.67)] // binary 2.675 is slightly below the tie
    fn round2_matches_python(#[case] value: f64, #[case] expected: f64) {
        assert_eq!(round2(value), expected);
    }
}
