<script setup lang="ts">
import type { KnownHere } from '@/api/generated/KnownHere'
import type { LoadedWorkspace } from '@/api/generated/LoadedWorkspace'
import type { Offered } from '@/api/generated/Offered'
import type { Snapshot } from '@/api/generated/Snapshot'
import { useRunner } from '@/api/commands'
import AppHeader from '@/components/AppHeader.vue'
import CommandHint, { flayer, making } from '@/components/CommandHint.vue'
import LoadForm from '@/components/LoadForm.vue'
import SnapshotGate from '@/components/SnapshotGate.vue'
import { assignLink } from '@/routes'

const runner = useRunner()
const { busy } = runner

/**
 * Load `known`, by the path the snapshot lists it by: the TUI's load form's
 * task for that path typed (`flayer load <path>`), offered only where core
 * says it can be loaded (`loadable`), so neither this workspace, which a
 * load refuses, nor one it loads already.
 */
function loadKnown(known: KnownHere) {
  void runner.ask({ type: 'load', path: known.path })
}

/**
 * Stop offering what `load` offers, by the entry its marker spells it by:
 * the TUI's task for it picked in the load form (`flayer unload <entry>`),
 * which the preview makes a question. What was installed from it stays.
 */
function unload(load: LoadedWorkspace) {
  void runner.ask({ type: 'unload', entry: load.entry })
}

/**
 * What `load` offers, in the order the snapshot lists every offer: those
 * whose origin is the one core gives it, which a second spelling of one
 * workspace shares with the first, as it shares its count.
 */
function offered(snapshot: Snapshot, load: LoadedWorkspace): Offered[] {
  return snapshot.offers.filter(offer => offer.origin === load.origin)
}

/**
 * How many projects `known` links, and how many of those open none, both
 * as core counts them, in figures as the page gives its other counts ("2 to
 * install"), not in `flayer workspaces`' words ("no projects; the default
 * one"), whose second half the badges beside it say.
 */
function links(known: KnownHere): string {
  const projects = `${known.projects} project${known.projects === 1 ? '' : 's'}`
  return known.stale_projects > 0 ? `${projects}, ${known.stale_projects} stale` : projects
}
</script>

