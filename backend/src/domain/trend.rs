//! Price trend of a strain versus a reference run (≥ 7 days older).

use super::model::{RunDto, TrendDirection, TrendDto};
use super::text::round2;

/// Compute the trend of the current minimum price against the reference run.
///
/// Returns `None` when there is no reference run, the strain had no priced
/// offer in it, or the current strain has no price. `flat` when the absolute
/// change is below half a cent.
pub fn compute_trend(
    min_price_now: Option<f64>,
    min_price_then: Option<f64>,
    reference: Option<&RunDto>,
) -> Option<TrendDto> {
    let reference = reference?;
    let now = min_price_now?;
    let then = min_price_then?;
    if then <= 0.0 {
        return None;
    }
    let delta = round2(now - then);
    let direction = if delta.abs() < 0.005 {
        TrendDirection::Flat
    } else if delta > 0.0 {
        TrendDirection::Up
    } else {
        TrendDirection::Down
    };
    Some(TrendDto {
        reference_run_id: reference.id,
        reference_at: reference.started_at,
        min_price_then: then,
        delta,
        delta_pct: round2(delta / then * 100.0),
        direction,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::metadata::test_run;
    use rstest::rstest;

    #[rstest]
    #[case(5.99, 6.49, -0.5, TrendDirection::Down)]
    #[case(6.99, 6.49, 0.5, TrendDirection::Up)]
    #[case(6.49, 6.49, 0.0, TrendDirection::Flat)]
    #[case(6.494, 6.49, 0.0, TrendDirection::Flat)]
    fn computes_direction(
        #[case] now: f64,
        #[case] then: f64,
        #[case] delta: f64,
        #[case] direction: TrendDirection,
    ) {
        let run = test_run();
        let trend = compute_trend(Some(now), Some(then), Some(&run)).unwrap();
        assert_eq!(trend.delta, delta);
        assert_eq!(trend.direction, direction);
        assert_eq!(trend.reference_run_id, run.id);
        assert_eq!(trend.reference_at, run.started_at);
        assert_eq!(trend.min_price_then, then);
    }

    #[test]
    fn delta_pct_is_relative_to_reference() {
        let run = test_run();
        let trend = compute_trend(Some(5.99), Some(6.49), Some(&run)).unwrap();
        assert!((trend.delta_pct - -7.7).abs() < 0.01, "{}", trend.delta_pct);
    }

    #[test]
    fn missing_inputs_yield_none() {
        let run = test_run();
        assert!(compute_trend(None, Some(1.0), Some(&run)).is_none());
        assert!(compute_trend(Some(1.0), None, Some(&run)).is_none());
        assert!(compute_trend(Some(1.0), Some(1.0), None).is_none());
        assert!(compute_trend(Some(1.0), Some(0.0), Some(&run)).is_none());
    }
}
