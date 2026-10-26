<script setup lang="ts">
import AppHeader from '@/components/AppHeader.vue'
</script>

<template>
  <UDashboardPanel
    id="overview"
    data-testid="overview"
  >
    <template #header>
      <AppHeader
        title="Overview"
        icon="i-lucide-layout-dashboard"
      />
    </template>

    <template #body>
      <div class="bg-dots grid flex-1 place-items-center">
        <UEmpty
          variant="naked"
          icon="i-lucide-brain-circuit"
          title="Nothing to show yet"
          description="Projects, the library and the shelf will appear here. Until they do, the command line has all of it."
          :avatar="{ ui: { icon: 'text-primary' } }"
          :ui="{ avatar: 'rounded-lg bg-elevated ring ring-default shadow-[0_0_32px_-6px_var(--ui-primary)]' }"
        >
          <template #footer>
            <code class="rounded-md border border-default bg-muted px-2.5 py-1 font-mono text-xs text-toned">
              <span
                class="select-none text-dimmed"
                aria-hidden="true"
              >$ </span>flayer list
            </code>
          </template>
        </UEmpty>
      </div>
    </template>
  </UDashboardPanel>
</template>
