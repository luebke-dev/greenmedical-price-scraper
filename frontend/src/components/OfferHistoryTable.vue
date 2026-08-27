<template>
  <div class="offer-history" :aria-busy="loading ? 'true' : 'false'">
    <div class="offer-history-toolbar">
      <q-toggle
        :model-value="state.mode === 'all'"
        dense
        color="primary"
        :label="state.mode === 'all' ? de.offerHistory.allRuns : de.offerHistory.changesOnly"
        :aria-label="de.offerHistory.modeAria"
        data-testid="offer-history-mode"
        @update:model-value="(value) => setMode(value ? 'all' : 'changes')"
      />
    </div>

    <EmptyState
      v-if="error"
      :message="error"
      tone="error"
      :retry-label="de.offerHistory.retry"
      @retry="reload"
    />
    <EmptyState v-else-if="!page && loading" :message="de.history.loading" />
    <EmptyState v-else-if="!page || page.total === 0" :message="de.offerHistory.empty" />

    <template v-else>
      <TablePager
        position="top"
        :page="state.page"
        :size="state.size"
        :total="page.total"
        :sizes="OFFER_HISTORY_SIZES"
        :noun="de.offerHistory.noun"
        @update:page="setPage"
        @update:size="setSize"
      />
      <q-table
        class="offer-history-table"
        :class="{ 'is-loading': loading }"
        :rows="rows"
        :columns="columns"
        :row-key="rowKey"
        v-model:pagination="pagination"
        :rows-per-page-options="OFFER_HISTORY_SIZES"
        :loading="loading"
        :rows-per-page-label="de.pager.perPage"
        :pagination-label="de.pager.tableLabel"
        hide-selected-banner
        flat
        square
        table-class="offers"
        @request="onRequest"
      >
        <template #header>
          <tr>
            <template v-if="page.mode === 'changes'">
              <th scope="col">{{ de.offerHistory.from }}</th>
              <th scope="col">{{ de.offerHistory.to }}</th>
            </template>
            <th v-else scope="col">{{ de.offerHistory.date }}</th>
            <th scope="col">{{ de.offers.pharmacy }}</th>
            <th scope="col">{{ de.offers.city }}</th>
            <th scope="col">{{ de.offers.price }}</th>
            <th scope="col">{{ de.offers.thcPrice }}</th>
            <th scope="col">{{ de.offers.status }}</th>
          </tr>
        </template>

        <template #body="props">
          <tr
            v-if="isPhase(props.row)"
            :class="{ delisted: props.row.delisted }"
            :data-key="rowKey(props.row)"
          >
            <td class="date">{{ formatHistoryAt(props.row.from, page.bucket) }}</td>
            <td class="date">
              <span v-if="props.row.to === null" class="current">{{
                de.offerHistory.current
              }}</span>
              <template v-else>{{ formatHistoryAt(props.row.to, page.bucket) }}</template>
              <span class="runs">({{ de.offerHistory.runs(props.row.runs) }})</span>
            </td>
            <td>{{ props.row.pharmacy }}</td>
            <td>{{ props.row.city }}</td>
            <td class="price">{{ props.row.delisted ? '' : euro(props.row.price, '€/g') }}</td>
            <td class="price">
              {{ props.row.delisted ? '' : euro(props.row.price_per_thc_gram, '€/g THC') }}
            </td>
            <td>
              <span v-if="props.row.delisted" class="status delisted">{{
                de.offerHistory.delisted
              }}</span>
              <StatusBadge v-else-if="props.row.availability" :value="props.row.availability" />
            </td>
          </tr>
          <tr
            v-else
            :class="{ 'run-start': isRunStart(props.rowIndex) }"
            :data-key="rowKey(props.row)"
          >
            <td class="date">{{ formatHistoryAt(props.row.at, page.bucket) }}</td>
            <td>{{ props.row.pharmacy }}</td>
            <td>{{ props.row.city }}</td>
            <td class="price">{{ euro(props.row.price, '€/g') }}</td>
            <td class="price">{{ euro(props.row.price_per_thc_gram, '€/g THC') }}</td>
            <td><StatusBadge v-if="props.row.availability" :value="props.row.availability" /></td>
          </tr>
        </template>
      </q-table>
    </template>
  </div>
</template>

<script setup lang="ts">
import type { QTableProps } from 'quasar';
import { computed, toRef, watch } from 'vue';
import type { OfferHistoryRow, OfferPhaseRow } from '@/api/types';
import { useOfferHistory } from '@/composables/useOfferHistory';
import { de } from '@/i18n/de';
import { euro } from '@/lib/format';
import { OFFER_HISTORY_SIZES, formatHistoryAt, type HistoryPreset } from '@/lib/history';
import { useCatalogStore } from '@/stores/catalog';
import EmptyState from './EmptyState.vue';
import StatusBadge from './StatusBadge.vue';
import TablePager from './TablePager.vue';

type Row = OfferHistoryRow | OfferPhaseRow;

const props = defineProps<{
  strainId: number | null;
  preset: HistoryPreset;
  /** Fixed reference time for the range (tests). */
  now?: Date | undefined;
}>();

const { state, page, loading, error, reload, setMode, setPage, setSize } = useOfferHistory(
  toRef(props, 'strainId'),
  toRef(props, 'preset'),
  props.now,
);

// New scrape run: the current page is refetched.
const catalog = useCatalogStore();
watch(
  () => catalog.runChanged,
  () => void reload(),
);

const rows = computed<Row[]>(() => page.value?.rows ?? []);

function isPhase(row: Row): row is OfferPhaseRow {
  return 'from' in row;
}

function rowKey(row: Row): string {
  return isPhase(row) ? `${row.pharmacy_id}|${row.from}` : `${row.at}|${row.pharmacy_id}`;
}

/** First row of a run/day group (visual separation in mode=all). */
function isRunStart(index: number): boolean {
  const row = rows.value[index];
  const previous = rows.value[index - 1];
  if (!row || isPhase(row)) return false;
  return index === 0 || !previous || isPhase(previous) || previous.at !== row.at;
}

const columns = computed<NonNullable<QTableProps['columns']>>(() => {
  const names =
    page.value?.mode === 'changes'
      ? ['from', 'to', 'pharmacy', 'city', 'price', 'price_per_thc_gram', 'availability']
      : ['at', 'pharmacy', 'city', 'price', 'price_per_thc_gram', 'availability'];
  return names.map((name) => ({ name, label: name, field: name, align: 'left', sortable: false }));
});

type Pagination = NonNullable<QTableProps['pagination']>;

const pagination = computed<Pagination>({
  get: () => ({
    page: state.page,
    rowsPerPage: state.size,
    rowsNumber: page.value?.total ?? 0,
  }),
  set: () => {},
});

function onRequest(request: { pagination: Pagination }): void {
  const next = request.pagination;
  if (next.rowsPerPage && next.rowsPerPage !== state.size) setSize(next.rowsPerPage);
  else if (next.page !== undefined && next.page !== state.page) setPage(next.page);
}
</script>
