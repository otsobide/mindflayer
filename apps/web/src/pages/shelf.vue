<script setup lang="ts">
import { computed } from 'vue'
import { useRoute } from 'vue-router'
import type { Kind } from '@/api/generated/Kind'
import type { Shelved } from '@/api/generated/Shelved'
import type { Snapshot } from '@/api/generated/Snapshot'
import { useSnapshot } from '@/api/snapshot'
import { first, useQueryChange } from '@/address'
import AppHeader from '@/components/AppHeader.vue'
import CommandHint, { flayer, making } from '@/components/CommandHint.vue'
import SnapshotGate from '@/components/SnapshotGate.vue'
import { instant, when } from '@/time'

const route = useRoute()
const change = useQueryChange()

// What the shelf is filtered by is the address's (src/address.ts):
// `?kind=skill&q=format`.
const kind = computed(() => first(route.query.kind))
const text = computed({
  get: () => first(route.query.q) ?? '',
  set: (q: string) => change({ q })
})

function count(snapshot: Snapshot, kind: Kind): number {
  return snapshot.summary.shelf.kinds.find(counted => counted.kind === kind)?.count ?? 0
}

/** What the filters keep, in the snapshot's order: by kind, name, source. */
function rows(snapshot: Snapshot): Shelved[] {
  const needle = text.value.trim().toLowerCase()
  return snapshot.shelf
    .filter(shelved => !kind.value || shelved.kind === kind.value)
    .filter(shelved => !needle || [shelved.name, shelved.summary, shelved.origin].some(field => field?.toLowerCase().includes(needle)))
}

const { data } = useSnapshot()
const shown = computed(() => (data.value ? rows(data.value) : []))

/** A commit as a person refers to it; the whole of it is its title. */
function short(revision: string): string {
  return revision.slice(0, 7)
}
</script>

