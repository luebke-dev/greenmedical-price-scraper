//! German, case-insensitive, numeric-aware text ordering.
//!
//! Approximates `Intl.Collator('de', { numeric: true, sensitivity: 'base' })`
//! without an ICU dependency:
//!
//! 1. [`fold`] builds a sort key: Unicode lowercase, NFKD decomposition with all
//!    combining marks removed (`Ä` → `a`, `é` → `e`), `ß` → `ss`.
//! 2. [`compare`] walks two folded keys and compares runs of ASCII digits by
//!    their numeric value (`sorte 9` < `sorte 10`); everything else is compared
//!    by code point.
//!
//! Differences to ICU are limited to exotic cases (punctuation weighting,
//! non-Latin scripts) that do not occur in the strain catalogue.

use std::cmp::Ordering;

use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;

/// Case- and diacritic-folded key for [`compare`].
pub fn fold(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.nfkd() {
        if is_combining_mark(ch) {
            continue;
        }
        if ch == 'ß' {
            out.push_str("ss");
            continue;
        }
        for lower in ch.to_lowercase() {
            out.push(lower);
        }
    }
    out
}

/// Compare two [`fold`]ed keys with numeric-aware ordering of digit runs.
pub fn compare(a: &str, b: &str) -> Ordering {
    let a = a.as_bytes();
    let b = b.as_bytes();
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        if a[i].is_ascii_digit() && b[j].is_ascii_digit() {
            let (na, ni) = digit_run(a, i);
            let (nb, nj) = digit_run(b, j);
            match na.len().cmp(&nb.len()).then_with(|| na.cmp(nb)) {
                Ordering::Equal => {}
                other => return other,
            }
            i = ni;
            j = nj;
        } else {
            match a[i].cmp(&b[j]) {
                Ordering::Equal => {}
                other => return other,
            }
            i += 1;
            j += 1;
        }
    }
    (a.len() - i).cmp(&(b.len() - j))
}

/// Digit run starting at `start` without leading zeros, plus the index after it.
fn digit_run(bytes: &[u8], start: usize) -> (&[u8], usize) {
    let mut end = start;
    while end < bytes.len() && bytes[end].is_ascii_digit() {
        end += 1;
    }
    let mut first = start;
    while first + 1 < end && bytes[first] == b'0' {
        first += 1;
    }
    (&bytes[first..end], end)
}

/// Convenience: compare two raw strings (folds both first).
pub fn compare_str(a: &str, b: &str) -> Ordering {
    compare(&fold(a), &fold(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folds_case_and_diacritics() {
        assert_eq!(fold("Äpfel"), "apfel");
        assert_eq!(fold("Straße"), "strasse");
        assert_eq!(fold("Élan"), "elan");
    }

    #[test]
    fn orders_like_de_base_numeric() {
        let mut values = vec!["Zebra", "apfel", "Äpfel", "Sorte 10", "Sorte 9", "Öl"];
        values.sort_by(|a, b| compare_str(a, b));
        assert_eq!(
            values,
            ["apfel", "Äpfel", "Öl", "Sorte 9", "Sorte 10", "Zebra"]
        );
        assert_eq!(compare_str("Äpfel", "apfel"), Ordering::Equal);
        assert_eq!(compare_str("a 007", "a 7"), Ordering::Equal);
        assert_eq!(compare_str("a 2b", "a 10a"), Ordering::Less);
        assert_eq!(compare_str("a", "a 1"), Ordering::Less);
    }
}
