<script setup lang="ts">
import { computed } from 'vue'
import { useRoute, useRouter, type LocationQueryValue } from 'vue-router'
import type { Kind } from '@/api/generated/Kind'
import type { Project } from '@/api/generated/Project'
import type { Snapshot } from '@/api/generated/Snapshot'
import type { Tally } from '@/api/generated/Tally'
import AgentBadges from '@/components/AgentBadges.vue'
import AppHeader from '@/components/AppHeader.vue'
import CommandHint, { flayer, making } from '@/components/CommandHint.vue'
import HealthBadge from '@/components/HealthBadge.vue'
import SnapshotGate from '@/components/SnapshotGate.vue'
import { useSnapshot } from '@/api/snapshot'
import { projectLink } from '@/routes'

const route = useRoute()
const router = useRouter()

// What the table is filtered and sorted by lives in the address, so a link
// to it, or a reload, opens it the same way: `?q=api&health=attention`,
// `?state=outdated`, `?sort=kind:skill&order=desc`.

function first(value: LocationQueryValue | LocationQueryValue[] | undefined): string | undefined {
  const one = Array.isArray(value) ? value[0] : value
  return one ?? undefined
}

/** The address with `changes` made to its query, an empty value left out. */
function change(changes: Record<string, string | undefined>) {
  const query = { ...route.query, ...changes }
  for (const [key, value] of Object.entries(query)) {
    if (value === undefined || value === '') {
      delete query[key]
    }
  }
  void router.replace({ query })
}

const text = computed({
  get: () => first(route.query.q) ?? '',
  set: (q: string) => change({ q })
})
const health = computed(() => first(route.query.health))
const state = computed(() => first(route.query.state))
const sort = computed(() => first(route.query.sort))
const descending = computed(() => first(route.query.order) === 'desc')

/** What a column sorts by: the name, the path, the health, or a kind's count. */
type Column = 'name' | 'path' | 'health' | `kind:${Kind}`

function count(tally: Tally, kind: Kind): number {
  return tally.kinds.find(counted => counted.kind === kind)?.count ?? 0
}

/** What a row is called: its name, or its entry when it opens no project. */
function title(project: Project): string {
  return project.name ?? project.entry ?? project.path
}

/** What a row is named by in a link: its entry, as `-p` takes it. */
function entry(project: Project): string {
  return project.entry ?? project.path
}

/**
 * The rows to show: the snapshot's, in its order, those the filters keep,
 * then in the order a column asks for, if one does. Healths are ordered as
 * the snapshot lists them, counts as numbers, names and paths as text; a
 * tie keeps the snapshot's order.
 */
function rows(snapshot: Snapshot): Project[] {
  const needle = text.value.trim().toLowerCase()
  const kept = snapshot.projects
    .map((project, index) => ({ project, index }))
    .filter(({ project }) => !needle || [project.name, project.entry, project.path].some(field => field?.toLowerCase().includes(needle)))
    .filter(({ project }) => !health.value || project.health === health.value)
    .filter(({ project }) => !state.value || (project.tally.states.find(counted => counted.state === state.value)?.count ?? 0) > 0)
  const column = sort.value as Column | undefined
  if (column) {
    const by = (project: Project): string | number => {
      if (column === 'name') {
        return title(project)
      }
      if (column === 'path') {
        return project.path
      }
      if (column === 'health') {
        return snapshot.healths.indexOf(project.health)
      }
      return count(project.tally, column.slice('kind:'.length) as Kind)
    }
    const sign = descending.value ? -1 : 1
    kept.sort((a, b) => {
      const [left, right] = [by(a.project), by(b.project)]
      const order = typeof left === 'number' && typeof right === 'number'
        ? left - right
        : String(left).localeCompare(String(right))
      return order * sign || a.index - b.index
    })
  }
  return kept.map(({ project }) => project)
}

/** Ascending, then descending, then the snapshot's own order again. */
function toggle(column: Column) {
  if (sort.value !== column) {
    change({ sort: column, order: undefined })
  } else if (!descending.value) {
    change({ order: 'desc' })
  } else {
    change({ sort: undefined, order: undefined })
  }
}

function ariaSort(column: Column): 'ascending' | 'descending' | undefined {
  if (sort.value !== column) {
    return undefined
  }
  return descending.value ? 'descending' : 'ascending'
}

