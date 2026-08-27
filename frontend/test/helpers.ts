// Mount helpers for component specs: Quasar plugin, a fresh Pinia per test and a memory router
// with the app's named routes.
import { installQuasarPlugin } from '@quasar/quasar-app-extension-testing-unit-vitest';
import { config } from '@vue/test-utils';
import { createPinia, setActivePinia, type Pinia } from 'pinia';
import { afterAll, beforeAll, beforeEach } from 'vitest';
import { defineComponent, h } from 'vue';
import { createMemoryHistory, createRouter, type Router, type RouterOptions } from 'vue-router';

const Stub = defineComponent({ render: () => h('div') });

export function createTestRouter(
  options: Partial<Omit<RouterOptions, 'history' | 'routes'>> = {},
): Router {
  return createRouter({
    ...options,
    history: createMemoryHistory(),
    routes: [
      { path: '/', name: 'index', component: Stub },
      { path: '/sorte/:id(\\d+)', name: 'strain', component: Stub },
    ],
  });
}

/** Creates a Pinia instance and makes it the active one (for stores used outside components). */
export function createTestPinia(): Pinia {
  const pinia = createPinia();
  setActivePinia(pinia);
  return pinia;
}

/**
 * Registers Quasar (via the official AE helper), a router and – fresh for every test – a Pinia
 * instance for every mount in the spec file.
 */
export function installTestPlugins(): void {
  installQuasarPlugin();
  let router: Router;
  let pinia: Pinia | null = null;
  beforeAll(() => {
    router = createTestRouter();
    config.global.plugins.push(router);
  });
  beforeEach(() => {
    if (pinia) config.global.plugins = config.global.plugins.filter((plugin) => plugin !== pinia);
    pinia = createTestPinia();
    config.global.plugins.push(pinia);
  });
  afterAll(() => {
    config.global.plugins = config.global.plugins.filter(
      (plugin) => plugin !== router && plugin !== pinia,
    );
  });
}
