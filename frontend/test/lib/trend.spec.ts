import { describe, expect, it } from 'vitest';
import { MINUS } from '@/lib/format';
import {
  TREND_GLYPH,
  trendAriaLabel,
  trendClass,
  trendDays,
  trendLabel,
  trendTooltip,
} from '@/lib/trend';
import { LATEST_AT, makeTrend } from '../fixtures';

describe('trend', () => {
  it('has one glyph per direction', () => {
    expect(TREND_GLYPH).toEqual({ up: '▲', down: '▼', flat: '–' });
    expect(trendClass('up')).toBe('trend-up');
    expect(trendLabel('down')).toBe('Preis gefallen');
  });

  it('counts whole days between reference and latest run', () => {
    expect(trendDays(makeTrend({ reference_at: '2026-08-20T19:56:20Z' }), LATEST_AT)).toBe(7);
    expect(trendDays(makeTrend({ reference_at: '2026-08-19T04:00:00Z' }), LATEST_AT)).toBe(9);
    expect(trendDays(makeTrend({ reference_at: '2026-08-27T10:00:00Z' }), LATEST_AT)).toBe(1);
    expect(trendDays(makeTrend({ reference_at: 'nope' }), LATEST_AT)).toBe(7);
  });

  it('renders the tooltip exactly as specified', () => {
    const trend = makeTrend({
      min_price_then: 6.49,
      delta: -0.5,
      delta_pct: -7.7,
      direction: 'down',
    });
    expect(trendTooltip(trend, LATEST_AT)).toBe(
      `vor 7 Tagen: 6,49 €/g (${MINUS}0,50 €, ${MINUS}7,7 %)`,
    );
  });

  it('handles increases, flat prices and the singular day', () => {
    expect(
      trendTooltip(
        makeTrend({ min_price_then: 5.99, delta: 0.5, delta_pct: 8.35, direction: 'up' }),
        LATEST_AT,
      ),
    ).toBe('vor 7 Tagen: 5,99 €/g (+0,50 €, +8,4 %)');
    expect(
      trendTooltip(
        makeTrend({ min_price_then: 5.99, delta: 0, delta_pct: 0, direction: 'flat' }),
        LATEST_AT,
      ),
    ).toBe('vor 7 Tagen: 5,99 €/g (±0,00 €, ±0,0 %)');
    expect(trendTooltip(makeTrend({ reference_at: '2026-08-27T02:00:00Z' }), LATEST_AT)).toMatch(
      /^vor 1 Tag: /,
    );
  });

  it('builds a screen reader label with direction and tooltip', () => {
    expect(trendAriaLabel(makeTrend(), LATEST_AT)).toBe(
      `Preis gefallen, vor 7 Tagen: 6,49 €/g (${MINUS}0,50 €, ${MINUS}7,7 %)`,
    );
  });
});