function sortIcon(column: Column): string {
  if (sort.value !== column) {
    return 'i-lucide-chevrons-up-down'
  }
  return descending.value ? 'i-lucide-arrow-down-wide-narrow' : 'i-lucide-arrow-up-narrow-wide'
}

// The same answer SnapshotGate draws from, read here too so the rows are
// worked out once a change rather than once each place they are used.
const { data } = useSnapshot()
const shown = computed(() => (data.value ? rows(data.value) : []))

const filtered = computed(() => Boolean(text.value || health.value || state.value))

function clear() {
  change({ q: undefined, health: undefined, state: undefined })
}
</script>

<template>
  <UDashboardPanel
    id="projects"
    role="main"
    data-testid="projects"
  >
    <template #header>
      <AppHeader
        title="Projects"
        icon="i-lucide-folder-git-2"
      />
    </template>

    <template #body>
      <SnapshotGate v-slot="{ snapshot }">
        <div
          v-if="snapshot.projects.length === 0"
          class="flex flex-col items-start gap-3"
          data-testid="no-projects"
        >
          <p class="text-muted">
            No project is linked to {{ snapshot.workspace.name }} yet.
          </p>
          <CommandHint
            :commands="[
              ...making(snapshot.workspace),
              { line: 'mind init', does: 'in a repository: make it a project' },
              { line: flayer(snapshot.workspace, 'link <PATH>'), does: 'link that project here' },
              { line: flayer(snapshot.workspace, 'scan --suggest'), does: 'the repositories most worth linking' }
            ]"
          />
        </div>

        <template v-else>
          <div
            class="flex flex-wrap items-center gap-2"
            data-testid="filters"
          >
            <UInput
              v-model="text"
              icon="i-lucide-search"
              placeholder="Filter by name or path"
              aria-label="Filter projects by name or path"
              size="sm"
              class="w-full sm:w-64"
              data-testid="filter-text"
            />
            <UFieldGroup
              role="group"
              aria-label="Show projects by health"
              size="sm"
            >
              <UButton
                label="all"
                color="neutral"
                :variant="health ? 'outline' : 'subtle'"
                :icon="health ? undefined : 'i-lucide-check'"
                :aria-pressed="!health"
                class="font-mono"
                @click="change({ health: undefined })"
              />
              <UButton
                v-for="each in snapshot.healths"
                :key="each"
                :label="each"
                color="neutral"
                :variant="health === each ? 'subtle' : 'outline'"
                :icon="health === each ? 'i-lucide-check' : undefined"
                :aria-pressed="health === each"
                class="font-mono"
                :data-testid="`filter-health-${each}`"
                @click="change({ health: each })"
              />
            </UFieldGroup>
            <UButton
              v-if="state"
              :label="`holding ${state}`"
              trailing-icon="i-lucide-x"
              color="neutral"
              variant="subtle"
              size="sm"
              class="font-mono"
              :aria-label="`Stop showing only projects holding ${state}`"
              data-testid="filter-state"
              @click="change({ state: undefined })"
            />
          </div>

          <div
            class="overflow-x-auto rounded-lg border border-default"
            tabindex="0"
            role="region"
            aria-label="Projects"
          >
            <table
              class="w-full min-w-max text-left"
              data-testid="table"
            >
              <caption class="sr-only">
                Every entry the workspace links, with its agents, how it shares, what it holds of each kind and how it stands. A column with a button sorts by it.
              </caption>
              <thead class="bg-elevated/40 font-mono text-xs text-muted">
                <tr>
                  <th
                    v-for="column in (['name', 'path'] as const)"
                    :key="column"
                    scope="col"
                    class="px-3 py-2 font-medium"
                    :aria-sort="ariaSort(column)"
                  >
                    <button
                      type="button"
                      class="inline-flex items-center gap-1 rounded-sm hover:text-highlighted focus-visible:outline-2 focus-visible:outline-primary"
                      :data-testid="`sort-${column}`"
                      @click="toggle(column)"
                    >
                      {{ column }}
                      <UIcon
                        :name="sortIcon(column)"
                        class="size-3.5"
                        aria-hidden="true"
                      />
                    </button>
                  </th>
                  <th
                    scope="col"
                    class="px-3 py-2 font-medium"
                  >
                    agents
                  </th>
                  <th
                    scope="col"
                    class="px-3 py-2 font-medium"
                  >
                    share
                  </th>
                  <th
                    v-for="kind in snapshot.kinds"
                    :key="kind"
                    scope="col"
                    class="px-3 py-2 text-right font-medium"
                    :aria-sort="ariaSort(`kind:${kind}`)"
                    :data-testid="`column-${kind}`"
                  >
                    <button
                      type="button"
                      class="inline-flex items-center gap-1 rounded-sm hover:text-highlighted focus-visible:outline-2 focus-visible:outline-primary"
                      :data-testid="`sort-kind-${kind}`"
                      @click="toggle(`kind:${kind}`)"
                    >
                      {{ kind }}
                      <UIcon
                        :name="sortIcon(`kind:${kind}`)"
                        class="size-3.5"
                        aria-hidden="true"
                      />
                    </button>
                  </th>
                  <th
                    scope="col"
                    class="px-3 py-2 font-medium"
                    :aria-sort="ariaSort('health')"
                  >
                    <button
                      type="button"
                      class="inline-flex items-center gap-1 rounded-sm hover:text-highlighted focus-visible:outline-2 focus-visible:outline-primary"
                      data-testid="sort-health"
                      @click="toggle('health')"
                    >
                      health
                      <UIcon
                        :name="sortIcon('health')"
                        class="size-3.5"
                        aria-hidden="true"
                      />
                    </button>
                  </th>
                  <th
                    scope="col"
                    class="px-3 py-2 font-medium"
                  >
                    stale
                  </th>
                </tr>
              </thead>
              <tbody class="divide-y divide-default">
                <tr
                  v-for="project in shown"
                  :key="entry(project)"
                  class="align-top hover:bg-elevated/30"
                  data-testid="row"
                >
                  <th
                    scope="row"
                    class="px-3 py-2 font-normal"
                  >
                    <RouterLink
                      :to="projectLink(entry(project))"
                      class="rounded-sm font-medium text-highlighted hover:text-primary focus-visible:outline-2 focus-visible:outline-primary"
                      data-testid="name"
                    >
                      {{ title(project) }}
                    </RouterLink>
                    <div
                      v-if="project.name && project.entry && project.entry !== project.name"
                      class="font-mono text-xs text-dimmed"
                    >
                      {{ project.entry }}
                    </div>
                  </th>
                  <td class="max-w-80 px-3 py-2">
                    <code
                      class="break-all font-mono text-xs text-muted"
                      data-testid="path"
                    >{{ project.path }}</code>
                  </td>
                  <td class="px-3 py-2">
                    <AgentBadges :agents="project.agents" />
                  </td>
                  <td
                    class="px-3 py-2 font-mono text-xs"
                    data-testid="share"
                  >
                    <template v-if="project.settings">
                      {{ project.settings.share }}
                      <span
                        v-if="project.settings.edit_tracked"
                        class="text-dimmed"
                      >, edits tracked files</span>
                    </template>
                    <span
                      v-else
                      class="text-dimmed"
                    >none</span>
                  </td>
                  <td
                    v-for="kind in snapshot.kinds"
                    :key="kind"
                    class="px-3 py-2 text-right tabular-nums"
                    :data-testid="`count-${kind}`"
                  >
                    {{ count(project.tally, kind) }}
                  </td>
                  <td class="px-3 py-2">
                    <HealthBadge :health="project.health" />
                  </td>
                  <td
                    class="max-w-72 px-3 py-2 text-xs"
                    data-testid="stale"
                  >
                    <span
                      v-if="project.stale"
                      class="line-clamp-2 text-muted"
                      :title="project.stale.message"
                    >{{ project.stale.message }}</span>
                    <span
                      v-else
                      class="font-mono text-dimmed"
                    >no</span>
                  </td>
                </tr>
              </tbody>
            </table>
          </div>

          <div
            v-if="shown.length === 0"
            class="flex flex-col items-start gap-2"
            data-testid="no-match"
          >
            <p class="text-muted">
              No project matches these filters.
            </p>
            <UButton
              label="Clear the filters"
              icon="i-lucide-filter-x"
              color="neutral"
              variant="outline"
              size="sm"
              @click="clear"
            />
          </div>
          <UButton
            v-else-if="filtered"
            label="Clear the filters"
            icon="i-lucide-filter-x"
            color="neutral"
            variant="ghost"
            size="sm"
            class="self-start"
            @click="clear"
          />
        </template>
      </SnapshotGate>
    </template>
  </UDashboardPanel>
</template>
