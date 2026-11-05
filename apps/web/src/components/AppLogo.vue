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
    aria-label="Mindweaver: overview"
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
      <!--
        A spider, which weaves, seen from above: its legs long and bent
        once, which is what tells it from a beetle or a tick at 16 pixels,
        and thinner than the body's outline, which would otherwise close
        the gaps between them at that size.
      -->
      <path
        d="M14.3 12.7 11.1 9.1 10.4 6M13.9 14.2 9.6 12.3 7 11M13.9 16 9.6 17.6 7 20.4M14.5 17.4 11.3 21.4 10.5 25.4M17.7 12.7 20.9 9.1 21.6 6M18.1 14.2 22.4 12.3 25 11M18.1 16 22.4 17.6 25 20.4M17.5 17.4 20.7 21.4 21.5 25.4"
        stroke="#fff"
        stroke-width="1.75"
        stroke-linecap="round"
        stroke-linejoin="round"
      />
      <circle
        cx="16"
        cy="13.4"
        r="2.5"
        fill="#fff"
      />
      <ellipse
        cx="16"
        cy="20"
        rx="3.5"
        ry="4.3"
        class="fill-white/15"
        stroke="#fff"
        stroke-width="2"
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
      <span class="truncate text-sm font-semibold tracking-tight text-highlighted">Mindweaver</span>
      <span class="font-mono text-[10px] font-medium uppercase tracking-wider text-dimmed">panel</span>
    </span>
  </RouterLink>
</template>
