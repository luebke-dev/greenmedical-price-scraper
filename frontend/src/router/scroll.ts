import type { RouterScrollBehavior } from 'vue-router';

/**
 * Scroll handling for route changes.
 *
 * - History navigation (back/forward) restores the saved position, so returning from
 *   `/sorte/:id` to the kept-alive overview lands where the user left off.
 * - Query-only navigations keep the viewport where it is: the overview mirrors every
 *   search/filter/sort change into the URL via `router.replace`, and the legacy site never
 *   moved the viewport on those (a sticky sort header must not jump the page to the top).
 * - A real page change (different path) starts at the top.
 */
export const scrollBehavior: RouterScrollBehavior = (to, from, savedPosition) => {
  if (savedPosition) return savedPosition;
  if (to.path === from.path) return false;
  return { left: 0, top: 0 };
};