<template>
  <UDashboardPanel
    id="shelf"
    role="main"
    data-testid="shelf"
  >
    <template #header>
      <AppHeader
        title="Shelf"
        icon="i-lucide-package"
      />
    </template>

    <template #body>
      <SnapshotGate v-slot="{ snapshot }">
        <div
          v-if="snapshot.shelf.length === 0"
          class="flex flex-col items-start gap-3"
          data-testid="empty"
        >
          <p class="text-muted">
            Nothing is gathered onto {{ snapshot.workspace.name }}'s shelf yet.
          </p>
          <CommandHint
            :commands="[
              ...making(snapshot.workspace),
              { line: flayer(snapshot.workspace, 'gather git <URL>'), does: 'skills from a repository, onto the shelf' }
            ]"
          />
        </div>

        <template v-else>
          <p class="text-muted">
            What was gathered from elsewhere, to install as it is. Each says where it came from as <code class="font-mono">--from</code> takes it, for when two places offer one name.
          </p>
          <CommandHint
            :commands="[{ line: flayer(snapshot.workspace, 'install -p <PROJECT> <KIND>:<NAME> --from <ORIGIN>'), does: 'one of these into a project' }]"
          />

          <div
            class="flex flex-wrap items-center gap-2"
            data-testid="filters"
          >
            <UInput
              v-model="text"
              icon="i-lucide-search"
              placeholder="Filter by name, summary or origin"
              aria-label="Filter the shelf by name, summary or origin"
              size="sm"
              class="w-full sm:w-72"
              data-testid="filter-text"
            />
            <UFieldGroup
              role="group"
              aria-label="Show the shelf by kind"
              size="sm"
            >
              <UButton
                label="all"
                color="neutral"
                :variant="kind ? 'outline' : 'subtle'"
                :icon="kind ? undefined : 'i-lucide-check'"
                :aria-pressed="!kind"
                class="font-mono"
                data-testid="filter-kind-all"
                @click="change({ kind: undefined })"
              />
              <UButton
                v-for="each in snapshot.kinds"
                :key="each"
                color="neutral"
                :variant="kind === each ? 'subtle' : 'outline'"
                :icon="kind === each ? 'i-lucide-check' : undefined"
                :aria-pressed="kind === each"
                class="font-mono"
                :data-testid="`filter-kind-${each}`"
                @click="change({ kind: each })"
              >
                {{ each }}
                <UBadge
                  :label="String(count(snapshot, each))"
                  color="neutral"
                  variant="soft"
                  size="sm"
                  class="tabular-nums"
                  data-testid="kind-count"
                />
              </UButton>
            </UFieldGroup>
          </div>

          <div
            v-if="shown.length"
            class="overflow-x-auto rounded-lg border border-default"
            tabindex="0"
            role="region"
            aria-label="The table of what is on the shelf"
          >
            <table
              class="w-full min-w-max text-left"
              data-testid="table"
            >
              <caption class="sr-only">
                Everything on the shelf: its kind and name, what it is for, where it came from as --from takes it, the commit it was taken at, when, and where it sits in the workspace.
              </caption>
              <thead class="bg-elevated/40 font-mono text-xs text-muted">
                <tr>
                  <th
                    scope="col"
                    class="px-3 py-2 font-medium"
                  >
                    name
                  </th>
                  <th
                    scope="col"
                    class="px-3 py-2 font-medium"
                  >
                    kind
                  </th>
                  <th
                    scope="col"
                    class="px-3 py-2 font-medium"
                  >
                    --from
                  </th>
                  <th
                    scope="col"
                    class="px-3 py-2 font-medium"
                  >
                    revision
                  </th>
                  <th
                    scope="col"
                    class="px-3 py-2 font-medium"
                  >
                    gathered
                  </th>
                </tr>
              </thead>
              <tbody class="divide-y divide-default">
                <tr
                  v-for="shelved in shown"
                  :key="shelved.path"
                  class="align-top"
                  data-testid="row"
                >
                  <th
                    scope="row"
                    class="max-w-96 px-3 py-2 font-normal"
                  >
                    <div
                      class="font-mono text-sm font-medium text-highlighted"
                      data-testid="name"
                    >
                      {{ shelved.name }}
                    </div>
                    <div
                      v-if="shelved.summary"
                      class="text-xs text-muted"
                      data-testid="summary"
                    >
                      {{ shelved.summary }}
                    </div>
                    <code
                      class="block break-all font-mono text-xs text-dimmed"
                      data-testid="path"
                    >{{ shelved.path }}</code>
                  </th>
                  <td
                    class="px-3 py-2 font-mono text-xs"
                    data-testid="kind"
                  >
                    {{ shelved.kind }}
                  </td>
                  <td class="px-3 py-2">
                    <code
                      class="break-all font-mono text-xs"
                      data-testid="origin"
                    >{{ shelved.origin }}</code>
                  </td>
                  <td
                    class="px-3 py-2 font-mono text-xs"
                    data-testid="revision"
                  >
                    <code
                      v-if="shelved.revision"
                      :title="shelved.revision"
                    >{{ short(shelved.revision) }}</code>
                    <span
                      v-else
                      class="text-dimmed"
                    >none</span>
                  </td>
                  <td class="px-3 py-2 text-xs text-muted">
                    <time
                      :datetime="instant(shelved.gathered_at)"
                      data-testid="gathered"
                    >{{ when(shelved.gathered_at) }}</time>
                  </td>
                </tr>
              </tbody>
            </table>
          </div>

          <div
            v-else
            class="flex flex-col items-start gap-2"
            data-testid="no-match"
          >
            <p class="text-muted">
              Nothing on the shelf matches these filters.
            </p>
            <UButton
              label="Clear the filters"
              icon="i-lucide-filter-x"
              color="neutral"
              variant="outline"
              size="sm"
              @click="change({ q: undefined, kind: undefined })"
            />
          </div>
        </template>
      </SnapshotGate>
    </template>
  </UDashboardPanel>
</template>
