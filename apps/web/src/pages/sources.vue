<script setup lang="ts">
import type { ShelfSource } from '@/api/generated/ShelfSource'
import type { Shelved } from '@/api/generated/Shelved'
import type { Snapshot } from '@/api/generated/Snapshot'
import ActivityFeed from '@/components/ActivityFeed.vue'
import AppHeader from '@/components/AppHeader.vue'
import CommandHint, { flayer, making } from '@/components/CommandHint.vue'
import SnapshotGate from '@/components/SnapshotGate.vue'

/**
 * What `source` supplied of the shelf, in the shelf's order: the rows that
 * came from it, told by the folder it owns, which no two sources share.
 */
function supplied(snapshot: Snapshot, source: ShelfSource): Shelved[] {
  return snapshot.shelf.filter(shelved => shelved.source.directory === source.directory)
}

/** A commit as a person refers to it; the whole of it is its title. */
function short(revision: string): string {
  return revision.slice(0, 7)
}
</script>

<template>
  <UDashboardPanel
    id="sources"
    role="main"
    data-testid="sources"
  >
    <template #header>
      <AppHeader
        title="Sources"
        icon="i-lucide-git-branch"
      />
    </template>

    <template #body>
      <SnapshotGate v-slot="{ snapshot }">
        <section
          aria-labelledby="sources-heading"
          class="flex flex-col gap-3"
          data-testid="list"
        >
          <h2
            id="sources-heading"
            class="text-base font-semibold text-highlighted"
          >
            Where the shelf is filled from
          </h2>

          <div
            v-if="snapshot.sources.length === 0"
            class="flex flex-col items-start gap-2"
            data-testid="empty"
          >
            <p class="text-muted">
              Nothing has been gathered from anywhere yet.
            </p>
            <CommandHint
              :commands="[
                ...making(snapshot.workspace),
                { line: flayer(snapshot.workspace, 'gather git <URL>'), does: 'skills from a repository, onto the shelf' }
              ]"
            />
          </div>

          <ul
            v-else
            class="flex flex-col gap-3"
            aria-label="Every source, by its address"
          >
            <li
              v-for="source in snapshot.sources"
              :key="source.directory"
              class="flex flex-col gap-2 rounded-lg border border-default p-4"
              data-testid="source"
            >
              <div class="flex flex-wrap items-center gap-2">
                <UBadge
                  :label="source.kind"
                  color="neutral"
                  variant="outline"
                  size="sm"
                  class="font-mono"
                  data-testid="kind"
                />
                <code
                  class="break-all font-mono text-sm font-medium text-highlighted"
                  data-testid="url"
                >{{ source.url }}</code>
              </div>
              <dl class="grid grid-cols-[max-content_1fr] gap-x-6 gap-y-1 text-xs">
                <dt class="font-mono text-muted">
                  --from
                </dt>
                <dd>
                  <code
                    class="break-all font-mono"
                    data-testid="origin"
                  >{{ source.origin }}</code>
                </dd>
                <dt class="font-mono text-muted">
                  branch
                </dt>
                <dd
                  class="font-mono"
                  data-testid="reference"
                >
                  <template v-if="source.reference">
                    {{ source.reference }}
                  </template>
                  <span
                    v-else
                    class="font-sans text-muted"
                  >whatever the remote's HEAD points at</span>
                </dd>
                <dt class="font-mono text-muted">
                  folder
                </dt>
                <dd
                  class="font-mono"
                  data-testid="subdirectory"
                >
                  {{ source.subdirectory }}
                </dd>
                <dt class="font-mono text-muted">
                  revision
                </dt>
                <dd
                  class="font-mono"
                  data-testid="revision"
                >
                  <code
                    v-if="source.revision"
                    :title="source.revision"
                  >{{ short(source.revision) }}</code>
                  <span
                    v-else
                    class="font-sans text-muted"
                  >none resolved yet</span>
                </dd>
                <dt class="font-mono text-muted">
                  kept as
                </dt>
                <dd
                  class="break-all font-mono"
                  data-testid="directory"
                >
                  {{ source.directory }}
                </dd>
                <dt class="font-mono text-muted">
                  supplied
                </dt>
                <dd data-testid="supplied">
                  <ul
                    v-if="supplied(snapshot, source).length"
                    class="flex flex-wrap gap-1"
                  >
                    <li
                      v-for="shelved in supplied(snapshot, source)"
                      :key="shelved.path"
                    >
                      <UBadge
                        :label="`${shelved.kind}:${shelved.name}`"
                        color="neutral"
                        variant="soft"
                        size="sm"
                        class="font-mono"
                        data-testid="shelved"
                      />
                    </li>
                  </ul>
                  <span
                    v-else
                    class="text-muted"
                  >nothing on the shelf now</span>
                </dd>
              </dl>
            </li>
          </ul>
        </section>

        <section
          aria-labelledby="activity-heading"
          class="flex flex-col gap-3"
          data-testid="history"
        >
          <h2
            id="activity-heading"
            class="text-base font-semibold text-highlighted"
          >
            What happened lately
          </h2>
          <ActivityFeed :entries="snapshot.history" />
        </section>
      </SnapshotGate>
    </template>
  </UDashboardPanel>
</template>
