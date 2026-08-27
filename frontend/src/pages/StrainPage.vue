<template>
  <q-page class="strain-page">
    <nav class="back-nav">
      <router-link class="back-link" :to="navigation.indexLocation">{{
        de.strain.back
      }}</router-link>
    </nav>

    <EmptyState v-if="error" :message="error" tone="error" />
    <EmptyState v-else-if="!detail" :message="de.strain.loading" />

    <template v-else>
      <section class="facts" :aria-label="de.strain.facts">
        <div class="facts-title">
          <h2 class="facts-name">{{ detail.name || '—' }}</h2>
          <div v-if="detail.bezeichnung" class="facts-sub">{{ detail.bezeichnung }}</div>
        </div>
        <dl class="facts-grid">
          <div class="fact">
            <dt>{{ de.strain.price }}</dt>
            <dd class="price">
              <span>{{ euro(detail.min_price, '€/g') || '–' }}</span>
              <TrendIndicator :trend="detail.trend" :latest-at="latestAt" />
            </dd>
          </div>
          <div class="fact">
            <dt>{{ de.strain.thcPrice }}</dt>
            <dd class="price">{{ euro(detail.min_price_per_thc_gram, '€/g THC') || '–' }}</dd>
          </div>
          <div class="fact">
            <dt>{{ de.strain.genetik }}</dt>
            <dd>{{ detail.genetik || '–' }}</dd>
          </div>
          <div class="fact">
            <dt>{{ de.strain.thc }}</dt>
            <dd>{{ detail.thc || '–' }}</dd>
          </div>
          <div class="fact">
            <dt>{{ de.strain.cbd }}</dt>
            <dd>{{ detail.cbd || '–' }}</dd>
          </div>
          <div class="fact">
            <dt>{{ de.strain.pharmacies }}</dt>
            <dd>{{ integer(detail.pharmacy_count) }}</dd>
          </div>
          <div class="fact">
            <dt>{{ de.strain.firstSeen }}</dt>
            <dd>{{ dateOnly(detail.first_seen_at) || '–' }}</dd>
          </div>
          <div class="fact">
            <dt>{{ de.strain.lastSeen }}</dt>
            <dd>{{ dateTime(detail.last_seen_at) || '–' }}</dd>
          </div>
        </dl>
        <p v-if="!detail.in_latest_run" class="notice" role="status">
          {{ de.strain.notInLatestRun }}
        </p>
      </section>

      <section class="history" :aria-label="de.history.heading">
        <div class="section-head">
          <h3>{{ de.history.heading }}</h3>
          <HistoryControls
            v-model:preset="preset"
            v-model:thc-mode="thcMode"
            v-model:pharmacies="pharmacies"
          />
        </div>
        <div
          class="history-body"
          :class="{ 'is-loading': historyLoading }"
          :aria-busy="historyLoading ? 'true' : 'false'"
        >
          <EmptyState v-if="historyError" :message="historyError" tone="error" />
          <template v-else-if="series && tableRows.length > 0">
            <PriceHistoryChart :series="series" :rows="tableRows" :label="chartAriaLabel" />
            <!-- A new range/toggle request in flight: keep the layout, dim the stale chart. -->
            <div v-if="historyLoading" class="history-overlay" role="status">
              {{ de.history.loading }}
            </div>
          </template>
          <EmptyState v-else-if="historyLoading" :message="de.history.loading" />
          <EmptyState v-else :message="de.history.empty" />
        </div>
      </section>

      <ReviewsSection v-if="strainId !== null" :strain-id="strainId" />

      <section class="strain-offers" :aria-label="de.strain.offersHeading">
        <div class="section-head">
          <h3>{{ de.strain.offersHeading }}</h3>
        </div>
        <div class="table-wrap">
          <OffersTable :offers="detail.offers" />
        </div>
      </section>
      <section class="strain-offers offer-history-section" :aria-label="de.offerHistory.heading">
        <div class="section-head">
          <h3>{{ de.offerHistory.heading }}</h3>
        </div>
        <div class="table-wrap" :aria-busy="offerHistoryLoading ? 'true' : 'false'">
          <EmptyState v-if="offerHistoryError" :message="offerHistoryError" tone="error" />
          <OfferHistoryTable
            v-else-if="offerHistory && (offerHistory.pharmacies?.length ?? 0) > 0"
            :history="offerHistory"
          />
          <EmptyState v-else-if="offerHistoryLoading" :message="de.history.loading" />
          <EmptyState v-else :message="de.offerHistory.empty" />
        </div>
      </section>
    </template>
  </q-page>
