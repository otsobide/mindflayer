<script setup lang="ts">
import { computed } from 'vue'
import { useRoute } from 'vue-router'
import type { NavigationMenuItem } from '@nuxt/ui'
import { storeToRefs } from 'pinia'
import { useShell } from '@/stores/shell'
import AppLogo from './AppLogo.vue'

const { sidebarOpen, sidebarCollapsed } = storeToRefs(useShell())
const route = useRoute()

// A view is a page under src/pages and a line here. A project's page is
// reached from the projects, so it lights their line, and one of the
// library's own, its editor and the page adding one light the library's:
// `/project` is no child of `/projects` to the router, nor `/artifact` of
// `/library` (src/routes.ts says why).
const links = computed<NavigationMenuItem[]>(() => [
  { label: 'Overview', icon: 'i-lucide-layout-dashboard', to: '/' },
  {
    label: 'Projects',
    icon: 'i-lucide-folder-git-2',
    to: '/projects',
    active: route.path === '/project' || undefined
  },
  { label: 'Discover', icon: 'i-lucide-radar', to: '/discover' },
  { label: 'Assign', icon: 'i-lucide-grid-3x3', to: '/assign' },
  { label: 'Updates', icon: 'i-lucide-circle-arrow-up', to: '/updates' },
  {
    label: 'Library',
    icon: 'i-lucide-library-big',
    to: '/library',
    active: ['/artifact', '/edit', '/new'].includes(route.path) || undefined
  },
  { label: 'Shelf', icon: 'i-lucide-package', to: '/shelf' },
  { label: 'Sources', icon: 'i-lucide-git-branch', to: '/sources' }
])

// Which server this panel came from. Each `flayer ui` takes a port of its
// own, so two open panels can belong to two different launches.
const address = window.location.host
</script>

<template>
  <!--
    An id of its own, or Nuxt UI keys the stored size and collapse on
    useId(), whose value moves whenever something that renders before the
    sidebar asks for an id too, and the stored state with it. A landmark of
    its own, so the logo and the server's address are in one: nothing on a
    page is outside a landmark.
  -->
  <UDashboardSidebar
    id="default"
    v-model:open="sidebarOpen"
    v-model:collapsed="sidebarCollapsed"
    role="complementary"
    aria-label="Sidebar"
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
