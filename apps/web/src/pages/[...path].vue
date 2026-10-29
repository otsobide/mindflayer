<script setup lang="ts">
import { useRoute } from 'vue-router'
import AppHeader from '@/components/AppHeader.vue'

const route = useRoute()
</script>

<template>
  <UDashboardPanel
    id="not-found"
    role="main"
    data-testid="not-found"
  >
    <template #header>
      <AppHeader
        title="Not found"
        icon="i-lucide-map-pin-off"
      />
    </template>

    <template #body>
      <div class="bg-dots grid flex-1 place-items-center">
        <UEmpty
          variant="naked"
          icon="i-lucide-map-pin-off"
          title="No such page"
          :description="`The panel has nothing at ${route.path}.`"
          :actions="[{ label: 'Back to the overview', icon: 'i-lucide-arrow-left', to: '/', color: 'neutral', variant: 'subtle', size: 'sm' }]"
          :ui="{ avatar: 'rounded-lg bg-elevated ring ring-default text-muted' }"
        />
      </div>
    </template>
  </UDashboardPanel>
</template>
