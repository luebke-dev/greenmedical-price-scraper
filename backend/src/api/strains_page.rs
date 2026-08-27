//! `GET /strains`: query parsing, filtering and paging over the [`Snapshot`].

use std::fmt::Write as _;

use serde::Deserialize;
use utoipa::IntoParams;

use super::error::ApiError;
use crate::db::snapshot::{Snapshot, SortDir, StrainSort};
use crate::domain::{StrainDto, StrainsPageDto};

const DEFAULT_LIMIT: i64 = 50;
const MAX_LIMIT: i64 = 500;

/// Raw query parameters (unknown ones are ignored by serde).
#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct StrainsQuery {
    /// Volltext (case-insensitiv, Teilstring) über Name, Bezeichnung, Genetik, THC-/CBD-Label.
    pub q: Option<String>,
    /// Genetik-Werte, kommagetrennt, case-insensitiv (`Indica,Sativa`).
    pub genetik: Option<String>,
    /// Untergrenze `min_price` (€/g, inklusive); Sorten ohne Preis nur ohne Grenzen.
    pub price_min: Option<f64>,
    /// Obergrenze `min_price` (€/g, inklusive).
    pub price_max: Option<f64>,
    /// Untergrenze `thc_value` (%, inklusive).
    pub thc_min: Option<f64>,
    /// Obergrenze `thc_value` (%, inklusive).
    pub thc_max: Option<f64>,
    /// Untergrenze `cbd_value` (%, inklusive).
    pub cbd_min: Option<f64>,
    /// Obergrenze `cbd_value` (%, inklusive).
    pub cbd_max: Option<f64>,
    /// Mindestbewertung; Sorten ohne Bewertung fallen heraus.
    pub rating_min: Option<f64>,
    /// `price` (Default) | `price_per_thc_gram` | `thc` | `cbd` | `pharmacy_count` | `rating` | `name` | `bezeichnung` | `genetik`.
    #[param(example = "price")]
    pub sort: Option<String>,
    /// `asc` (Default) | `desc`; numerische Nullwerte stehen in beiden Richtungen hinten.
    #[param(example = "asc")]
    pub dir: Option<String>,
    /// 1–500, Default 50.
    pub limit: Option<i64>,
    /// ≥ 0, Default 0.
    pub offset: Option<i64>,
}

/// Inclusive range filter; strains without a value pass only when both bounds are absent.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Range {
    pub min: Option<f64>,
    pub max: Option<f64>,
}

impl Range {
    fn matches(self, value: Option<f64>) -> bool {
        if self.min.is_none() && self.max.is_none() {
            return true;
        }
        let Some(v) = value else { return false };
        self.min.is_none_or(|m| v >= m) && self.max.is_none_or(|m| v <= m)
    }
}

/// Validated request.
#[derive(Debug, Clone, PartialEq)]
pub struct StrainsRequest {
    pub q: Option<String>,
    /// Lowercased, deduplicated, sorted.
    pub genetik: Vec<String>,
    pub price: Range,
    pub thc: Range,
    pub cbd: Range,
    pub rating_min: Option<f64>,
    pub sort: StrainSort,
    pub dir: SortDir,
    pub limit: i64,
    pub offset: i64,
}

fn finite(name: &str, value: Option<f64>) -> Result<Option<f64>, ApiError> {
    match value {
        Some(v) if !v.is_finite() => Err(ApiError::bad_request(format!(
            "`{name}` muss eine endliche Zahl sein"
        ))),
        other => Ok(other),
    }
}

fn range(name: &str, min: Option<f64>, max: Option<f64>) -> Result<Range, ApiError> {
    Ok(Range {
        min: finite(&format!("{name}_min"), min)?,
        max: finite(&format!("{name}_max"), max)?,
    })
}

