<script setup lang="ts">
import { computed } from 'vue'
import { useRoute } from 'vue-router'
import type { Held } from '@/api/generated/Held'
import type { Kind } from '@/api/generated/Kind'
import type { Snapshot } from '@/api/generated/Snapshot'
import { useSnapshot } from '@/api/snapshot'
import { first, useQueryChange } from '@/address'
import AppHeader from '@/components/AppHeader.vue'
import CommandHint, { flayer, making } from '@/components/CommandHint.vue'
import IssueList from '@/components/IssueList.vue'
import SnapshotGate from '@/components/SnapshotGate.vue'
import StateBadge from '@/components/StateBadge.vue'
import { holding } from '@/library'
import { artifactLink, libraryLink, newLink, projectLink } from '@/routes'

const route = useRoute()
const change = useQueryChange()

// The kind shown and the text it is filtered by are the address's
// (src/address.ts): `?kind=rule&q=git`. No kind named is the first the
// snapshot lists, so the page of a kind core adds is a link like the rest.
const asked = computed(() => first(route.query.kind))
const text = computed({
  get: () => first(route.query.q) ?? '',
  set: (q: string) => change({ q })
})

const { data } = useSnapshot()

/** The kind on screen, when the snapshot knows it. */
function shown(snapshot: Snapshot): Kind | undefined {
  const kind = asked.value ?? snapshot.kinds[0]
  return snapshot.kinds.find(known => known === kind)
}

const kind = computed(() => (data.value ? shown(data.value) : undefined))

function count(snapshot: Snapshot, kind: Kind): number {
  return snapshot.summary.library.kinds.find(counted => counted.kind === kind)?.count ?? 0
}

/** Its own of `kind`, in the snapshot's order. */
function ofKind(snapshot: Snapshot, kind: Kind): Held[] {
  return snapshot.library.artifacts.filter(held => held.kind === kind)
}

/** Those of `kind` the text keeps, by name or summary. */
function rows(snapshot: Snapshot, kind: Kind): Held[] {
  const needle = text.value.trim().toLowerCase()
  return ofKind(snapshot, kind).filter(held => !needle || [held.name, held.summary].some(field => field?.toLowerCase().includes(needle)))
}

const shownRows = computed(() => (data.value && kind.value ? rows(data.value, kind.value) : []))

/** Where the workspace keeps `kind`, as a route from its root. */
function folder(snapshot: Snapshot, kind: Kind): string | undefined {
  return snapshot.library.directories.find(directory => directory.kind === kind)?.path
}
</script>

