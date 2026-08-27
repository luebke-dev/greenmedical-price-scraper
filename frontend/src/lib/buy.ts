// greenmedical.health ignores `deliveryTarget` on product pages: the pharmacy shown there comes
// from the PHP session, which is only switched by visiting the flower listing with that target.
// Opening the listing first and then redirecting the same tab to the product page therefore lands
// on the product with the correct pharmacy selected.

const LISTING_PATH = '/de/cannabis/flowers';
export const SESSION_DELAY_MS = 1800;

/** Drops a URL fragment such as "#reviews" that older scrapes stored on product URLs. */
export function withoutFragment(url: string): string {
  const hash = url.indexOf('#');
  return hash === -1 ? url : url.slice(0, hash);
}

/** Returns the listing URL that selects the offer's pharmacy, or null if the URL has no target. */
export function listingUrlFor(productUrl: string): string | null {
  let url: URL;
  try {
    url = new URL(productUrl);
  } catch {
    return null;
  }
  const target = url.searchParams.get('deliveryTarget');
  if (!target) return null;
  const listing = new URL(LISTING_PATH, url.origin);
  listing.searchParams.set('deliveryTarget', target);
  listing.searchParams.set('onlyShowIfAvailable', '1');
  return listing.toString();
}

export interface BuyWindow {
  open(this: void, url: string): { location: { href: string }; opener: unknown } | null;
  navigate(this: void, url: string): void;
  schedule(this: void, fn: () => void, ms: number): void;
}

const browserWindow: BuyWindow = {
  open: (url) => window.open(url, '_blank'),
  navigate: (url) => {
    window.location.href = url;
  },
  schedule: (fn, ms) => {
    window.setTimeout(fn, ms);
  },
};

/**
 * Opens the product page in a new tab with the offer's pharmacy selected. Falls back to a plain
 * navigation when there is no delivery target or the popup was blocked.
 */
export function openProduct(rawUrl: string, win: BuyWindow = browserWindow): void {
  const productUrl = withoutFragment(rawUrl);
  const listing = listingUrlFor(productUrl);
  if (!listing) {
    win.navigate(productUrl);
    return;
  }
  const tab = win.open(listing);
  if (!tab) {
    win.navigate(productUrl);
    return;
  }
  tab.opener = null; // no reverse-tabnabbing; we keep our own handle for the redirect
  win.schedule(() => {
    tab.location.href = productUrl;
  }, SESSION_DELAY_MS);
}
