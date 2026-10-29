<script setup lang="ts">
import { computed } from 'vue'
import type { BadgeProps } from '@nuxt/ui'
import type { Health } from '@/api/generated/Health'

const props = defineProps<{
  health: Health
}>()

interface Look {
  color: BadgeProps['color']
  icon: string
}

// As a state's badge: the word is core's verdict and always shown, the
// colour and the icon only repeat it, and a health core adds is drawn by
// its word with a plain look.
const LOOKS: Partial<Record<Health, Look>> = {
  ok: { color: 'success', icon: 'i-lucide-circle-check' },
  attention: { color: 'warning', icon: 'i-lucide-triangle-alert' },
  broken: { color: 'error', icon: 'i-lucide-circle-x' },
  stale: { color: 'neutral', icon: 'i-lucide-link-2-off' }
}

const PLAIN: Look = { color: 'neutral', icon: 'i-lucide-circle' }

const look = computed(() => LOOKS[props.health] ?? PLAIN)
</script>

<template>
  <UBadge
    :color="look.color"
    :icon="look.icon"
    :label="health"
    variant="subtle"
    size="sm"
    class="font-mono"
    data-testid="health"
  />
</template>