<template>
  <UDashboardPanel
    id="library"
    role="main"
    data-testid="library"
  >
    <template #header>
      <AppHeader
        title="Library"
        icon="i-lucide-library-big"
      >
        <UButton
          v-if="kind"
          :label="`New ${kind}`"
          icon="i-lucide-file-plus-2"
          size="sm"
          :to="newLink(kind)"
          data-testid="new"
        />
      </AppHeader>
    </template>

    <template #body>
      <SnapshotGate v-slot="{ snapshot }">
        <nav
          aria-label="The library by kind"
          data-testid="kinds"
        >
          <UFieldGroup size="sm">
            <UButton
              v-for="each in snapshot.kinds"
              :key="each"
              :to="libraryLink(each)"
              color="neutral"
              :variant="kind === each ? 'subtle' : 'outline'"
              :aria-current="kind === each ? 'page' : undefined"
              class="font-mono"
              :data-testid="`kind-${each}`"
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
        </nav>

        <p
          v-if="!kind"
          class="text-muted"
          data-testid="no-kind"
        >
          This build of Mindflayer knows no kind called <code class="font-mono">{{ asked }}</code>.
        </p>

        <section
          v-else
          aria-labelledby="own-heading"
          class="flex flex-col gap-3"
          data-testid="own"
        >
          <div class="flex flex-wrap items-baseline gap-x-3 gap-y-1">
            <h2
              id="own-heading"
              class="text-base font-semibold text-highlighted"
            >
              Each {{ kind }} of the workspace's own
            </h2>
            <code
              v-if="folder(snapshot, kind)"
              class="break-all font-mono text-xs text-muted"
              data-testid="folder"
            >{{ folder(snapshot, kind) }}</code>
          </div>

          <ul
            v-if="snapshot.library.failures.length"
            class="flex flex-col gap-2"
            aria-label="What could not be read in the library"
          >
            <li
              v-for="(said, index) in snapshot.library.failures"
              :key="index"
              data-testid="failure"
            >
              <UAlert
                color="warning"
                variant="subtle"
                icon="i-lucide-circle-alert"
                :title="said.code"
                :description="said.message"
                :ui="{ title: 'font-mono text-xs' }"
              />
            </li>
          </ul>

          <div
            v-if="ofKind(snapshot, kind).length === 0"
            class="flex flex-col items-start gap-2"
            data-testid="empty"
          >
            <p class="text-muted">
              No {{ kind }} of the workspace's own yet.
            </p>
            <CommandHint
              :commands="[
                ...making(snapshot.workspace),
                { line: flayer(snapshot.workspace, `add ${kind} <NAME> <DESCRIPTION>`), does: 'one of its own, to install into projects' }
              ]"
            />
          </div>

          <template v-else>
            <UInput
              v-model="text"
              icon="i-lucide-search"
              placeholder="Filter by name or summary"
              :aria-label="`Filter each ${kind} by name or summary`"
              size="sm"
              class="w-full sm:w-64"
              data-testid="filter-text"
            />

            <div
              v-if="shownRows.length"
              class="overflow-x-auto rounded-lg border border-default"
              tabindex="0"
              role="region"
              :aria-label="`The table of each ${kind} of its own`"
            >
              <table
                class="w-full min-w-max text-left"
                data-testid="table"
              >
                <caption class="sr-only">
                  Each {{ kind }} the workspace keeps of its own: what it is for, the projects holding a copy of it and how each copy stands, and what is wrong with it. A name opens it.
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
                      in projects
                    </th>
                    <th
                      scope="col"
                      class="px-3 py-2 font-medium"
                    >
                      issues
                    </th>
                  </tr>
                </thead>
                <tbody class="divide-y divide-default">
                  <tr
                    v-for="held in shownRows"
                    :key="held.name"
                    class="align-top hover:bg-elevated/30"
                    data-testid="row"
                  >
                    <th
                      scope="row"
                      class="max-w-96 px-3 py-2 font-normal"
                    >
                      <RouterLink
                        :to="artifactLink(held.kind, held.name)"
                        class="rounded-sm font-mono text-sm font-medium text-highlighted hover:text-primary focus-visible:outline-2 focus-visible:outline-primary"
                        data-testid="name"
                      >
                        {{ held.name }}
                      </RouterLink>
                      <div
                        v-if="held.summary"
                        class="text-xs text-muted"
                        data-testid="summary"
                      >
                        {{ held.summary }}
                      </div>
                    </th>
                    <td
                      class="px-3 py-2"
                      data-testid="holding"
                    >
                      <ul
                        v-if="holding(snapshot, held).length"
                        class="flex flex-col gap-1"
                      >
                        <li
                          v-for="{ project, copy } in holding(snapshot, held)"
                          :key="project.entry ?? project.path"
                          class="flex items-center gap-2"
                        >
                          <RouterLink
                            :to="projectLink(project.entry ?? project.path)"
                            class="rounded-sm text-sm hover:text-primary focus-visible:outline-2 focus-visible:outline-primary"
                            data-testid="project"
                          >
                            {{ project.name ?? project.entry ?? project.path }}
                          </RouterLink>
                          <StateBadge :state="copy.state" />
                        </li>
                      </ul>
                      <span
                        v-else
                        class="font-mono text-xs text-dimmed"
                      >none</span>
                    </td>
                    <td
                      class="max-w-96 px-3 py-2 text-xs"
                      data-testid="issues"
                    >
                      <IssueList :held="held" />
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
                No {{ kind }} matches <q>{{ text }}</q>.
              </p>
              <UButton
                label="Clear the filter"
                icon="i-lucide-filter-x"
                color="neutral"
                variant="outline"
                size="sm"
                @click="text = ''"
              />
            </div>
          </template>
        </section>

        <section
          aria-labelledby="loaded-heading"
          class="flex flex-col gap-3"
          data-testid="loaded"
        >
          <h2
            id="loaded-heading"
            class="text-base font-semibold text-highlighted"
          >
            Loaded workspaces
          </h2>

          <div
            v-if="snapshot.loaded.length === 0"
            class="flex flex-col items-start gap-2"
            data-testid="no-loaded"
          >
            <p class="text-muted">
              It loads no other workspace, whose own it would offer beside its own.
            </p>
            <CommandHint
              :commands="[
                ...making(snapshot.workspace),
                { line: flayer(snapshot.workspace, 'load <PATH>'), does: 'offer another workspace\'s own here' }
              ]"
            />
          </div>

          <div
            v-else
            class="overflow-x-auto rounded-lg border border-default"
            tabindex="0"
            role="region"
            aria-label="The table of loaded workspaces"
          >
            <table class="w-full min-w-max text-left">
              <caption class="sr-only">
                Each workspace this one loads: where it is, how many of its own it offers, what --from takes for them, and why one does not open.
              </caption>
              <thead class="bg-elevated/40 font-mono text-xs text-muted">
                <tr>
                  <th
                    scope="col"
                    class="px-3 py-2 font-medium"
                  >
                    entry
                  </th>
                  <th
                    scope="col"
                    class="px-3 py-2 font-medium"
                  >
                    path
                  </th>
                  <th
                    scope="col"
                    class="px-3 py-2 text-right font-medium"
                  >
                    offers
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
                    opens
                  </th>
                </tr>
              </thead>
              <tbody class="divide-y divide-default">
                <tr
                  v-for="load in snapshot.loaded"
                  :key="load.entry"
                  class="align-top"
                  data-testid="load"
                >
                  <th
                    scope="row"
                    class="px-3 py-2 font-normal"
                  >
                    <div class="font-medium text-highlighted">
                      {{ load.name ?? load.entry }}
                    </div>
                    <div
                      v-if="load.name"
                      class="font-mono text-xs text-dimmed"
                    >
                      {{ load.entry }}
                    </div>
                  </th>
                  <td class="max-w-80 px-3 py-2">
                    <code class="break-all font-mono text-xs text-muted">{{ load.path }}</code>
                  </td>
                  <td
                    class="px-3 py-2 text-right tabular-nums"
                    data-testid="offers"
                  >
                    {{ load.offers }}
                  </td>
                  <td class="px-3 py-2">
                    <code
                      class="font-mono text-xs"
                      data-testid="origin"
                    >{{ load.origin }}</code>
                  </td>
                  <td
                    class="max-w-80 px-3 py-2 text-xs"
                    data-testid="error"
                  >
                    <span
                      v-if="load.error"
                      class="flex items-start gap-1.5"
                    >
                      <UIcon
                        name="i-lucide-link-2-off"
                        class="mt-px size-3.5 shrink-0 text-warning"
                        aria-hidden="true"
                      />
                      <span><span class="font-mono text-warning">{{ load.error.code }}:</span> {{ load.error.message }}</span>
                    </span>
                    <span
                      v-else
                      class="font-mono text-dimmed"
                    >yes</span>
                  </td>
                </tr>
              </tbody>
            </table>
          </div>
        </section>
      </SnapshotGate>
    </template>
  </UDashboardPanel>
</template>
