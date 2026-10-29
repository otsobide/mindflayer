<script setup lang="ts">
import { computed } from 'vue'
import { useRoute } from 'vue-router'
import type { Held } from '@/api/generated/Held'
import type { Kind } from '@/api/generated/Kind'
import type { Project } from '@/api/generated/Project'
import type { Snapshot } from '@/api/generated/Snapshot'
import { useSnapshot } from '@/api/snapshot'
import AgentBadges from '@/components/AgentBadges.vue'
import AppHeader from '@/components/AppHeader.vue'
import CommandHint from '@/components/CommandHint.vue'
import HealthBadge from '@/components/HealthBadge.vue'
import SnapshotGate from '@/components/SnapshotGate.vue'
import StateBadge from '@/components/StateBadge.vue'

const route = useRoute()

/** The entry the address names, as the registry spells it (see routes.ts). */
const entry = computed(() => {
  const value = route.query.entry
  return (Array.isArray(value) ? value[0] : value) ?? undefined
})

/** The project at that entry, if the workspace links one there. */
function find(snapshot: Snapshot): Project | undefined {
  return entry.value === undefined
    ? undefined
    : snapshot.projects.find(project => (project.entry ?? project.path) === entry.value)
}

const { data } = useSnapshot()
const project = computed(() => (data.value ? find(data.value) : undefined))
const title = computed(() => project.value?.name ?? entry.value ?? 'Project')

/** Its artifacts of `kind`, in the snapshot's order: grouped, never sorted here. */
function ofKind(project: Project, kind: Kind): Held[] {
  return project.artifacts.filter(held => held.kind === kind)
}

function count(project: Project, kind: Kind): number {
  return project.tally.kinds.find(counted => counted.kind === kind)?.count ?? 0
}

/** `a`, `a and b`, `a, b and c`: a list said in words. */
function spelled(words: string[]): string {
  return words.length < 2 ? words.join('') : `${words.slice(0, -1).join(', ')} and ${words.at(-1)}`
}
</script>

