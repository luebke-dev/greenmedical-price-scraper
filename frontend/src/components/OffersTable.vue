<template>
  <table class="offers">
    <thead>
      <tr>
        <th scope="col">{{ de.offers.pharmacy }}</th>
        <th scope="col">{{ de.offers.provider }}</th>
        <th scope="col">{{ de.offers.city }}</th>
        <th scope="col">{{ de.offers.price }}</th>
        <th scope="col">{{ de.offers.thcPrice }}</th>
        <th scope="col">{{ de.offers.status }}</th>
        <th scope="col">
          <span class="sr-only">{{ de.offers.buy }}</span>
        </th>
      </tr>
    </thead>
    <tbody>
      <tr v-if="offers.length === 0">
        <td class="empty" colspan="7">{{ de.offers.empty }}</td>
      </tr>
      <tr v-for="offer in offers" :key="offer.offer_id">
        <td>{{ offer.pharmacy || '' }}</td>
        <td>
          <span class="provider-badge">{{ de.offers.providers[offer.provider] }}</span>
        </td>
        <td>{{ offer.pharmacy_city || '' }}</td>
        <td class="price">{{ offer.price_per_gram || euro(offer.price_eur_per_gram, '€/g') }}</td>
        <td class="price">{{ euro(offer.price_eur_per_thc_gram, '€/g THC') }}</td>
        <td>
          <StatusBadge v-if="offer.availability" :value="offer.availability" />
        </td>
        <td class="buy-cell">
          <BuyLink v-if="offer.product_url" :url="offer.product_url" :pharmacy="offer.pharmacy" />
        </td>
      </tr>
    </tbody>
  </table>
</template>

<script setup lang="ts">
import type { Offer } from '@/api/types';
import { de } from '@/i18n/de';
import { euro } from '@/lib/format';
import BuyLink from './BuyLink.vue';
import StatusBadge from './StatusBadge.vue';

defineProps<{ offers: readonly Offer[] }>();
</script>
