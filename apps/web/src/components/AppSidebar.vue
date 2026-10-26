<script setup lang="ts">
import type { NavigationMenuItem } from '@nuxt/ui'
import { storeToRefs } from 'pinia'
import { useShell } from '@/stores/shell'
import AppLogo from './AppLogo.vue'

const { sidebarOpen, sidebarCollapsed } = storeToRefs(useShell())

// One entry for now: the views arrive with the snapshot (#72 onward), each
// as a page under src/pages and a line here.
const links: NavigationMenuItem[] = [
  { label: 'Overview', icon: 'i-lucide-layout-dashboard', to: '/' }
]

// Which server this panel came from. Each `flayer ui` takes a port of its
// own, so two open panels can belong to two different launches.
const address = window.location.host
</script>

<template>
  <!--
    An id of its own, or Nuxt UI keys the stored size and collapse on
    useId(), whose value moves whenever something that renders before the
    sidebar asks for an id too, and the stored state with it.
  -->
  <UDashboardSidebar
    id="default"
    v-model:open="sidebarOpen"
    v-model:collapsed="sidebarCollapsed"
    collapsible
    resizable
    data-testid="sidebar"
    class="bg-elevated/25"
    :min-size="13"
    :default-size="15"
    :max-size="20"
    :ui="{
      header: 'h-(--ui-header-height) border-b border-default',
      footer: 'border-t border-default py-2.5'
    }"
  >
    <template #header="{ collapsed }">
      <AppLogo :collapsed="collapsed" />
    </template>

    <template #default="{ collapsed }">
      <UNavigationMenu
        :items="links"
        :collapsed="collapsed"
        orientation="vertical"
        tooltip
        aria-label="Panel"
      />
    </template>

    <template #footer="{ collapsed }">
      <UTooltip
        :text="`Served by ${address}`"
        :disabled="!collapsed"
      >
        <div
          class="flex min-w-0 items-center gap-2 px-1.5 font-mono text-xs text-dimmed"
          data-testid="address"
        >
          <UIcon
            name="i-lucide-server"
            class="size-3.5 shrink-0"
          />
          <span
            v-if="!collapsed"
            class="truncate"
          >{{ address }}</span>
        </div>
      </UTooltip>
    </template>
  </UDashboardSidebar>
</template>
