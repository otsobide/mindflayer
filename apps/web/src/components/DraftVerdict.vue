<script setup lang="ts">
import { computed } from 'vue'
import type { Judged } from '@/api/generated/Judged'
import IssueList from './IssueList.vue'

const props = defineProps<{
  /** What core said of the draft, once it said anything. */
  judged: Judged | undefined
  /** Whether what is drawn is of a draft since changed, the next answer asked for. */
  pending: boolean
  /** Why it could not be asked, when it could not. */
  error?: Error | null
  /** What writes it, as the refusal is said of it: `flayer add`, saving. */
  writer: string
}>()

/** Its issues as `IssueList` draws an artifact's, with nothing unread. */
const listed = computed(() => ({ issues: props.judged?.issues ?? [], unread: null }))
</script>

<template>
  <!--
    What core says, in core's words, each said in words as well as by its
    colour and icon: `refused`, why the command would write nothing, then
    every issue `validate` would say once it is written, as the artifact's
    page lists them. Polite, so a screen reader says the verdict once
    somebody stops typing, not every keystroke.
  -->
  <section
    aria-labelledby="verdict-heading"
    class="flex flex-col gap-2"
    data-testid="verdict"
  >
    <h2
      id="verdict-heading"
      class="flex items-center gap-2 text-sm font-medium text-highlighted"
    >
      What core says of it
      <span
        v-if="pending"
        class="text-xs font-normal text-muted"
        data-testid="judging"
      >judging the latest change</span>
    </h2>
    <div
      role="status"
      aria-live="polite"
      class="flex flex-col gap-2 text-xs"
    >
      <UAlert
        v-if="error"
        color="warning"
        variant="subtle"
        icon="i-lucide-circle-help"
        title="It could not be judged"
        :description="error.message"
        data-testid="unjudged"
      />
      <template v-else-if="judged">
        <UAlert
          v-if="judged.refused"
          color="error"
          variant="subtle"
          icon="i-lucide-ban"
          :title="`${writer} would refuse it`"
          :ui="{ description: 'break-words' }"
          data-testid="refused"
        >
          <template #description>
            <span class="me-2 font-mono text-xs">{{ judged.refused.code }}<span class="sr-only">:</span></span>
            <span data-testid="refused-message">{{ judged.refused.message }}</span>
          </template>
        </UAlert>
        <p
          v-else
          class="flex items-center gap-1.5 text-muted"
          data-testid="writes"
        >
          <UIcon
            name="i-lucide-circle-check"
            class="size-3.5 shrink-0 text-success"
            aria-hidden="true"
          />
          {{ writer }} would write it.
        </p>
        <p
          v-if="judged.issues.length === 0 && !judged.refused"
          class="text-muted"
          data-testid="valid"
        >
          Validate would say nothing: it reads, and passes with no warning.
        </p>
        <div
          v-else-if="judged.issues.length > 0"
          class="flex flex-col gap-1"
          data-testid="issues"
        >
          <p class="text-muted">
            What validate would say of it once written:
          </p>
          <IssueList :held="listed" />
        </div>
      </template>
      <p
        v-else
        class="text-muted"
        data-testid="unasked"
      >
        Nothing to judge yet.
      </p>
    </div>
  </section>
</template>
