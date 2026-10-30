<script setup lang="ts">
import { nextTick, watch } from 'vue'
import { useRoute } from 'vue-router'
import type { Does } from '@/api/generated/Does'
import type { Holder } from '@/api/generated/Holder'
import type { Pending } from '@/api/generated/Pending'
import type { ProjectUpdates } from '@/api/generated/ProjectUpdates'
import type { State } from '@/api/generated/State'
import type { Updates } from '@/api/generated/Updates'
import { useRunner } from '@/api/commands'
import { useSnapshot } from '@/api/snapshot'
import { TransportError, type Task } from '@/api/transport'
import { useUpdates } from '@/api/updates'
import AppHeader from '@/components/AppHeader.vue'
import CommandHint, { flayer, making } from '@/components/CommandHint.vue'
import SnapshotGate from '@/components/SnapshotGate.vue'
import StateBadge from '@/components/StateBadge.vue'
import { projectLink } from '@/routes'

// What an update would do is core's survey (`update::Updates`), asked for
// beside the snapshot: every list, count and reason below is its, laid out
// by state, the states in the order it gives them, and filtered, never
// worked out here.
const { data: updates, error } = useUpdates()
// The snapshot, which the gate below draws the sections behind.
const { data: gated } = useSnapshot()
const runner = useRunner()
const { busy } = runner

// A link names one state's list by its hash, as the projects table's
// outdated badge does. The router scrolls to none, and the list is drawn
// only once both the snapshot and the survey are answered, after the
// navigation is done, so the page brings it into view itself once it is
// there: once for each hash it is opened at, and not again when a live
// update answers the survey again and somebody has scrolled away since.
const route = useRoute()
let reached: string | undefined
watch([() => route.hash, updates, gated], async ([hash]) => {
  if (!hash || hash === reached) {
    return
  }
  await nextTick()
  const target = document.getElementById(decodeURIComponent(hash.slice(1)))
  if (target) {
    reached = hash
    target.scrollIntoView()
  }
}, { immediate: true })

/** One installation, and the project it is in. */
interface Row {
  project: ProjectUpdates
  pending: Pending
}

/** Each installation standing in `state`, project by project in the order core lists them. */
function inState(updates: Updates, state: State): Row[] {
  return updates.projects.flatMap(project => project.installations
    .filter(pending => pending.state === state)
    .map(pending => ({ project, pending })))
}

/** How a task names a project: by its entry, which the server turns into its folder. */
function holder(project: ProjectUpdates): Holder {
  return { type: 'project', entry: project.entry }
}

/** An installation as a command line names it, `kind:name`. */
function reference(pending: Pending): string {
  return `${pending.kind}:${pending.name}`
}

/** What a change offered beside a row is: its words, its icon and the task it sends. */
interface Offer {
  label: string
  icon: string
  task: Task
}

/**
 * What each step core gave offers, a word at a time: an update for what an
 * update writes or leaves for an edit, and an uninstall for what nothing
 * offers any more, either forced where core says only a forced one reaches
 * the copy (`forced`), which the server's preview makes a confirmation.
 * Nothing for a step this build knows no change for, a place an install
 * has to be told (`unsure`), whose reason says what to type, or one core
 * adds: the row is still drawn, by its word.
 */
function offer({ project, pending }: Row): Offer | undefined {
  const force = pending.forced
  switch (pending.does) {
    case 'refresh':
    case 'put_back':
    case 'leave':
      return {
        label: force ? 'Force update' : 'Update',
        icon: force ? 'i-lucide-pencil-off' : 'i-lucide-refresh-cw',
        task: { type: 'update', holder: holder(project), reference: reference(pending), force }
      }
    case 'orphaned':
      return {
        label: force ? 'Force uninstall' : 'Uninstall',
        icon: 'i-lucide-package-minus',
        task: { type: 'uninstall', projects: [project.entry], kind: pending.kind, name: pending.name, force }
      }
    default:
      return undefined
  }
}

// What an update does, said for each of core's words, and nothing it
// decides: which one an installation gets is core's. Partial, so a step
// core adds is shown by its word.
const DOES: Partial<Record<Does, string>> = {
  keep: 'nothing to write',
  refresh: 'written again, as its origin offers it now',
  put_back: 'put back, and every other copy brought up to date',
  write_over: 'written over, the change with it',
  leave: 'left alone, every copy of it',
  orphaned: 'left as it is: nothing offers it to write',
  unsure: 'left as it is: where it came from cannot be told'
}

/** Every project, `flayer update --all --yes`. */
const ALL: Task = { type: 'update', holder: { type: 'workspace' }, reference: null, force: false }

/** Every installation nothing offers any more, out of every project. */
const ORPHANS: Task = { type: 'uninstall_orphaned', holder: { type: 'workspace' } }

function ask(task: Task) {
  void runner.ask(task)
}