<template>
  <UDashboardPanel
    id="loads"
    role="main"
    data-testid="loads"
  >
    <template #header>
      <AppHeader
        title="Loaded workspaces"
        icon="i-lucide-layers"
      />
    </template>

    <template #body>
      <SnapshotGate v-slot="{ snapshot }">
        <section
          aria-labelledby="loaded-heading"
          class="flex flex-col gap-3"
          data-testid="loaded"
        >
          <div class="flex flex-col gap-1">
            <h2
              id="loaded-heading"
              class="text-base font-semibold text-highlighted"
            >
              What it loads
            </h2>
            <p class="text-sm text-muted">
              Another workspace's own skills and rules, offered to the projects beside the library's and the shelf's, and read from where that workspace keeps them each time one is installed.
            </p>
          </div>

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

          <ul
            v-else
            class="flex flex-col gap-3"
            aria-label="Every workspace it loads, in the order its marker lists them"
          >
            <li
              v-for="load in snapshot.loaded"
              :key="load.entry"
              class="flex flex-col gap-2 rounded-lg border border-default p-4"
              data-testid="load"
            >
              <div class="flex flex-wrap items-center gap-2">
                <h3
                  class="font-medium text-highlighted"
                  data-testid="name"
                >
                  {{ load.name ?? load.entry }}
                </h3>
                <UBadge
                  v-if="load.error"
                  label="does not open"
                  color="warning"
                  variant="subtle"
                  size="sm"
                  icon="i-lucide-link-2-off"
                  class="font-mono"
                  data-testid="unopened"
                />
                <UButton
                  label="Unload"
                  icon="i-lucide-unplug"
                  color="neutral"
                  variant="outline"
                  size="xs"
                  class="ms-auto"
                  :aria-label="`Unload ${load.name ?? load.entry}`"
                  :disabled="busy"
                  data-testid="unload"
                  @click="unload(load)"
                />
              </div>
              <dl class="grid grid-cols-[max-content_1fr] gap-x-6 gap-y-1 text-xs">
                <dt class="font-mono text-muted">
                  entry
                </dt>
                <dd>
                  <code
                    class="break-all font-mono"
                    data-testid="entry"
                  >{{ load.entry }}</code>
                </dd>
                <dt class="font-mono text-muted">
                  path
                </dt>
                <dd>
                  <code
                    class="break-all font-mono"
                    data-testid="path"
                  >{{ load.path }}</code>
                </dd>
                <dt class="font-mono text-muted">
                  --from
                </dt>
                <dd>
                  <code
                    class="break-all font-mono"
                    data-testid="origin"
                  >{{ load.origin }}</code>
                </dd>
                <template v-if="load.error">
                  <dt class="font-mono text-muted">
                    why
                  </dt>
                  <dd
                    class="flex items-start gap-1.5"
                    data-testid="error"
                  >
                    <span><span class="font-mono text-warning">{{ load.error.code }}:</span> {{ load.error.message }}</span>
                  </dd>
                </template>
                <dt class="font-mono text-muted">
                  offers
                </dt>
                <dd
                  class="flex flex-col items-start gap-1.5"
                  data-testid="offers"
                >
                  <span
                    class="tabular-nums"
                    data-testid="count"
                  >{{ load.offers }} to install</span>
                  <ul
                    v-if="offered(snapshot, load).length"
                    class="flex flex-col gap-1"
                    :aria-label="`What ${load.name ?? load.entry} offers`"
                  >
                    <li
                      v-for="offer in offered(snapshot, load)"
                      :key="`${offer.kind}:${offer.name}`"
                      class="flex flex-wrap items-baseline gap-x-2"
                      data-testid="offer"
                    >
                      <!-- On one line, so a reader hears the space between them. -->
                      <code class="font-mono text-highlighted">{{ offer.kind }}:{{ offer.name }}</code> <span
                        v-if="offer.summary"
                        class="text-muted"
                      >{{ offer.summary }}</span>
                    </li>
                  </ul>
                  <RouterLink
                    v-if="load.offers > 0"
                    :to="assignLink(load.origin)"
                    class="inline-flex items-center gap-1 rounded-sm text-primary hover:underline focus-visible:outline-2 focus-visible:outline-primary"
                    data-testid="assign"
                  >
                    <UIcon
                      name="i-lucide-grid-3x3"
                      class="size-3.5"
                      aria-hidden="true"
                    />
                    Assign them to projects
                  </RouterLink>
                </dd>
              </dl>
            </li>
          </ul>
        </section>

        <section
          aria-labelledby="known-heading"
          class="flex flex-col gap-3"
          data-testid="known"
        >
          <div class="flex flex-col gap-1">
            <h2
              id="known-heading"
              class="text-base font-semibold text-highlighted"
            >
              Workspaces this machine knows
            </h2>
            <p class="text-sm text-muted">
              The default workspace, in your home, remembers each one <code class="font-mono">flayer init</code> makes once the default is made itself. Load one to offer its own here, or name any other by its path below.
            </p>
          </div>

          <div
            v-if="snapshot.known.length === 0"
            class="flex flex-col items-start gap-2"
            data-testid="no-known"
          >
            <!--
              Core lists none only while the default is not made and is the
              one on screen: a workspace `flayer init` made before it is a
              workspace still, which the default never remembered.
            -->
            <p
              class="text-muted"
              data-testid="none-known"
            >
              The default workspace is not made yet, so it remembers none: load one made before it by its path below.
            </p>
            <CommandHint :commands="making(snapshot.workspace)" />
          </div>

          <ul
            v-else
            class="flex flex-col divide-y divide-default rounded-lg border border-default"
            aria-label="Every workspace this machine knows: the default first, then each the default remembers"
          >
            <li
              v-for="known in snapshot.known"
              :key="known.path"
              class="flex flex-wrap items-center gap-x-3 gap-y-1 px-4 py-2.5"
              data-testid="known-workspace"
            >
              <div class="flex min-w-0 flex-1 flex-col">
                <span
                  v-if="known.name !== null"
                  class="font-medium text-highlighted"
                  data-testid="name"
                >{{ known.name }}</span>
                <code
                  class="break-all font-mono text-xs"
                  :class="known.name === null ? 'text-highlighted' : 'text-muted'"
                  data-testid="path"
                >{{ known.path }}</code>
                <span
                  v-if="known.stale"
                  class="text-xs"
                  data-testid="stale"
                >{{ known.stale }}</span>
              </div>
              <ul
                class="flex flex-wrap items-center gap-1.5"
                aria-label="What it is"
                data-testid="badges"
              >
                <li v-if="known.default">
                  <UBadge
                    label="default"
                    color="neutral"
                    variant="outline"
                    size="sm"
                    icon="i-lucide-house"
                    class="font-mono"
                  />
                </li>
                <li v-if="known.in_use">
                  <UBadge
                    label="this workspace"
                    color="primary"
                    variant="subtle"
                    size="sm"
                    icon="i-lucide-map-pin"
                    class="font-mono"
                  />
                </li>
                <li v-if="known.loaded_as !== null">
                  <UBadge
                    :label="`loaded as ${known.loaded_as}`"
                    color="success"
                    variant="subtle"
                    size="sm"
                    icon="i-lucide-layers"
                    class="font-mono"
                  />
                </li>
                <li v-if="known.stale">
                  <UBadge
                    label="does not open"
                    color="warning"
                    variant="subtle"
                    size="sm"
                    icon="i-lucide-link-2-off"
                    class="font-mono"
                  />
                </li>
                <li
                  v-if="!known.stale"
                  class="font-mono text-xs text-muted tabular-nums"
                  data-testid="projects"
                >
                  {{ links(known) }}
                </li>
              </ul>
              <UButton
                v-if="known.loadable"
                label="Load"
                icon="i-lucide-layers"
                size="xs"
                :aria-label="`Load ${known.name ?? known.path}`"
                :disabled="busy"
                data-testid="load-known"
                @click="loadKnown(known)"
              />
            </li>
          </ul>

          <LoadForm />
        </section>
      </SnapshotGate>
    </template>
  </UDashboardPanel>
</template>
