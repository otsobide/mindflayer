<script lang="ts">
import type { BadgeProps } from '@nuxt/ui'
import type { State } from '@/api/generated/State'

/** How a state is drawn: a colour and an icon, beside its word. */
export interface Look {
  color: BadgeProps['color']
  icon: string
}

// How each state looks, and nothing it means: the word is core's, and the
// badge always says it, so the colour and the icon only repeat it. Partial,
// with a plain look for the rest, so a state core adds is drawn by its word
// before anybody gives it a look of its own.
const LOOKS: Partial<Record<State, Look>> = {
  current: { color: 'success', icon: 'i-lucide-circle-check' },
  outdated: { color: 'warning', icon: 'i-lucide-circle-arrow-up' },
  edited: { color: 'warning', icon: 'i-lucide-pencil' },
  missing: { color: 'error', icon: 'i-lucide-circle-dashed' },
  orphaned: { color: 'warning', icon: 'i-lucide-unlink' },
  foreign: { color: 'neutral', icon: 'i-lucide-user-round' },
  authored: { color: 'info', icon: 'i-lucide-feather' }
}

const PLAIN: Look = { color: 'neutral', icon: 'i-lucide-circle' }

/**
 * How `state` looks, a badge's or a box's in the matrix alike, so a state
 * reads the same wherever it is drawn: plain for none, and for a state no
 * look is given to.
 */
export function stateLook(state: State | null): Look {
  return state === null ? PLAIN : LOOKS[state] ?? PLAIN
}
</script>

<script setup lang="ts">
import { computed } from 'vue'

const props = defineProps<{
  /** None for a row that stands in no state: one whose only copy does not read. */
  state: State | null
}>()

const look = computed(() => stateLook(props.state))
</script>

<template>
  <UBadge
    v-if="state !== null"
    :color="look.color"
    :icon="look.icon"
    :label="state"
    variant="subtle"
    size="sm"
    class="font-mono"
    data-testid="state"
  />
  <span
    v-else
    class="font-mono text-xs text-dimmed"
    data-testid="state"
  >no state</span>
</template>
