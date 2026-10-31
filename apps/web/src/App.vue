<script setup lang="ts">
import AppSidebar from './components/AppSidebar.vue'
import CommandPalette from './components/CommandPalette.vue'
import CommandRunner from './components/CommandRunner.vue'
import ShortcutSheet from './components/ShortcutSheet.vue'

// The sidebar's size and whether it is collapsed go to vueuse's useStorage
// (`storage="local"`), which src/preferences.ts points at a cookie of the
// panel's own, so they outlive the launch. Nuxt UI's default, `"cookie"`, is
// a stub outside Nuxt that keeps nothing.
</script>

<template>
  <UApp :tooltip="{ delayDuration: 300 }">
    <CommandRunner>
      <UDashboardGroup
        unit="rem"
        storage="local"
      >
        <AppSidebar />
        <RouterView />
      </UDashboardGroup>
      <CommandPalette />
      <ShortcutSheet />
    </CommandRunner>
  </UApp>
</template>
