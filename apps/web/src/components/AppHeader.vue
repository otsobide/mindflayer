<script setup lang="ts">
defineProps<{
  title: string
  icon?: string
}>()
</script>

<template>
  <UDashboardNavbar
    :title="title"
    :icon="icon"
    :ui="{ title: 'text-sm font-medium', icon: 'size-4 text-dimmed', right: 'gap-1' }"
  >
    <template #leading>
      <UDashboardSidebarCollapse
        size="sm"
        class="-ms-1"
      />
    </template>

    <template #right>
      <slot />
      <UColorModeButton size="sm" />
    </template>
  </UDashboardNavbar>
</template>