<template>
  <UDashboardPanel
    id="project"
    role="main"
    data-testid="project"
  >
    <template #header>
      <AppHeader
        :title="title"
        icon="i-lucide-folder-git-2"
      />
    </template>

    <template #body>
      <SnapshotGate v-slot="{ snapshot }">
        <UBreadcrumb
          :items="[{ label: 'Projects', icon: 'i-lucide-folder-git-2', to: '/projects' }, { label: title }]"
        />

        <div
          v-if="!project"
          class="flex flex-col items-start gap-3"
          data-testid="no-project"
        >
          <p class="text-muted">
            <template v-if="entry === undefined">
              No project is named: the address says no entry.
            </template>
            <template v-else>
              {{ snapshot.workspace.name }} links no project as <code class="font-mono">{{ entry }}</code>.
            </template>
          </p>
          <UButton
            label="Every project"
            icon="i-lucide-arrow-left"
            to="/projects"
            color="neutral"
            variant="outline"
            size="sm"
          />
        </div>

        <template v-else>
          <div class="flex flex-wrap items-center gap-3">
            <h2 class="text-base font-semibold text-highlighted">
              {{ title }}
            </h2>
            <HealthBadge :health="project.health" />
          </div>

          <dl
            class="grid grid-cols-[max-content_1fr] gap-x-6 gap-y-2 text-sm"
            data-testid="facts"
          >
            <dt class="font-mono text-xs text-muted">
              entry
            </dt>
            <dd class="font-mono text-xs">
              {{ project.entry ?? 'none' }}
            </dd>
            <dt class="font-mono text-xs text-muted">
              path
            </dt>
            <dd class="break-all font-mono text-xs">
              {{ project.path }}
            </dd>
            <template v-if="!project.stale">
              <dt class="font-mono text-xs text-muted">
                agents
              </dt>
              <dd>
                <AgentBadges :agents="project.agents" />
              </dd>
              <dt class="font-mono text-xs text-muted">
                share
              </dt>
              <dd
                class="font-mono text-xs"
                data-testid="share"
              >
                <template v-if="project.settings">
                  {{ project.settings.share }}{{ project.settings.edit_tracked ? ', edits tracked files' : '' }}
                </template>
                <template v-else>
                  none: its marker names no settings
                </template>
              </dd>
              <template
                v-for="targets in project.settings?.targets ?? []"
                :key="targets.kind"
              >
                <dt class="font-mono text-xs text-muted">
                  {{ targets.kind }} targets
                </dt>
                <dd class="font-mono text-xs">
                  {{ targets.folders.join(', ') }}
                </dd>
              </template>
              <dt class="font-mono text-xs text-muted">
                marker
              </dt>
              <dd class="font-mono text-xs">
                .mind/{{ project.marker }}
              </dd>
              <dt class="font-mono text-xs text-muted">
                id
              </dt>
              <dd class="break-all font-mono text-xs">
                {{ project.id ?? 'none' }}
              </dd>
            </template>
          </dl>

          <div
            v-if="project.stale"
            class="flex flex-col items-start gap-3"
            data-testid="stale"
          >
            <UAlert
              color="warning"
              variant="subtle"
              icon="i-lucide-link-2-off"
              title="This entry opens no project"
              :description="project.stale.message"
            />
            <CommandHint
              :commands="[
                { line: `flayer relink ${project.entry ?? project.path} <PATH>`, does: 'follow it to where it moved' },
                { line: `flayer unlink ${project.entry ?? project.path}`, does: 'or let it go' }
              ]"
            />
          </div>

          <template v-else>
            <ul
              v-if="project.unreached.length || project.missed.length || project.misplaced.length || project.failures.length"
              class="flex flex-col gap-2"
              aria-label="What wants looking at"
              data-testid="notices"
            >
              <li
                v-for="unreached in project.unreached"
                :key="`unreached:${unreached.kind}`"
                data-testid="unreached"
              >
                <UAlert
                  color="warning"
                  variant="subtle"
                  icon="i-lucide-eye-off"
                  :title="`Not reached: ${spelled(unreached.agents)}`"
                  :description="`Every ${unreached.kind} it installs goes to ${spelled(unreached.targets)}, where ${spelled(unreached.agents)} never ${unreached.agents.length === 1 ? 'reads' : 'read'}.`"
                />
              </li>
              <li
                v-for="missed in project.missed"
                :key="`missed:${missed.kind}:${missed.name}`"
                data-testid="missed"
              >
                <UAlert
                  color="warning"
                  variant="subtle"
                  icon="i-lucide-eye-off"
                  :title="`Not reached: ${spelled(missed.agents)}`"
                  :description="`The ${missed.kind} ${missed.name} is read nowhere by ${spelled(missed.agents)}.`"
                />
              </li>
              <li
                v-for="(said, index) in [...project.misplaced, ...project.failures]"
                :key="`said:${index}`"
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

            <section
              v-for="kind in snapshot.kinds"
              :key="kind"
              :aria-labelledby="`kind-${kind}`"
              class="flex flex-col gap-2"
              :data-testid="`kind-${kind}`"
            >
              <h2
                :id="`kind-${kind}`"
                class="flex items-center gap-2 font-mono text-sm font-medium text-highlighted"
              >
                {{ kind }}
                <UBadge
                  :label="String(count(project, kind))"
                  color="neutral"
                  variant="soft"
                  size="sm"
                  class="tabular-nums"
                  data-testid="kind-count"
                />
              </h2>

              <div
                v-if="ofKind(project, kind).length === 0"
                class="flex flex-col items-start gap-2"
                data-testid="kind-empty"
              >
                <p class="text-muted">
                  No {{ kind }} here yet. From <code class="break-all font-mono text-xs">{{ project.path }}</code>:
                </p>
                <CommandHint
                  :commands="[
                    { line: `mind install ${kind}:<NAME>`, does: 'one the workspace offers' },
                    { line: `mind add ${kind} <NAME> <DESCRIPTION>`, does: 'one of its own' }
                  ]"
                />
              </div>

              <div
                v-else
                class="overflow-x-auto rounded-lg border border-default"
                tabindex="0"
                role="region"
                :aria-label="`${kind} in ${title}`"
              >
                <table class="w-full min-w-max text-left">
                  <caption class="sr-only">
                    Each {{ kind }} {{ title }} holds: how it stands, which of its agents read it, where it came from, where it is held, and what is wrong with it.
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
                        state
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
                        origin
                      </th>
                      <th
                        scope="col"
                        class="px-3 py-2 font-medium"
                      >
                        targets
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
                      v-for="held in ofKind(project, kind)"
                      :key="held.name"
                      class="align-top"
                      data-testid="artifact"
                    >
                      <th
                        scope="row"
                        class="max-w-80 px-3 py-2 font-normal"
                      >
                        <div class="font-mono text-sm font-medium text-highlighted">
                          {{ held.name }}
                        </div>
                        <div
                          v-if="held.summary"
                          class="text-xs text-muted"
                        >
                          {{ held.summary }}
                        </div>
                      </th>
                      <td class="px-3 py-2">
                        <StateBadge :state="held.state" />
                      </td>
                      <td class="px-3 py-2">
                        <AgentBadges
                          :agents="project.agents"
                          :reached="held.agents"
                        />
                      </td>
                      <td
                        class="px-3 py-2 font-mono text-xs"
                        data-testid="origin"
                      >
                        <span v-if="held.origin">{{ held.origin }}</span>
                        <span
                          v-else
                          class="text-dimmed"
                        >none</span>
                      </td>
                      <td class="px-3 py-2">
                        <ul
                          class="flex flex-col gap-0.5"
                          data-testid="targets"
                        >
                          <li
                            v-for="target in held.targets"
                            :key="target"
                          >
                            <code class="font-mono text-xs text-muted">{{ target }}</code>
                          </li>
                        </ul>
                      </td>
                      <td
                        class="max-w-96 px-3 py-2 text-xs"
                        data-testid="issues"
                      >
                        <ul
                          v-if="held.unread || held.issues.length"
                          class="flex flex-col gap-1"
                        >
                          <li
                            v-if="held.unread"
                            class="flex items-start gap-1.5"
                            data-testid="unread"
                          >
                            <UIcon
                              name="i-lucide-file-x"
                              class="mt-px size-3.5 shrink-0 text-error"
                              aria-hidden="true"
                            />
                            <span>
                              <span class="font-mono text-error">does not read:</span>
                              {{ held.unread.message }}
                              <code class="block font-mono text-dimmed">{{ held.unread.path }}</code>
                            </span>
                          </li>
                          <li
                            v-for="issue in held.issues"
                            :key="`${issue.code}:${issue.message}`"
                            class="flex items-start gap-1.5"
                            data-testid="issue"
                          >
                            <UIcon
                              :name="issue.warning ? 'i-lucide-triangle-alert' : 'i-lucide-circle-x'"
                              class="mt-px size-3.5 shrink-0"
                              :class="issue.warning ? 'text-warning' : 'text-error'"
                              aria-hidden="true"
                            />
                            <span>
                              <span
                                class="font-mono"
                                :class="issue.warning ? 'text-warning' : 'text-error'"
                              >{{ issue.warning ? 'warning:' : 'invalid:' }}</span>
                              {{ issue.message }}
                            </span>
                          </li>
                        </ul>
                        <span
                          v-else
                          class="font-mono text-dimmed"
                        >none</span>
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
