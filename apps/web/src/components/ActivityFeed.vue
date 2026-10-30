<script setup lang="ts">
import type { BadgeProps } from '@nuxt/ui'
import type { Entry } from '@/api/generated/Entry'
import { instant, when } from '@/time'

defineProps<{
  /** The latest actions the ledger logged, newest first, as the snapshot holds them. */
  entries: Entry[]
}>()

interface Look {
  color: BadgeProps['color']
  icon: string
}

// How each outcome looks, and nothing it means: the word is the ledger's,
// and the badge always says it. Partial, with a plain look for the rest,
// as a state's badge is, so an outcome the ledger adds is drawn by its word.
const LOOKS: Partial<Record<string, Look>> = {
  ok: { color: 'success', icon: 'i-lucide-circle-check' },
  failed: { color: 'error', icon: 'i-lucide-circle-x' }
}

const PLAIN: Look = { color: 'neutral', icon: 'i-lucide-circle' }

function look(outcome: string): Look {
  return LOOKS[outcome] ?? PLAIN
}
</script>

<template>
  <p
    v-if="entries.length === 0"
    class="text-muted"
    data-testid="no-activity"
  >
    The ledger has logged nothing yet.
  </p>
  <!--
    Newest first, as the ledger hands them over: every action, what it was
    about and what it said, each URL in either already without credentials
    (core writes them through url::redacted).
  -->
  <ol
    v-else
    class="flex flex-col divide-y divide-default rounded-lg border border-default"
    aria-label="The latest actions, newest first"
    data-testid="activity"
  >
    <li
      v-for="(entry, index) in entries"
      :key="index"
      class="flex flex-col gap-1 px-3 py-2 sm:flex-row sm:items-start sm:gap-3"
      data-testid="entry"
    >
      <time
        :datetime="instant(entry.at)"
        class="shrink-0 text-xs text-muted sm:w-40"
        data-testid="at"
      >{{ when(entry.at) }}</time>
      <div class="flex min-w-0 flex-col gap-0.5">
        <div class="flex flex-wrap items-center gap-2">
          <span
            class="font-mono text-sm font-medium text-highlighted"
            data-testid="action"
          >{{ entry.action }}</span>
          <code
            v-if="entry.target"
            class="break-all font-mono text-xs text-muted"
            data-testid="target"
          >{{ entry.target }}</code>
          <UBadge
            :color="look(entry.outcome).color"
            :icon="look(entry.outcome).icon"
            :label="entry.outcome"
            variant="subtle"
            size="sm"
            class="font-mono"
            data-testid="outcome"
          />
        </div>
        <p
          v-if="entry.detail"
          class="break-words text-xs text-muted"
          data-testid="detail"
        >
          {{ entry.detail }}
        </p>
      </div>
    </li>
  </ol>
</template>