</template>

<script setup lang="ts">
import {
  computed,
  defineAsyncComponent,
  defineComponent,
  h,
  onBeforeUnmount,
  ref,
  shallowRef,
  watch,
} from 'vue';
import type { StrainDetail } from '@/api/types';
import EmptyState from '@/components/EmptyState.vue';
import HistoryControls from '@/components/HistoryControls.vue';
import OfferHistoryTable from '@/components/OfferHistoryTable.vue';
import OffersTable from '@/components/OffersTable.vue';
import ReviewsSection from '@/components/ReviewsSection.vue';
import TrendIndicator from '@/components/TrendIndicator.vue';
import { useHistoryQuery } from '@/composables/useHistoryQuery';
import { de } from '@/i18n/de';
import { dateOnly, dateTime, euro, integer } from '@/lib/format';
import {
  DEFAULT_PRESET,
  buildSeries,
  historyTableRows,
  seriesAriaLabel,
  type HistoryPreset,
} from '@/lib/history';
import { strainErrorMessage, useCatalogStore } from '@/stores/catalog';
import { useNavigationStore } from '@/stores/navigation';

defineOptions({ name: 'StrainPage' });

// ECharts lives in its own chunk; it is only downloaded on this page.
const ChartLoading = defineComponent({
  inheritAttrs: false,
  render: () => h(EmptyState, { message: de.history.loading }),
});
const ChartError = defineComponent({
  inheritAttrs: false,
  render: () => h(EmptyState, { message: de.history.loadError, tone: 'error' }),
});
const PriceHistoryChart = defineAsyncComponent({
  loader: () => import('@/components/PriceHistoryChart.vue'),
  loadingComponent: ChartLoading,
  errorComponent: ChartError,
  delay: 100,
  // A stale chunk after a deployment (or Vite's dep re-optimisation) gets one retry.
  onError(error, retry, fail, attempts) {
    if (attempts <= 1) retry();
    else fail();
  },
});

const props = defineProps<{ id: number }>();

const catalog = useCatalogStore();
const navigation = useNavigationStore();
const detail = shallowRef<StrainDetail | null>(null);
const error = ref<string | null>(null);

const strainId = computed(() => (Number.isInteger(props.id) && props.id > 0 ? props.id : null));
const latestAt = computed(
  () => detail.value?.run.finished_at ?? detail.value?.run.started_at ?? catalog.latestAt,
);

let controller: AbortController | null = null;

watch(
  strainId,
  async (id) => {
    controller?.abort();
    detail.value = null;
    error.value = null;
    if (id === null) {
      error.value = de.strain.notFound;
      return;
    }
    controller = new AbortController();
    const signal = controller.signal;
    try {
      const result = await catalog.loadDetail(id, signal);
      if (signal.aborted) return;
      detail.value = result;
    } catch (cause) {
      if (signal.aborted) return;
      error.value = strainErrorMessage(cause);
    }
  },
  { immediate: true },
);

onBeforeUnmount(() => controller?.abort());

const preset = ref<HistoryPreset>(DEFAULT_PRESET);
const thcMode = ref(false);
const pharmacies = ref(false);

const {
  history,
  loading: historyLoading,
  error: historyError,
} = useHistoryQuery(strainId, preset, pharmacies);

// The offer history always needs the per-pharmacy series, independent of the chart toggle.
const withPharmacies = ref(true);
const {
  history: offerHistory,
  loading: offerHistoryLoading,
  error: offerHistoryError,
} = useHistoryQuery(strainId, preset, withPharmacies);

const series = computed(() =>
  history.value
    ? buildSeries(history.value, { thcMode: thcMode.value, pharmacies: pharmacies.value })
    : null,
);
const tableRows = computed(() =>
  history.value ? historyTableRows(history.value, thcMode.value) : [],
);
const chartAriaLabel = computed(() =>
  series.value ? seriesAriaLabel(detail.value?.name ?? '', series.value) : '',
);
</script>
