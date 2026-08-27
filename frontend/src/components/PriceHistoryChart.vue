<template>
  <div class="price-chart">
    <div class="chart-frame" role="img" :aria-label="label">
      <v-chart
        :key="rangeKey"
        class="chart"
        :option="option"
        :update-options="CHART_UPDATE_OPTIONS"
        autoresize
        aria-hidden="true"
      />
    </div>
    <q-expansion-item
      class="chart-table"
      dense
      :label="de.history.dataTable"
      header-class="chart-table-header"
      :expand-icon="expandIcon"
    >
      <div class="chart-table-wrap">
        <table class="offers history-table">
          <thead>
            <tr>
              <th scope="col">{{ de.history.date }}</th>
              <th scope="col">{{ de.history.min }}</th>
              <th scope="col">{{ de.history.avg }}</th>
              <th scope="col">{{ de.history.max }}</th>
              <th scope="col">{{ de.history.offers }}</th>
              <th scope="col">{{ de.history.pharmacyCount }}</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="row in rows" :key="row.at">
              <td>{{ row.at }}</td>
              <td class="price">{{ euro(row.min, series.unit) }}</td>
              <td class="price">{{ euro(row.avg, series.unit) }}</td>
              <td class="price">{{ euro(row.max, series.unit) }}</td>
              <td>{{ integer(row.offerCount) }}</td>
              <td>{{ integer(row.pharmacyCount) }}</td>
            </tr>
          </tbody>
        </table>
      </div>
    </q-expansion-item>
  </div>
</template>

<script setup lang="ts">
import { mdiChevronDown } from '@quasar/extras/mdi-v7';
import { LineChart } from 'echarts/charts';
import {
  DataZoomComponent,
  GridComponent,
  LegendComponent,
  TooltipComponent,
} from 'echarts/components';
import { use } from 'echarts/core';
import { CanvasRenderer } from 'echarts/renderers';
import { useQuasar } from 'quasar';
import VChart from 'vue-echarts';
import 'vue-echarts/style.css';
import { computed, nextTick, onMounted, shallowRef, watch } from 'vue';
import { usePrefersReducedMotion } from '@/composables/usePrefersReducedMotion';
import { de } from '@/i18n/de';
import {
  CHART_UPDATE_OPTIONS,
  buildChartOption,
  readChartTheme,
  type ChartTheme,
} from '@/lib/chart';
import { euro, integer } from '@/lib/format';
import type { HistorySeries, HistoryTableRow } from '@/lib/history';

// Tree-shaken ECharts: only what the line chart needs.
use([
  LineChart,
  GridComponent,
  TooltipComponent,
  LegendComponent,
  DataZoomComponent,
  CanvasRenderer,
]);

const props = defineProps<{
  series: HistorySeries;
  rows: readonly HistoryTableRow[];
  label: string;
}>();

const $q = useQuasar();
const reducedMotion = usePrefersReducedMotion();
const theme = shallowRef<ChartTheme>(readChartTheme());
const expandIcon = mdiChevronDown;

function refreshTheme(): void {
  // Wait one tick so body.body--dark and its CSS tokens are applied first.
  void nextTick(() => {
    theme.value = readChartTheme();
  });
}

onMounted(refreshTheme);
watch(() => $q.dark.isActive, refreshTheme);

const option = computed(() =>
  buildChartOption(props.series, theme.value, { animation: !reducedMotion.value }),
);

// A different x axis (other preset → other range) is a new chart: remount so the zoom resets.
const rangeKey = computed(() => `${props.series.bucket}:${props.series.keys.join('|')}`);
</script>