impl StrainsQuery {
    pub fn validate(self) -> Result<StrainsRequest, ApiError> {
        let limit = self.limit.unwrap_or(DEFAULT_LIMIT);
        if !(1..=MAX_LIMIT).contains(&limit) {
            return Err(ApiError::bad_request(format!(
                "`limit` muss zwischen 1 und {MAX_LIMIT} liegen"
            )));
        }
        let offset = self.offset.unwrap_or(0);
        if offset < 0 {
            return Err(ApiError::bad_request("`offset` darf nicht negativ sein"));
        }
        let sort = match self.sort.as_deref() {
            None | Some("") => StrainSort::Price,
            Some(value) => StrainSort::ALL
                .into_iter()
                .find(|s| s.as_str() == value)
                .ok_or_else(|| {
                    ApiError::bad_request(format!("Unbekannter `sort`-Wert {value:?}"))
                })?,
        };
        let dir = match self.dir.as_deref() {
            None | Some("") | Some("asc") => SortDir::Asc,
            Some("desc") => SortDir::Desc,
            Some(value) => {
                return Err(ApiError::bad_request(format!(
                    "`dir` muss `asc` oder `desc` sein, nicht {value:?}"
                )));
            }
        };
        let q = self
            .q
            .map(|q| q.trim().to_lowercase())
            .filter(|q| !q.is_empty());
        let mut genetik: Vec<String> = self
            .genetik
            .as_deref()
            .unwrap_or_default()
            .split(',')
            .map(|g| g.trim().to_lowercase())
            .filter(|g| !g.is_empty())
            .collect();
        genetik.sort();
        genetik.dedup();
        Ok(StrainsRequest {
            q,
            genetik,
            price: range("price", self.price_min, self.price_max)?,
            thc: range("thc", self.thc_min, self.thc_max)?,
            cbd: range("cbd", self.cbd_min, self.cbd_max)?,
            rating_min: finite("rating_min", self.rating_min)?,
            sort,
            dir,
            limit,
            offset,
        })
    }
}

impl StrainsRequest {
    fn matches(&self, snapshot: &Snapshot, index: usize) -> bool {
        let strain: &StrainDto = &snapshot.strains[index];
        if let Some(q) = &self.q
            && !strain.search.contains(q.as_str())
        {
            return false;
        }
        if !self.genetik.is_empty()
            && self
                .genetik
                .binary_search(&snapshot.keys[index].genetik_lower)
                .is_err()
        {
            return false;
        }
        if !self.price.matches(strain.sort.price)
            || !self.thc.matches(strain.sort.thc)
            || !self.cbd.matches(strain.sort.cbd)
        {
            return false;
        }
        if let Some(min) = self.rating_min
            && !strain.sort.rating.is_some_and(|r| r >= min)
        {
            return false;
        }
        true
    }

    /// Filter, sort and slice the snapshot.
    pub fn page(&self, snapshot: &Snapshot) -> StrainsPageDto {
        let order = snapshot.sorted_indices(self.sort, self.dir);
        let mut total = 0i64;
        let mut strains = Vec::with_capacity(self.limit.min(order.len() as i64) as usize);
        for &index in order {
            if !self.matches(snapshot, index) {
                continue;
            }
            if total >= self.offset && (strains.len() as i64) < self.limit {
                strains.push(snapshot.list_items[index].clone());
            }
            total += 1;
        }
        StrainsPageDto {
            run: snapshot.run.clone(),
            reference_run: snapshot.reference_run.clone(),
            total,
            limit: self.limit,
            offset: self.offset,
            facets: snapshot.facets.clone(),
            strains,
        }
    }

