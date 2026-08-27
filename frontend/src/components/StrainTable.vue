<template>
  <section
    class="table-wrap"
    :aria-label="de.table.heading"
    :aria-busy="loading ? 'true' : 'false'"
  >
    <TablePager
      class="strain-pager"
      position="top"
      :page="page"
      :size="size"
      :total="total"
      :noun="de.toolbar.noun"
      @update:page="(value) => emit('paginate', { page: value, size })"
      @update:size="(value) => emit('paginate', { page, size: value })"
    />
    <q-table
      class="strain-table"
      :rows="rows"
      :columns="tableColumns"
      row-key="id"
      v-model:pagination="pagination"
      :rows-per-page-options="PAGE_SIZES"
      :loading="loading"
      :rows-per-page-label="de.pager.perPage"
      :pagination-label="de.pager.tableLabel"
      hide-selected-banner
      flat
      square
      table-class="strains"
      @request="onRequest"
    >
      <template #header>
        <tr>
          <th
            v-for="column in COLUMNS"
            :key="column.key"
            scope="col"
            :style="{ width: column.width }"
            :aria-sort="ariaSort(sort, column.key)"
            :data-key="column.key"
          >
            <button
              type="button"
              :aria-label="de.table.sortBy(column.label)"
              :data-key="column.key"
              @click="emit('sort', column.key)"
            >
              <span>{{ column.label }}</span>
              <span class="sort-indicator" aria-hidden="true">{{ indicator(column.key) }}</span>
            </button>
          </th>
        </tr>
      </template>

      <template #body="props">
        <tr
          class="group-row"
          :data-id="props.row.id"
          tabindex="0"
          role="link"
          :aria-label="de.table.openAria(props.row.name)"
          @click="open(props.row.id)"
          @keydown.enter.prevent="open(props.row.id)"
          @keydown.space.prevent="open(props.row.id)"
        >
          <td
            v-for="column in COLUMNS"
            :key="column.key"
            :class="column.className"
            :data-key="column.key"
          >
            <template v-if="column.key === 'name'">
              <router-link
                class="strain-name"
                :to="{ name: 'strain', params: { id: props.row.id } }"
                @click.stop
              >
                <span class="chevron" aria-hidden="true">▸</span>
                <span>{{ props.row.name || '—' }}</span>
              </router-link>
            </template>
            <template v-else-if="column.key === 'price'">
              <span>{{ fromEuro(props.row.min_price, '€/g') }}</span>
              <TrendIndicator :trend="props.row.trend" :latest-at="latestAt" />
            </template>
            <template v-else-if="column.key === 'price_per_thc_gram'">
              {{ fromEuro(props.row.min_price_per_thc_gram, '€/g THC') }}
            </template>
            <template v-else-if="column.key === 'pharmacy_count'">
              {{ integer(props.row.pharmacy_count) }}
            </template>
            <template v-else-if="column.key === 'rating'">
              <span
                v-if="ratingCompact(props.row.rating?.value, props.row.rating?.count)"
                class="rating-compact"
                :aria-label="
                  de.rating.compactAria(
                    rating(props.row.rating?.value),
                    props.row.rating?.count ?? 0,
                  )
                "
              >
                <span class="star-glyph" aria-hidden="true">★</span>
                <span aria-hidden="true">{{
                  ` ${ratingCompact(props.row.rating?.value, props.row.rating?.count)}`
                }}</span>
              </span>
            </template>
            <template v-else>{{ textCell(props.row, column.key) }}</template>
          </td>
        </tr>
      </template>

      <template #no-data>
        <EmptyState
          :message="statusMessage"
          :tone="error ? 'error' : 'status'"
          :retry-label="error ? de.table.retry : null"
          @retry="emit('retry')"
        />
      </template>
    </q-table>
  </section>
</template>

<script setup lang="ts">
import type { QTableProps } from 'quasar';
import { computed } from 'vue';
import { useRouter } from 'vue-router';
import type { StrainListItem } from '@/api/types';
import { de } from '@/i18n/de';
import { fromEuro, integer, rating, ratingCompact } from '@/lib/format';
import { COLUMNS, ariaSort, isSortKey, type SortKey, type SortState } from '@/lib/sort';
import { PAGE_SIZES, type PageRequest } from '@/lib/url-state';
import EmptyState from './EmptyState.vue';
import TablePager from './TablePager.vue';
import TrendIndicator from './TrendIndicator.vue';

const props = defineProps<{
  rows: readonly StrainListItem[];
  sort: SortState;
  /** 1-based page. */
  page: number;
  size: number;
  /** Total hits (rowsNumber). */
  total: number;
  latestAt?: string | null | undefined;
  loading?: boolean | undefined;
  error?: string | null | undefined;
}>();

const emit = defineEmits<{ sort: [key: SortKey]; paginate: [request: PageRequest]; retry: [] }>();

const router = useRouter();

function open(id: number): void {
  void router.push({ name: 'strain', params: { id } });
}

type Pagination = NonNullable<QTableProps['pagination']>;

/** Server-side pagination: q-table shows the rows as they are and reports changes via @request. */
const pagination = computed<Pagination>({
  get: () => ({
    page: props.page,
    rowsPerPage: props.size,
    rowsNumber: props.total,
    sortBy: props.sort.key,
    descending: props.sort.direction === 'desc',
  }),
  // The parent owns the state; it comes back through the props after @request.
  set: () => {},
});

function onRequest(request: { pagination: Pagination }): void {
  const next = request.pagination;
  const page = next.page ?? props.page;
  const size = next.rowsPerPage || props.size;
  if (page !== props.page || size !== props.size) emit('paginate', { page, size });
  const sortBy = next.sortBy ?? null;
  if (sortBy !== null && sortBy !== props.sort.key && isSortKey(sortBy)) emit('sort', sortBy);
}

const tableColumns = computed<NonNullable<QTableProps['columns']>>(() =>
  COLUMNS.map((column) => ({
    name: column.key,
    label: column.label,
    field: column.key,
    align: 'left',
    sortable: false,
  })),
);

const statusMessage = computed(() => {
  if (props.error) return props.error;
  if (props.loading) return de.table.loading;
  return de.table.empty;
});

function indicator(key: SortKey): string {
  if (props.sort.key !== key) return '';
  return props.sort.direction === 'asc' ? '^' : 'v';
}

function textCell(row: StrainListItem, key: SortKey): string {
  switch (key) {
    case 'bezeichnung':
      return row.bezeichnung || '';
    case 'genetik':
      return row.genetik || '';
    case 'thc':
      return row.thc || '';
    case 'cbd':
      return row.cbd || '';
    default:
      return '';
  }
}
</script>
