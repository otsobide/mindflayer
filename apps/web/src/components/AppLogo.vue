<script setup lang="ts">
import { useId } from 'vue'

defineProps<{
  /** Only the mark, for a sidebar reduced to its icons. */
  collapsed?: boolean
}>()

// The sidebar draws its header twice on a narrow screen, once in the menu
// that slides over the page, and two gradients under one id would both
// paint the first one's, hidden or not.
const gradient = useId()
</script>

<template>
  <RouterLink
    to="/"
    class="flex min-w-0 items-center gap-2.5 rounded-md focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-primary"
    aria-label="Mindflayer: overview"
  >
    <svg
      viewBox="0 0 32 32"
      fill="none"
      class="size-7 shrink-0"
      aria-hidden="true"
    >
      <rect
        width="32"
        height="32"
        rx="8"
        :fill="`url(#${gradient})`"
      />
      <path
        d="M9 15a7 7 0 0 1 14 0Z"
        class="fill-white/15"
        stroke="#fff"
        stroke-width="2"
        stroke-linejoin="round"
      />
      <path
        d="M12 15v4c0 1.6-.8 2.8-2 3.5M16 15v8M20 15v4c0 1.6.8 2.8 2 3.5"
        stroke="#fff"
        stroke-width="2"
        stroke-linecap="round"
        stroke-linejoin="round"
      />
      <defs>
        <linearGradient
          :id="gradient"
          x1="0"
          y1="0"
          x2="32"
          y2="32"
          gradientUnits="userSpaceOnUse"
        >
          <stop class="[stop-color:var(--ui-color-primary-400)]" />
          <stop
            offset="1"
            class="[stop-color:var(--ui-color-primary-700)]"
          />
        </linearGradient>
      </defs>
    </svg>
    <span
      v-if="!collapsed"
      class="flex min-w-0 items-baseline gap-1.5"
    >
      <span class="truncate text-sm font-semibold tracking-tight text-highlighted">Mindflayer</span>
      <span class="font-mono text-[10px] font-medium uppercase tracking-wider text-dimmed">panel</span>
    </span>
  </RouterLink>
</template>