    /// Canonical `key=value&…` form (sorted keys, defaults included) for the ETag hash.
    pub fn normalised(&self) -> String {
        let mut pairs: Vec<(&str, String)> = Vec::new();
        let mut num = |name: &'static str, value: Option<f64>| {
            if let Some(v) = value {
                pairs.push((name, format!("{v}")));
            }
        };
        num("cbd_max", self.cbd.max);
        num("cbd_min", self.cbd.min);
        num("price_max", self.price.max);
        num("price_min", self.price.min);
        num("rating_min", self.rating_min);
        num("thc_max", self.thc.max);
        num("thc_min", self.thc.min);
        pairs.push(("dir", self.dir.as_str().into()));
        if !self.genetik.is_empty() {
            pairs.push(("genetik", self.genetik.join(",")));
        }
        pairs.push(("limit", self.limit.to_string()));
        pairs.push(("offset", self.offset.to_string()));
        if let Some(q) = &self.q {
            pairs.push(("q", q.clone()));
        }
        pairs.push(("sort", self.sort.as_str().into()));
        pairs.sort_by(|a, b| a.0.cmp(b.0));
        let mut out = String::new();
        for (i, (k, v)) in pairs.iter().enumerate() {
            if i > 0 {
                out.push('&');
            }
            let _ = write!(out, "{k}={v}");
        }
        out
    }

    /// 64-bit FNV-1a of [`normalised`](Self::normalised), hex.
    pub fn hash(&self) -> String {
        format!("{:016x}", fnv1a(self.normalised().as_bytes()))
    }
}

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_validation() {
        let req = StrainsQuery::default().validate().unwrap();
        assert_eq!(req.limit, 50);
        assert_eq!(req.offset, 0);
        assert_eq!(req.sort, StrainSort::Price);
        assert_eq!(req.dir, SortDir::Asc);
        assert!(req.genetik.is_empty());
        for (limit, offset) in [(0, 0), (501, 0), (1, -1)] {
            let query = StrainsQuery {
                limit: Some(limit),
                offset: Some(offset),
                ..Default::default()
            };
            assert!(query.validate().is_err());
        }
        assert!(
            StrainsQuery {
                sort: Some("nope".into()),
                ..Default::default()
            }
            .validate()
            .is_err()
        );
        assert!(
            StrainsQuery {
                dir: Some("up".into()),
                ..Default::default()
            }
            .validate()
            .is_err()
        );
        assert!(
            StrainsQuery {
                price_min: Some(f64::NAN),
                ..Default::default()
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn genetik_is_normalised() {
        let req = StrainsQuery {
            genetik: Some(" Sativa, indica,,SATIVA ".into()),
            q: Some("  OG ".into()),
            ..Default::default()
        }
        .validate()
        .unwrap();
        assert_eq!(req.genetik, ["indica", "sativa"]);
        assert_eq!(req.q.as_deref(), Some("og"));
    }

    #[test]
    fn range_null_rule() {
        assert!(Range::default().matches(None));
        let r = Range {
            min: Some(1.0),
            max: None,
        };
        assert!(!r.matches(None));
        assert!(r.matches(Some(1.0)));
        assert!(!r.matches(Some(0.5)));
        let r = Range {
            min: Some(1.0),
            max: Some(2.0),
        };
        assert!(r.matches(Some(2.0)));
        assert!(!r.matches(Some(2.1)));
    }

    #[test]
    fn normalised_query_is_order_independent() {
        let a = StrainsQuery {
            genetik: Some("Sativa,Indica".into()),
            price_min: Some(5.0),
            ..Default::default()
        }
        .validate()
        .unwrap();
        let b = StrainsQuery {
            genetik: Some("indica,sativa".into()),
            price_min: Some(5.0),
            sort: Some("price".into()),
            dir: Some("asc".into()),
            limit: Some(50),
            ..Default::default()
        }
        .validate()
        .unwrap();
        assert_eq!(a.normalised(), b.normalised());
        assert_eq!(
            a.normalised(),
            "dir=asc&genetik=indica,sativa&limit=50&offset=0&price_min=5&sort=price"
        );
        assert_eq!(a.hash(), b.hash());
        let c = StrainsQuery {
            offset: Some(50),
            ..Default::default()
        }
        .validate()
        .unwrap();
        assert_ne!(a.hash(), c.hash());
        assert_eq!(c.hash().len(), 16);
    }
}
