import { describe, expect, it, vi } from 'vitest';
import {
  SESSION_DELAY_MS,
  listingUrlFor,
  openProduct,
  withoutFragment,
  type BuyWindow,
} from '@/lib/buy';

const PRODUCT =
  'https://greenmedical.health/de/cannabis/flower/x-y?deliveryTarget=cGhhcm1hY3k6fDphYmM%3D#reviews';

function fakeWindow(tab: { location: { href: string }; opener: unknown } | null) {
  const open = vi.fn(() => tab);
  const win: BuyWindow & {
    scheduled: { fn: () => void; ms: number }[];
    navigated: string[];
    openSpy: typeof open;
  } = {
    scheduled: [],
    navigated: [],
    openSpy: open,
    open,
    navigate: (url) => win.navigated.push(url),
    schedule: (fn, ms) => win.scheduled.push({ fn, ms }),
  };
  return win;
}

describe('listingUrlFor', () => {
  it('builds the flower listing URL carrying the same delivery target', () => {
    expect(listingUrlFor(PRODUCT)).toBe(
      'https://greenmedical.health/de/cannabis/flowers?deliveryTarget=cGhhcm1hY3k6fDphYmM%3D&onlyShowIfAvailable=1',
    );
  });

  it('returns null without a target or for invalid URLs', () => {
    expect(listingUrlFor('https://greenmedical.health/de/cannabis/flower/x')).toBeNull();
    expect(listingUrlFor('not a url')).toBeNull();
  });
});

describe('openProduct', () => {
  it('opens the listing first, detaches the opener and redirects to the product later', () => {
    const tab = { location: { href: '' }, opener: {} };
    const win = fakeWindow(tab);
    openProduct(PRODUCT, win);
    expect(win.openSpy).toHaveBeenCalledWith(listingUrlFor(PRODUCT));
    expect(tab.opener).toBeNull();
    expect(win.navigated).toEqual([]);
    expect(win.scheduled).toHaveLength(1);
    expect(win.scheduled[0]!.ms).toBe(SESSION_DELAY_MS);
    win.scheduled[0]!.fn();
    expect(tab.location.href).toBe(withoutFragment(PRODUCT));
    expect(tab.location.href.endsWith('#reviews')).toBe(false);
  });

  it('falls back to a plain navigation when the popup is blocked', () => {
    const win = fakeWindow(null);
    openProduct(PRODUCT, win);
    expect(win.navigated).toEqual([withoutFragment(PRODUCT)]);
  });

  it('navigates directly when the URL has no delivery target', () => {
    const win = fakeWindow({ location: { href: '' }, opener: {} });
    openProduct('https://greenmedical.health/de/cannabis/flower/x', win);
    expect(win.openSpy).not.toHaveBeenCalled();
    expect(win.navigated).toEqual(['https://greenmedical.health/de/cannabis/flower/x']);
  });
});

describe('withoutFragment', () => {
  it('strips #reviews and leaves other URLs untouched', () => {
    expect(withoutFragment('https://x.test/p?a=1#reviews')).toBe('https://x.test/p?a=1');
    expect(withoutFragment('https://x.test/p?a=1')).toBe('https://x.test/p?a=1');
  });
});
