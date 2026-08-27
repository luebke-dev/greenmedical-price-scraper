import { onScopeDispose, readonly, ref, type Ref } from 'vue';

const QUERY = '(prefers-reduced-motion: reduce)';

/** Reactive `prefers-reduced-motion: reduce` (false when matchMedia is unavailable). */
export function usePrefersReducedMotion(): Readonly<Ref<boolean>> {
  const reduced = ref(false);
  if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') {
    return readonly(reduced);
  }
  const media = window.matchMedia(QUERY);
  reduced.value = media.matches;
  const onChange = (event: MediaQueryListEvent) => {
    reduced.value = event.matches;
  };
  media.addEventListener('change', onChange);
  onScopeDispose(() => media.removeEventListener('change', onChange));
  return readonly(reduced);
}
