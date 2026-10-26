<script setup lang="ts">
import AppSidebar from './components/AppSidebar.vue'

// The sidebar's size and whether it is collapsed are kept in localStorage,
// not in the cookie Nuxt UI keeps them in by default: outside Nuxt, that
// cookie is a stub that keeps nothing.
</script>

<template>
  <UApp :tooltip="{ delayDuration: 300 }">
    <UDashboardGroup
      unit="rem"
      storage="local"
      storage-key="mindflayer-panel"
    >
      <AppSidebar />
      <RouterView />
    </UDashboardGroup>
  </UApp>
</template>
