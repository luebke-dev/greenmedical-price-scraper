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
        <td>{{ offer.apotheke || '' }}</td>
        <td>
          <span class="provider-badge">{{ de.offers.providers[offer.provider] }}</span>
        </td>
        <td>{{ offer.apotheke_stadt || '' }}</td>
        <td class="price">{{ offer.preis_pro_gramm || euro(offer.preis_eur_pro_gramm, '€/g') }}</td>
        <td class="price">{{ euro(offer.preis_eur_pro_gramm_thc, '€/g THC') }}</td>
        <td>
          <StatusBadge v-if="offer.verfuegbarkeit" :value="offer.verfuegbarkeit" />
        </td>
        <td class="buy-cell">
          <BuyLink v-if="offer.produkt_url" :url="offer.produkt_url" :pharmacy="offer.apotheke" />
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
