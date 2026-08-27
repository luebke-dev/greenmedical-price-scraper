// Configuration for your app
// https://v2.quasar.dev/quasar-cli-vite/quasar-config-file

import { fileURLToPath } from 'node:url';
import { defineConfig } from '#q-app';
import { mockApiPlugin } from './dev/mock-api';

const DEFAULT_PROXY_TARGET = 'http://localhost:8080';

function isEnabled(value: string | undefined): boolean {
  return value !== undefined && value !== '' && value !== '0' && value.toLowerCase() !== 'false';
}

export default defineConfig((ctx) => {
  // `pnpm dev` proxies /api to the backend (API_PROXY_TARGET, default localhost:8080).
  // `MOCK_API=1 pnpm dev` (= `pnpm dev:mock`) answers /api/v1/* from dev/fixtures instead.
  const useMockApi = ctx.dev && isEnabled(process.env.MOCK_API);
  const proxyTarget = process.env.API_PROXY_TARGET || DEFAULT_PROXY_TARGET;

  return {
    boot: [],

    css: ['app.scss'],

    // No icon font / Roboto: icons come from @quasar/extras/mdi-v7 (SVG), fonts from the system.
    extras: [],

    build: {
      target: {
        browser: 'baseline-widely-available',
        node: 'node22',
      },

      typescript: {
        strict: true,
        vueShim: true,
      },

      vueRouterMode: 'history',

      vitePlugins: [
        [
          'vite-plugin-checker',
          {
            vueTsc: true,
            eslint: {
              lintCommand:
                'eslint -c ./eslint.config.js "./{src,dev,test}/**/*.{ts,js,mjs,cjs,vue}"',
              useFlatConfig: true,
            },
          },
          { server: false },
        ],
        useMockApi
          ? [
              mockApiPlugin,
              { fixturesDir: fileURLToPath(new URL('./dev/fixtures', import.meta.url)) },
              { server: false },
            ]
          : null,
      ],
    },

    devServer: {
      open: false,
      port: 9000,
      proxy: useMockApi
        ? {}
        : {
            '/api': {
              target: proxyTarget,
              changeOrigin: true,
            },
          },
    },

    framework: {
      config: {
        dark: 'auto',
      },
      iconSet: 'svg-mdi-v7',
      lang: 'de-DE',
      plugins: [],
    },

    animations: [],
  };
});
