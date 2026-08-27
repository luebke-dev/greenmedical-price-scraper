<template>
  <div class="offer-history">
    <div class="offer-history-toolbar">
      <q-toggle
        v-model="allRuns"
        dense
        color="primary"
        :label="allRuns ? de.offerHistory.allRuns : de.offerHistory.changesOnly"
      />
    </div>

    <table v-if="!allRuns" class="offers offer-history-table">
      <caption class="sr-only">
        {{
          de.offerHistory.caption
        }}
      </caption>
      <thead>
        <tr>
          <th scope="col">{{ de.offerHistory.from }}</th>
          <th scope="col">{{ de.offerHistory.to }}</th>
          <th scope="col">{{ de.offers.pharmacy }}</th>
          <th scope="col">{{ de.offers.city }}</th>
          <th scope="col">{{ de.offers.price }}</th>
          <th scope="col">{{ de.offers.thcPrice }}</th>
          <th scope="col">{{ de.offers.status }}</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="row in visiblePhases" :key="row.key" :class="{ delisted: row.delisted }">
          <td class="date">{{ row.from }}</td>
          <td class="date">
            <span v-if="row.to === null" class="current">{{ de.offerHistory.current }}</span>
            <template v-else>{{ row.to }}</template>
            <span class="runs">({{ de.offerHistory.runs(row.runs) }})</span>
          </td>
          <td>{{ row.pharmacy }}</td>
          <td>{{ row.city }}</td>
          <td class="price">{{ row.price }}</td>
          <td class="price">{{ row.thcPrice }}</td>
          <td>
            <span v-if="row.delisted" class="status delisted">{{ de.offerHistory.delisted }}</span>
            <StatusBadge v-else-if="row.availability" :value="row.availability" />
          </td>
        </tr>
      </tbody>
    </table>

    <table v-else class="offers offer-history-table">
      <caption class="sr-only">
        {{
          de.offerHistory.caption
        }}
      </caption>
      <thead>
        <tr>
          <th scope="col">{{ de.offerHistory.date }}</th>
          <th scope="col">{{ de.offers.pharmacy }}</th>
          <th scope="col">{{ de.offers.city }}</th>
          <th scope="col">{{ de.offers.price }}</th>
          <th scope="col">{{ de.offers.thcPrice }}</th>
          <th scope="col">{{ de.offers.status }}</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="row in visibleRows" :key="row.key" :class="{ 'run-start': row.first }">
          <td class="date">{{ row.date }}</td>
          <td>{{ row.pharmacy }}</td>
          <td>{{ row.city }}</td>
          <td class="price">{{ row.price }}</td>
          <td class="price">{{ row.thcPrice }}</td>
          <td><StatusBadge v-if="row.availability" :value="row.availability" /></td>
        </tr>
      </tbody>
    </table>

    <div v-if="total > limit" class="offer-history-more">
      <span class="result-count">{{ de.offerHistory.shown(Math.min(limit, total), total) }}</span>
      <button type="button" class="clear-button" @click="limit += PAGE">
        {{ de.offerHistory.more }}
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import type { History } from '@/api/types';
import { de } from '@/i18n/de';
import { offerHistoryPhases, offerHistoryRows } from '@/lib/history';
import StatusBadge from './StatusBadge.vue';

const PAGE = 100;

const props = defineProps<{ history: History }>();

const allRuns = ref(false);
const limit = ref(PAGE);
watch([() => props.history, allRuns], () => {
  limit.value = PAGE;
});

const phases = computed(() => offerHistoryPhases(props.history));
const rows = computed(() => (allRuns.value ? offerHistoryRows(props.history) : []));
const total = computed(() => (allRuns.value ? rows.value.length : phases.value.length));
const visiblePhases = computed(() => phases.value.slice(0, limit.value));
const visibleRows = computed(() => rows.value.slice(0, limit.value));
</script>
