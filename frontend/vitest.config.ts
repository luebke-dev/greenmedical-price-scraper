import { quasarViteTestingConfig } from '@quasar/quasar-app-extension-testing-unit-vitest/config';
import { defineConfig, mergeConfig } from 'vitest/config';

// Deterministic date formatting in specs (the app itself uses the browser's zone).
process.env.TZ = 'Europe/Berlin';

// https://vitest.dev/config/
export default defineConfig(async () =>
  mergeConfig(await quasarViteTestingConfig(), {
    test: {
      environment: 'happy-dom',
      setupFiles: ['test/setup.ts'],
      include: ['test/**/*.spec.ts', 'src/**/*.spec.ts'],
      env: { TZ: 'Europe/Berlin' },
      // Only the global stylesheet is compiled (test/css); component styles stay skipped.
      css: { include: [/[\\/]src[\\/]css[\\/][^?]*\.scss(\?.*)?$/] },
    },
  }),
);
