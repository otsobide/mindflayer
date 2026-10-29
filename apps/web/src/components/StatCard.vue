<script setup lang="ts">
import { computed } from 'vue'
import type { RouteLocationRaw } from 'vue-router'

const props = defineProps<{
  /** What is counted, as a word or two. */
  label: string
  /** The count, as the snapshot says it. */
  value: number
  icon: string
  /** Where the counted things are listed, when they are. */
  to?: RouteLocationRaw
  /**
   * Drawn in the warning colour while the count is not zero: something
   * there asks to be looked at. The label says what, whatever the colour.
   */
  warns?: boolean
}>()

const warning = computed(() => props.warns && props.value > 0)
</script>

<template>
  <component
    :is="to ? 'RouterLink' : 'div'"
    :to="to"
    class="group flex h-full flex-col gap-2 rounded-lg border bg-elevated/30 p-4"
    :class="[
      warning ? 'border-warning/40' : 'border-default',
      to && 'transition-colors hover:bg-elevated/70 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-primary'
    ]"
  >
    <span class="flex items-center justify-between gap-2">
      <span class="font-mono text-xs font-medium uppercase tracking-wider text-muted">{{ label }}</span>
      <UIcon
        :name="warning ? 'i-lucide-triangle-alert' : icon"
        class="size-4 shrink-0"
        :class="warning ? 'text-warning' : 'text-dimmed group-hover:text-primary'"
        aria-hidden="true"
      />
    </span>
    <span
      class="text-2xl font-semibold tabular-nums"
      :class="warning ? 'text-warning' : 'text-highlighted'"
      data-testid="count"
    >{{ value }}</span>
    <span
      v-if="$slots.default"
      class="text-xs text-dimmed"
    >
      <slot />
    </span>
  </component>
</template>
