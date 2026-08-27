<template>
  <section class="reviews" :aria-label="de.reviews.heading" :aria-busy="loading ? 'true' : 'false'">
    <div class="section-head">
      <h3>{{ de.reviews.heading }}</h3>
      <label v-if="entry && entry.total > 0" class="reviews-sort">
        <span>{{ de.reviews.sortLabel }}</span>
        <select v-model="sort" class="reviews-sort-select" data-testid="reviews-sort">
          <option v-for="option in SORT_OPTIONS" :key="option" :value="option">
            {{ de.reviews.sort[option] }}
          </option>
        </select>
      </label>
    </div>

    <div class="reviews-body">
      <EmptyState
        v-if="error"
        :message="error"
        tone="error"
        :retry-label="de.reviews.retry"
        @retry="load"
      />
      <EmptyState v-else-if="!entry" :message="de.reviews.loading" />
      <EmptyState v-else-if="entry.summary.scraped_at === null" :message="de.reviews.notScraped" />
      <EmptyState v-else-if="entry.summary.count === 0" :message="de.reviews.empty" />

      <template v-else>
        <div class="reviews-summary" :aria-label="de.reviews.summaryAria" role="group">
          <div class="reviews-average">
            <div class="reviews-average-value" data-testid="reviews-average">
              {{ rating(entry.summary.value) || '–' }}
            </div>
            <RatingStars
              v-if="entry.summary.value !== null"
              :value="entry.summary.value"
              size="lg"
            />
            <div class="reviews-count" data-testid="reviews-count">
              {{ de.reviews.count(entry.summary.count) }}
            </div>
            <div
              v-if="entry.summary.stored_count > 0"
              class="reviews-verified"
              data-testid="reviews-verified"
            >
              {{ de.reviews.verifiedShare(verifiedPct) }}
            </div>
          </div>
          <ol class="reviews-distribution" :aria-label="de.reviews.distributionAria">
            <li
              v-for="row in distribution"
              :key="row.stars"
              class="distribution-row"
              :aria-label="de.reviews.distributionRow(row.stars, row.count)"
            >
              <span class="distribution-label" aria-hidden="true">{{ row.stars }} ★</span>
              <span class="distribution-bar" aria-hidden="true">
                <span class="distribution-fill" :style="{ width: `${row.pct}%` }"></span>
              </span>
              <span class="distribution-count" aria-hidden="true">{{ integer(row.count) }}</span>
            </li>
          </ol>
          <div class="reviews-asof">{{ de.reviews.asOf(dateTime(entry.summary.scraped_at)) }}</div>
        </div>

        <ol v-if="entry.reviews.length > 0" class="reviews-list" :aria-label="de.reviews.listAria">
          <li v-for="review in entry.reviews" :key="review.id" class="review">
            <div class="review-head">
              <span class="review-author">{{ review.author || '–' }}</span>
              <span class="review-date">
                {{ review.reviewed_on ? calendarDay(review.reviewed_on) : de.reviews.noDate }}
              </span>
              <RatingStars :value="review.rating" size="sm" />
              <span v-if="review.verified" class="review-verified">{{ de.reviews.verified }}</span>
            </div>
            <p v-if="review.content" class="review-text">{{ review.content }}</p>
          </li>
        </ol>

        <div v-if="entry.total > entry.reviews.length" class="reviews-more">
          <span class="result-count">{{
            de.reviews.shown(entry.reviews.length, entry.total)
          }}</span>
          <button type="button" class="clear-button" :disabled="loading" @click="more">
            {{ loading ? de.reviews.loading : de.reviews.more }}
          </button>
        </div>
      </template>
    </div>
  </section>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, ref, shallowRef, watch } from 'vue';
import type { ReviewSort } from '@/api/types';
import { de } from '@/i18n/de';
import { calendarDay, dateTime, integer, num, rating } from '@/lib/format';
import { isAbortError } from '@/api/client';
import { DEFAULT_REVIEW_SORT, useReviewsStore, type ReviewsEntry } from '@/stores/reviews';
import EmptyState from './EmptyState.vue';
import RatingStars from './RatingStars.vue';

const SORT_OPTIONS: readonly ReviewSort[] = ['newest', 'oldest', 'highest', 'lowest'];

const props = defineProps<{ strainId: number }>();

const store = useReviewsStore();
const sort = ref<ReviewSort>(DEFAULT_REVIEW_SORT);
const entry = shallowRef<ReviewsEntry | null>(null);
const loading = ref(false);
const error = ref<string | null>(null);
let generation = 0;

async function run(task: () => Promise<ReviewsEntry>): Promise<void> {
  const current = ++generation;
  loading.value = true;
  error.value = null;
  try {
    const result = await task();
    if (current !== generation) return;
    entry.value = result;
  } catch (cause) {
    if (current !== generation || isAbortError(cause)) return;
    error.value = de.reviews.loadError;
  } finally {
    if (current === generation) loading.value = false;
  }
}

function load(): void {
  entry.value = null;
  void run(() => store.fetchReviews(props.strainId, sort.value));
}

function more(): void {
  void run(() => store.loadMore(props.strainId, sort.value));
}

watch([() => props.strainId, sort], load, { immediate: true });
onBeforeUnmount(() => {
  generation += 1;
  store.abortAll();
});

const verifiedPct = computed(() => {
  const summary = entry.value?.summary;
  if (!summary || summary.stored_count === 0) return '0';
  return num(Math.round((summary.verified_count / summary.stored_count) * 100), 0);
});

const distribution = computed(() => {
  const summary = entry.value?.summary;
  if (!summary) return [];
  const max = Math.max(1, ...Object.values(summary.distribution));
  return ([5, 4, 3, 2, 1] as const).map((stars) => {
    const count = summary.distribution[String(stars) as keyof typeof summary.distribution] ?? 0;
    return { stars, count, pct: Math.round((count / max) * 100) };
  });
});
</script>
