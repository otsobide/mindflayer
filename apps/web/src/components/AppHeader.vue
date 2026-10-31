<script setup lang="ts">
import { useShell } from '@/stores/shell'
import WorkspaceSwitcher from './WorkspaceSwitcher.vue'

defineProps<{
  title: string
  icon?: string
}>()

const shell = useShell()
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
      <WorkspaceSwitcher />
      <slot />
      <UTooltip text="Transcript: every task this session ran">
        <UButton
          icon="i-lucide-scroll-text"
          color="neutral"
          variant="ghost"
          size="sm"
          aria-label="Transcript"
          data-testid="open-transcript"
          @click="shell.transcriptOpen = true"
        />
      </UTooltip>
      <UColorModeButton size="sm" />
    </template>
  </UDashboardNavbar>
</template>
