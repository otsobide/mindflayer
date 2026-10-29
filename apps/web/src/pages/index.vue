<script setup lang="ts">
import type { Kind } from '@/api/generated/Kind'
import type { Snapshot } from '@/api/generated/Snapshot'
import type { State } from '@/api/generated/State'
import type { Tally } from '@/api/generated/Tally'
import AppHeader from '@/components/AppHeader.vue'
import CommandHint, { type Command } from '@/components/CommandHint.vue'
import SnapshotGate from '@/components/SnapshotGate.vue'
import StatCard from '@/components/StatCard.vue'
import WarningsPanel from '@/components/WarningsPanel.vue'

// What a tally says of one kind or one state: looked up, never counted, so
// a kind core adds has its card with no change here.
function ofKind(tally: Tally, kind: Kind): number {
  return tally.kinds.find(counted => counted.kind === kind)?.count ?? 0
}

function inState(tally: Tally, state: State): number {
  return tally.states.find(counted => counted.state === state)?.count ?? 0
}

/**
 * The commands that would fill what is empty: projects to link, the
 * library's own, the shelf. Emptiness is what the snapshot lists and
 * counts, as it says them.
 */
function starting(snapshot: Snapshot): Command[] {
  const commands: Command[] = []
  if (snapshot.projects.length === 0) {
    commands.push(
      { line: 'mind init', does: 'in a repository: make it a project' },
      { line: 'flayer link <PATH>', does: 'link that project here' },
      { line: 'flayer scan --suggest', does: 'the repositories most worth linking' }
    )
  }
  if (snapshot.summary.library.total === 0) {
    commands.push({ line: 'flayer add <KIND> <NAME> <DESCRIPTION>', does: 'a skill or a rule of the workspace\'s own' })
  }
  if (snapshot.summary.shelf.total === 0) {
    commands.push({ line: 'flayer gather git <URL>', does: 'skills from a repository, onto the shelf' })
  }
  return commands
}

// The two states the overview counts on their own: the installs an update
// brings up to date, and the ones changed by hand, which it leaves alone.
const ASKING: { state: State, icon: string, says: string }[] = [
  { state: 'outdated', icon: 'i-lucide-circle-arrow-up', says: 'behind where they came from' },
  { state: 'edited', icon: 'i-lucide-pencil', says: 'changed since they were installed' }
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
      <SnapshotGate v-slot="{ snapshot }">
        <div class="flex flex-wrap items-baseline gap-x-3 gap-y-1">
          <h2 class="text-base font-semibold text-highlighted">
            {{ snapshot.workspace.name }}
          </h2>
          <code
            class="break-all font-mono text-xs text-muted"
            data-testid="workspace-path"
          >{{ snapshot.workspace.path }}</code>
          <UBadge
            :label="snapshot.workspace.reason"
            color="neutral"
            variant="outline"
            size="sm"
            class="font-mono"
            :title="`How this workspace was chosen: ${snapshot.workspace.reason}`"
          />
        </div>

        <UAlert
          v-if="!snapshot.workspace.made"
          color="info"
          variant="subtle"
          icon="i-lucide-info"
          title="Not made yet"
          description="This is the default workspace as it will be: the first command that writes makes it here."
          data-testid="unmade"
        />

        <ul
          class="grid grid-cols-[repeat(auto-fill,minmax(9rem,1fr))] gap-3"
          aria-label="What the workspace holds"
          data-testid="cards"
        >
          <li data-testid="card-projects">
            <StatCard
              label="projects"
              :value="snapshot.summary.projects"
              icon="i-lucide-folder-git-2"
              to="/projects"
            >
              linked, each folder once
            </StatCard>
          </li>
          <li data-testid="card-stale">
            <StatCard
              label="stale"
              :value="snapshot.summary.stale"
              icon="i-lucide-link-2-off"
              :to="{ path: '/projects', query: { health: 'stale' } }"
              warns
            >
              linked, opening no project
            </StatCard>
          </li>
          <li
            v-for="kind in snapshot.kinds"
            :key="kind"
            :data-testid="`card-kind-${kind}`"
          >
            <StatCard
              :label="kind"
              :value="ofKind(snapshot.summary.held, kind)"
              icon="i-lucide-boxes"
            >
              in projects; {{ ofKind(snapshot.summary.library, kind) }} in the library
            </StatCard>
          </li>
          <li
            v-for="asking in ASKING"
            :key="asking.state"
            :data-testid="`card-${asking.state}`"
          >
            <StatCard
              :label="asking.state"
              :value="inState(snapshot.summary.held, asking.state)"
              :icon="asking.icon"
              :to="{ path: '/projects', query: { state: asking.state } }"
              warns
            >
              {{ asking.says }}
            </StatCard>
          </li>
          <li data-testid="card-shelf">
            <StatCard
              label="shelf"
              :value="snapshot.summary.shelf.total"
              icon="i-lucide-library"
            >
              gathered: {{ snapshot.summary.shelf.kinds.map(counted => `${counted.count} ${counted.kind}`).join(', ') }}
            </StatCard>
          </li>
        </ul>

        <section
          v-if="starting(snapshot).length > 0"
          aria-labelledby="start-heading"
          class="flex flex-col gap-2"
          data-testid="start"
        >
          <h2
            id="start-heading"
            class="text-sm font-medium text-highlighted"
          >
            Each of these is a command away
          </h2>
          <CommandHint :commands="starting(snapshot)" />
        </section>

        <WarningsPanel :snapshot="snapshot" />
      </SnapshotGate>
    </template>
  </UDashboardPanel>
</template>
