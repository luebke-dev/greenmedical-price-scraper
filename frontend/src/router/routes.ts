import type { RouteRecordRaw } from 'vue-router';

const routes: RouteRecordRaw[] = [
  {
    path: '/',
    component: () => import('@/layouts/MainLayout.vue'),
    children: [
      { path: '', name: 'index', component: () => import('@/pages/IndexPage.vue') },
      {
        // Integer DB id only; anything else falls through to the 404 page.
        path: 'sorte/:id(\\d+)',
        name: 'strain',
        component: () => import('@/pages/StrainPage.vue'),
        props: (route) => ({ id: Number(route.params.id) }),
      },
      { path: 'abo', name: 'subscribe', component: () => import('@/pages/SubscribePage.vue') },
      {
        path: 'abo/bestaetigen',
        name: 'subscribe-confirm',
        component: () => import('@/pages/ConfirmPage.vue'),
      },
      {
        path: 'abo/verwalten',
        name: 'subscribe-manage',
        component: () => import('@/pages/ManagePage.vue'),
      },
      // Always leave this as last one.
      {
        path: ':catchAll(.*)*',
        name: 'not-found',
        component: () => import('@/pages/ErrorNotFound.vue'),
      },
    ],
  },
];

export default routes;
