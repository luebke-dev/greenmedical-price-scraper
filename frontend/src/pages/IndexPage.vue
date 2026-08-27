<template>
  <q-page class="index-page">
    <MetricCards
      :metadata="catalog.metadata"
      :loading="!catalog.metadata && !catalog.metadataError"
    />

    <FilterToolbar
      v-model="filters.searchInput.value"
      :count="filters.count.value"
      :open="filtersOpen"
      :panel-id="FILTER_PANEL_ID"
      @toggle="filtersOpen = !filtersOpen"
      @reset="filters.reset()"
    />

    <FilterPanel
      :id="FILTER_PANEL_ID"
      :open="filtersOpen"
      :genetik="filters.genetik.value"
      :selected-genetik="filters.state.genetik"
      :bounds="filters.bounds.value"
      :ranges="filters.state.ranges"
      @toggle-genetik="filters.toggleGenetik"
      @update-range="filters.setRange"
    />

    <StrainTable
      :rows="filters.rows.value"
      :sort="filters.state.sort"
      :page="filters.state.page"
      :size="filters.state.size"
      :total="filters.count.value"
      :latest-at="catalog.latestAt"
      :loading="catalog.loading"
      :error="catalog.error"
      @sort="filters.setSort"
      @paginate="onPaginate"
      @retry="catalog.refresh()"
    />
  </q-page>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue';
import FilterPanel from '@/components/FilterPanel.vue';
import FilterToolbar from '@/components/FilterToolbar.vue';
import MetricCards from '@/components/MetricCards.vue';
import StrainTable from '@/components/StrainTable.vue';
import { useStrainFilters } from '@/composables/useStrainFilters';
import type { PageRequest } from '@/lib/url-state';
import { useCatalogStore } from '@/stores/catalog';

// Name is required for <keep-alive include="IndexPage">.
defineOptions({ name: 'IndexPage' });

const FILTER_PANEL_ID = 'filters';

const catalog = useCatalogStore();
const filters = useStrainFilters();
const filtersOpen = ref(false);

// A new scrape run has landed: reload the current page with the current parameters.
watch(
  () => catalog.runChanged,
  () => void catalog.refresh(),
);

function onPaginate(request: PageRequest): void {
  if (request.size !== filters.state.size) filters.setSize(request.size);
  else filters.setPage(request.page);
}
</script>
