<script setup lang="ts">
import { computed } from 'vue'
import type { RouteLocationRaw } from 'vue-router'
import type { Said } from '@/api/generated/Said'
import type { Snapshot } from '@/api/generated/Snapshot'
import { projectLink } from '@/routes'

const props = defineProps<{
  snapshot: Snapshot
}>()

/** What could not be read, under what it belongs to. */
interface Group {
  key: string
  /** What holds them: a project, a load, the library, the workspace. */
  holder: string
  to?: RouteLocationRaw
  said: Said[]
}

// Every failure the snapshot collected, where it collected it: beside what
// it is about, so each is listed under that, in the snapshot's order. A
// stale entry's reason is among them, as `flayer list` says one beside the
// rest. Nothing is weighed or counted here: the lists are core's.
const groups = computed<Group[]>(() => {
  const { snapshot } = props
  const groups: Group[] = [
    { key: 'workspace', holder: `workspace ${snapshot.workspace.name}`, said: snapshot.warnings },
    { key: 'library', holder: 'library', said: snapshot.library.failures }
  ]
  for (const project of snapshot.projects) {
    const entry = project.entry ?? project.path
    groups.push({
      key: `project:${entry}`,
      holder: project.name ?? entry,
      to: projectLink(entry),
      said: [...(project.stale ? [project.stale] : []), ...project.failures, ...project.misplaced]
    })
  }
  for (const load of snapshot.loaded) {
    groups.push({
      key: `load:${load.entry}`,
      holder: `load ${load.entry}`,
      said: load.error ? [load.error] : []
    })
  }
  return groups.filter(group => group.said.length > 0)
})
</script>

<template>
  <section
    aria-labelledby="warnings-heading"
    class="rounded-lg border border-default"
    data-testid="warnings"
  >
    <h2
      id="warnings-heading"
      class="flex items-center gap-2 border-b border-default px-4 py-2.5 text-sm font-medium text-highlighted"
    >
      <UIcon
        name="i-lucide-list-x"
        class="size-4 text-dimmed"
        aria-hidden="true"
      />
      What could not be read
    </h2>
    <p
      v-if="groups.length === 0"
      class="flex items-center gap-2 px-4 py-3 text-muted"
      data-testid="no-warnings"
    >
      <UIcon
        name="i-lucide-circle-check"
        class="size-4 text-success"
        aria-hidden="true"
      />
      Everything read: nothing failed.
    </p>
    <ul
      v-else
      class="divide-y divide-default"
    >
      <li
        v-for="group in groups"
        :key="group.key"
        class="px-4 py-3"
        data-testid="warning-group"
      >
        <h3 class="mb-1.5 font-mono text-xs font-medium text-muted">
          <RouterLink
            v-if="group.to"
            :to="group.to"
            class="rounded-sm hover:text-primary focus-visible:outline-2 focus-visible:outline-primary"
          >
            {{ group.holder }}
          </RouterLink>
          <template v-else>
            {{ group.holder }}
          </template>
        </h3>
        <ul class="space-y-1.5">
          <li
            v-for="(said, index) in group.said"
            :key="index"
            class="flex items-start gap-2"
            data-testid="warning"
          >
            <UIcon
              name="i-lucide-circle-alert"
              class="mt-0.5 size-4 shrink-0 text-warning"
              aria-hidden="true"
            />
            <span class="min-w-0 break-words">
              <span class="me-2 font-mono text-xs text-dimmed">{{ said.code }}<span class="sr-only">:</span></span>
              {{ said.message }}
            </span>
          </li>
        </ul>
      </li>
    </ul>
  </section>
</template>