/** Why the survey was refused, with its code. */
function why(error: unknown): string {
  return error instanceof TransportError && error.code ? `${error.code}: ${error.message}` : String(error)
}
</script>

<template>
  <UDashboardPanel
    id="updates"
    role="main"
    data-testid="updates"
  >
    <template #header>
      <AppHeader
        title="Updates"
        icon="i-lucide-circle-arrow-up"
      >
        <UButton
          label="Update all"
          icon="i-lucide-refresh-cw"
          size="sm"
          :disabled="busy || !updates?.writes"
          data-testid="update-all"
          @click="ask(ALL)"
        />
      </AppHeader>
    </template>

    <template #body>
      <SnapshotGate v-slot="{ snapshot }">
        <p class="max-w-3xl text-sm text-muted">
          What updating every project the workspace links would do, as <code class="font-mono text-xs">flayer update --all</code> says before it is told <code class="font-mono text-xs">--yes</code>:
          what is behind where it came from is written again, a copy gone is put back, and a copy changed since it was installed is left alone unless the update is forced.
        </p>

        <UAlert
          v-if="error && !updates"
          color="error"
          variant="subtle"
          icon="i-lucide-circle-alert"
          title="What an update would do could not be read"
          :description="why(error)"
          data-testid="updates-unread"
        />
        <p
          v-else-if="!updates"
          role="status"
          class="text-sm text-muted"
          data-testid="updates-reading"
        >
          Reading what an update would do
        </p>

        <template v-else>
          <ul
            v-if="updates.unread.length > 0"
            class="flex flex-col gap-2"
            aria-label="Entries that open no project"
            data-testid="unread"
          >
            <li
              v-for="(said, index) in updates.unread"
              :key="index"
            >
              <UAlert
                color="warning"
                variant="subtle"
                icon="i-lucide-link-2-off"
                :title="said.code"
                :description="`${said.message}: what it holds cannot be updated.`"
                :ui="{ title: 'font-mono text-xs' }"
              />
            </li>
          </ul>

          <ul
            class="flex flex-wrap gap-2"
            aria-label="How many installations stand in each state"
            data-testid="counts"
          >
            <li
              v-for="counted in updates.states"
              :key="counted.state"
              :data-testid="`count-${counted.state}`"
            >
              <a
                :href="`#state-${counted.state}`"
                class="inline-flex items-center gap-1.5 rounded-sm focus-visible:outline-2 focus-visible:outline-primary"
              >
                <StateBadge :state="counted.state" />
                <span
                  class="font-mono text-xs tabular-nums"
                  data-testid="count"
                >{{ counted.count }}</span>
              </a>
            </li>
          </ul>

          <div
            v-if="updates.projects.length === 0"
            class="flex flex-col items-start gap-3"
            data-testid="nothing"
          >
            <p class="text-muted">
              Everything installed in the projects the workspace links is current: an update has nothing to do.
            </p>
            <CommandHint
              :commands="[
                ...making(snapshot.workspace),
                { line: flayer(snapshot.workspace, 'update --all'), does: 'says so too' }
              ]"
            />
          </div>

          <template v-else>
            <section
              aria-labelledby="by-project-heading"
              class="flex flex-col gap-2"
              data-testid="by-project"
            >
              <h2
                id="by-project-heading"
                class="text-sm font-medium text-highlighted"
              >
                By project
              </h2>
              <ul class="flex flex-col divide-y divide-default rounded-lg border border-default">
                <li
                  v-for="project in updates.projects"
                  :key="project.entry"
                  class="flex flex-wrap items-center justify-between gap-2 px-3 py-2"
                  data-testid="project"
                >
                  <div class="flex min-w-0 flex-wrap items-baseline gap-x-2">
                    <RouterLink
                      :to="projectLink(project.entry)"
                      class="rounded-sm font-medium text-highlighted hover:text-primary focus-visible:outline-2 focus-visible:outline-primary"
                      data-testid="project-name"
                    >
                      {{ project.name }}
                    </RouterLink>
                    <span
                      v-if="project.entry !== project.name"
                      class="font-mono text-xs text-dimmed"
                    >{{ project.entry }}</span>
                  </div>
                  <UButton
                    v-if="project.writes"
                    :label="`Update ${project.name}`"
                    icon="i-lucide-refresh-cw"
                    color="neutral"
                    variant="outline"
                    size="xs"
                    :disabled="busy"
                    data-testid="update-project"
                    @click="ask({ type: 'update', holder: holder(project), reference: null, force: false })"
                  />
                  <span
                    v-else
                    class="text-xs text-muted"
                    data-testid="writes-nothing"
                  >An update writes nothing here.</span>
                </li>
              </ul>
            </section>

            <section
              v-for="counted in updates.states"
              :id="`state-${counted.state}`"
              :key="counted.state"
              :aria-labelledby="`state-${counted.state}-heading`"
              class="flex scroll-mt-4 flex-col gap-2"
              :data-testid="`state-${counted.state}`"
            >
              <div class="flex flex-wrap items-center justify-between gap-2">
                <h2
                  :id="`state-${counted.state}-heading`"
                  class="flex items-center gap-2 text-sm font-medium text-highlighted"
                >
                  <StateBadge :state="counted.state" />
                  <span class="font-mono text-xs tabular-nums text-muted">{{ counted.count }}</span>
                </h2>
                <UButton
                  v-if="counted.state === 'orphaned' && counted.count > 0"
                  label="Uninstall every orphaned one"
                  icon="i-lucide-package-minus"
                  color="neutral"
                  variant="outline"
                  size="xs"
                  :disabled="busy"
                  data-testid="uninstall-orphans"
                  @click="ask(ORPHANS)"
                />
              </div>

              <p
                v-if="counted.count === 0"
                class="text-xs text-muted"
                data-testid="none"
              >
                Nothing installed is {{ counted.state }}.
              </p>
              <div
                v-else
                class="overflow-x-auto rounded-lg border border-default"
                tabindex="0"
                role="region"
                :aria-label="`What is ${counted.state}`"
              >
                <table class="w-full min-w-max text-left">
                  <caption class="sr-only">
                    Each installation that is {{ counted.state }}: the project it is in, where it came from, and what an update does with it, or why it leaves it.
                  </caption>
                  <thead class="bg-elevated/40 font-mono text-xs text-muted">
                    <tr>
                      <th
                        scope="col"
                        class="px-3 py-2 font-medium"
                      >
                        artifact
                      </th>
                      <th
                        scope="col"
                        class="px-3 py-2 font-medium"
                      >
                        project
                      </th>
                      <th
                        scope="col"
                        class="px-3 py-2 font-medium"
                      >
                        origin
                      </th>
                      <th
                        scope="col"
                        class="px-3 py-2 font-medium"
                      >
                        an update
                      </th>
                      <th
                        scope="col"
                        class="px-3 py-2 font-medium"
                      >
                        <span class="sr-only">change</span>
                      </th>
                    </tr>
                  </thead>
                  <tbody class="divide-y divide-default">
                    <tr
                      v-for="row in inState(updates, counted.state)"
                      :key="JSON.stringify([row.project.entry, reference(row.pending)])"
                      class="align-top"
                      data-testid="row"
                    >
                      <th
                        scope="row"
                        class="px-3 py-2 font-normal"
                      >
                        <span class="flex items-center gap-2">
                          <UBadge
                            :label="row.pending.kind"
                            color="neutral"
                            variant="outline"
                            size="sm"
                            class="font-mono"
                          />
                          <span
                            class="font-mono text-sm text-highlighted"
                            data-testid="name"
                          >{{ row.pending.name }}</span>
                        </span>
                      </th>
                      <td class="px-3 py-2">
                        <RouterLink
                          :to="projectLink(row.project.entry)"
                          class="rounded-sm text-sm hover:text-primary focus-visible:outline-2 focus-visible:outline-primary"
                          data-testid="project"
                        >
                          {{ row.project.name }}
                        </RouterLink>
                      </td>
                      <td
                        class="px-3 py-2 font-mono text-xs"
                        data-testid="origin"
                      >
                        <span v-if="row.pending.origin">{{ row.pending.origin }}</span>
                        <span
                          v-else
                          class="text-dimmed"
                        >not kept</span>
                      </td>
                      <td class="max-w-xl px-3 py-2 text-xs">
                        <div class="flex flex-col gap-1">
                          <span data-testid="does">
                            <span class="font-mono text-muted">{{ row.pending.does }}</span>:
                            {{ DOES[row.pending.does] ?? row.pending.does }}
                          </span>
                          <span
                            v-if="row.pending.copies.length > 0"
                            class="flex flex-wrap gap-1"
                          >
                            <code
                              v-for="copy in row.pending.copies"
                              :key="copy"
                              class="font-mono text-dimmed"
                              data-testid="copy"
                            >{{ copy }}</code>
                          </span>
                          <span
                            v-if="row.pending.left"
                            class="text-muted"
                            data-testid="left"
                          >
                            <span class="me-1 font-mono font-medium text-warning">{{ row.pending.left.code }}<span class="sr-only">:</span></span>
                            {{ row.pending.left.message }}
                          </span>
                        </div>
                      </td>
                      <td class="px-3 py-2 text-right">
                        <UButton
                          v-if="offer(row)"
                          :label="offer(row)!.label"
                          :icon="offer(row)!.icon"
                          :aria-label="`${offer(row)!.label}: ${row.pending.kind} ${row.pending.name} in ${row.project.name}`"
                          color="neutral"
                          variant="outline"
                          size="xs"
                          :disabled="busy"
                          data-testid="change"
                          @click="ask(offer(row)!.task)"
                        />
                      </td>
                    </tr>
                  </tbody>
                </table>
              </div>
            </section>
          </template>
        </template>
      </SnapshotGate>
    </template>
  </UDashboardPanel>
</template>
