<script setup lang="ts">
import AppHeader from '@/components/AppHeader.vue'

// Until the views arrive, the commands that show what they will: `list`
// leaves the shelf out, because `gather list` is the shelf's listing.
const commands = [
  { line: 'flayer list', shows: 'projects and the library' },
  { line: 'flayer gather list', shows: 'the shelf' }
]
</script>

<template>
  <UDashboardPanel
    id="overview"
    role="main"
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
          description="Projects, the library and the shelf will appear here. Until they do, the command line shows all of it."
          :avatar="{ ui: { icon: 'text-primary' } }"
          :ui="{ avatar: 'rounded-lg bg-elevated ring ring-default shadow-[0_0_32px_-6px_var(--ui-primary)]' }"
        >
          <template #footer>
            <div
              class="grid grid-cols-[auto_auto] gap-x-4 gap-y-1 rounded-md border border-default bg-muted px-3 py-2 text-left font-mono text-xs"
              data-testid="commands"
            >
              <template
                v-for="command in commands"
                :key="command.line"
              >
                <code class="text-toned"><span
                  class="select-none text-dimmed"
                  aria-hidden="true"
                >$ </span>{{ command.line }}</code>
                <span class="select-none text-dimmed"># {{ command.shows }}</span>
              </template>
            </div>
          </template>
        </UEmpty>
      </div>
    </template>
  </UDashboardPanel>
</template>
