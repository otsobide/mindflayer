<script lang="ts">
import type { BadgeProps } from '@nuxt/ui'
import type { GivenTo } from '@/api/generated/GivenTo'
import type { Starts } from '@/api/generated/Starts'

/** How an agent is given a server, as a badge says it: its words, a colour and an icon. */
interface Given {
  label: string
  color: BadgeProps['color']
  icon: string
}

/**
 * How `agent` is given a server, from what core said of it: refused, when
 * it cannot be given it at all; given it without what its entry drops; or
 * given it whole. Words first, so a state is never said by colour alone.
 */
export function givenLook(agent: GivenTo): Given {
  if (agent.refused.length > 0) {
    return { label: 'refused', color: 'error', icon: 'i-lucide-ban' }
  }
  if (agent.dropped.length > 0) {
    return { label: 'given in part', color: 'warning', icon: 'i-lucide-circle-minus' }
  }
  return { label: 'given whole', color: 'success', icon: 'i-lucide-circle-check' }
}

// What each word core says an agent asks before it starts a server means,
// in the words the install's line says it in: the panel's, as a state's
// look is, and a word it does not know shown as it is.
const STARTS: Partial<Record<Starts, string>> = {
  once_approved: 'asks to approve it before it starts it in a session, though a run with -p starts it unasked',
  in_a_trusted_project: 'starts it only in a project its user\'s config marks trusted',
  unasked: 'starts it without asking'
}

/** What an agent asks before it starts a server, in words. */
export function startsSaid(starts: Starts): string {
  return STARTS[starts] ?? starts
}
</script>

<script setup lang="ts">
import { computed } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import type { Held } from '@/api/generated/Held'
import type { LibraryServer } from '@/api/generated/LibraryServer'
import type { Snapshot } from '@/api/generated/Snapshot'
import { useSnapshot } from '@/api/snapshot'
import { first } from '@/address'
import AppHeader from '@/components/AppHeader.vue'
import CommandHint, { weaver, making } from '@/components/CommandHint.vue'
import IssueList from '@/components/IssueList.vue'
import ServerForm from '@/components/ServerForm.vue'
import SnapshotGate from '@/components/SnapshotGate.vue'
import StateBadge from '@/components/StateBadge.vue'
import { holding } from '@/library'
import { artifactLink, editLink, projectLink, serversLink } from '@/routes'

const route = useRoute()
const router = useRouter()

// Which server the form changes is the address's: `?edit=github`. None,
// and the form adds one.
const editing = computed(() => first(route.query.edit))

const { data } = useSnapshot()

/** The library's row of `server`: its summary and what validate says of it. */
function rowOf(snapshot: Snapshot, server: LibraryServer): Held | undefined {
  return snapshot.library.artifacts.find(held => held.kind === 'mcp' && held.name === server.name)
}

/** The server the form changes, as the snapshot lists it. */
const changed = computed(() => data.value?.library.servers.find(server => server.name === editing.value))

/** What doctor finds in each project holding `server`, by the project. */
function examined(snapshot: Snapshot, server: LibraryServer) {
  return holding(snapshot, { kind: 'mcp', name: server.name }).map(({ project, copy }) => ({
    project,
    copy,
    findings: snapshot.doctor.find(each => each.entry !== null && each.entry === project.entry)?.findings ?? []
  }))
}

/** Each of the library's own servers that does not read, which the list above cannot show. */
function unread(snapshot: Snapshot): Held[] {
  return snapshot.library.artifacts.filter(held => held.kind === 'mcp' && !snapshot.library.servers.some(server => server.name === held.name))
}

/** Once the form wrote, the server's page in the list, the form back to adding. */
async function done(name: string) {
  await router.push(serversLink(undefined, `#server-${name}`))
}
</script>

<template>
  <UDashboardPanel
    id="mcp"
    role="main"
    data-testid="mcp"
  >
    <template #header>
      <AppHeader
        title="MCP servers"
        icon="i-lucide-plug"
      >
        <UButton
          label="Assign them"
          icon="i-lucide-grid-3x3"
          color="neutral"
          variant="outline"
          size="sm"
          :to="{ path: '/assign', query: { kind: 'mcp' } }"
          data-testid="to-assign"
        />
      </AppHeader>
    </template>

    <template #body>
      <SnapshotGate v-slot="{ snapshot }">
        <p class="text-sm text-muted">
          The library's MCP servers, a definition each, and how each agent a project names would be given one: what it would run or
          reach, as its own config file would spell it, what it cannot be given, and what it asks before it starts one.
        </p>

        <div
          v-if="snapshot.library.servers.length === 0 && unread(snapshot).length === 0"
          class="flex flex-col items-start gap-2"
          data-testid="empty"
        >
          <p class="text-muted">
            No MCP server of the workspace's own yet: add one below.
          </p>
          <CommandHint
            :commands="[
              ...making(snapshot.workspace),
              { line: weaver(snapshot.workspace, 'add mcp <NAME> <DESCRIPTION> --command <CMD> --secret-env <VAR>'), does: 'a program an agent starts' },
              { line: weaver(snapshot.workspace, 'add mcp <NAME> <DESCRIPTION> --url <URL> --bearer-env <VAR>'), does: 'an address an agent reaches' }
            ]"
          />
        </div>

        <ul
          v-if="unread(snapshot).length > 0"
          class="flex flex-col gap-2"
          aria-label="Servers whose definition does not read"
        >
          <li
            v-for="held in unread(snapshot)"
            :key="held.name"
            class="flex flex-col gap-1 rounded-lg border border-default p-3"
            data-testid="unread-server"
          >
            <RouterLink
              :to="artifactLink(held.kind, held.name)"
              class="self-start rounded-sm font-mono text-sm font-medium text-highlighted hover:text-primary focus-visible:outline-2 focus-visible:outline-primary"
            >
              {{ held.name }}
            </RouterLink>
            <div class="text-xs">
              <IssueList :held="held" />
            </div>
          </li>
        </ul>

        <section
          v-for="server in snapshot.library.servers"
          :id="`server-${server.name}`"
          :key="server.name"
          :aria-labelledby="`server-${server.name}-heading`"
          class="flex flex-col gap-3 rounded-lg border border-default p-4"
          data-testid="server"
        >
          <div class="flex flex-wrap items-start gap-x-3 gap-y-1">
            <div class="flex min-w-0 flex-col gap-0.5">
              <h2
                :id="`server-${server.name}-heading`"
                class="font-mono text-base font-semibold text-highlighted"
              >
                <RouterLink
                  :to="artifactLink('mcp', server.name)"
                  class="rounded-sm hover:text-primary focus-visible:outline-2 focus-visible:outline-primary"
                  data-testid="name"
                >
                  {{ server.name }}
                </RouterLink>
              </h2>
              <p
                v-if="rowOf(snapshot, server)?.summary"
                class="text-sm text-muted"
                data-testid="summary"
              >
                {{ rowOf(snapshot, server)?.summary }}
              </p>
            </div>
            <div class="ms-auto flex gap-2">
              <UButton
                v-if="server.given"
                label="Change how it starts"
                icon="i-lucide-settings-2"
                color="neutral"
                variant="outline"
                size="xs"
                :to="serversLink(server.name, '#server-form-heading')"
                :aria-label="`Change how ${server.name} is started or reached`"
                data-testid="change"
              />
              <UButton
                v-else
                label="Edit its file"
                icon="i-lucide-file-pen-line"
                color="neutral"
                variant="outline"
                size="xs"
                :to="editLink('mcp', server.name)"
                :aria-label="`Edit the file of ${server.name}`"
                data-testid="edit-file"
              />
            </div>
          </div>

          <div
            v-if="rowOf(snapshot, server)?.issues.length"
            class="text-xs"
            data-testid="issues"
          >
            <IssueList :held="rowOf(snapshot, server)!" />
          </div>

          <div
            class="overflow-x-auto rounded-md border border-default"
            tabindex="0"
            role="region"
            :aria-label="`How each agent would be given ${server.name}`"
          >
            <table
              class="w-full min-w-max text-left text-sm"
              data-testid="agents"
            >
              <caption class="sr-only">
                Each agent: whether it can be given {{ server.name }}, what it would run or reach, and what it asks before it starts it.
              </caption>
              <thead class="bg-elevated/40 font-mono text-xs text-muted">
                <tr>
                  <th
                    scope="col"
                    class="px-3 py-2 font-medium"
                  >
                    agent
                  </th>
                  <th
                    scope="col"
                    class="px-3 py-2 font-medium"
                  >
                    given
                  </th>
                  <th
                    scope="col"
                    class="px-3 py-2 font-medium"
                  >
                    runs or reaches
                  </th>
                  <th
                    scope="col"
                    class="px-3 py-2 font-medium"
                  >
                    before it starts it
                  </th>
                </tr>
              </thead>
              <tbody class="divide-y divide-default">
                <tr
                  v-for="agent in server.agents"
                  :key="agent.agent"
                  class="align-top"
                  data-testid="agent"
                >
                  <th
                    scope="row"
                    class="px-3 py-2 font-mono font-normal"
                    data-testid="agent-name"
                  >
                    {{ agent.agent }}
                  </th>
                  <td class="px-3 py-2">
                    <UBadge
                      :label="givenLook(agent).label"
                      :color="givenLook(agent).color"
                      :icon="givenLook(agent).icon"
                      variant="subtle"
                      size="sm"
                      data-testid="given"
                    />
                    <ul
                      v-if="agent.refused.length + agent.dropped.length > 0"
                      class="mt-1 flex max-w-md flex-col gap-1 text-xs"
                    >
                      <li
                        v-for="said in [...agent.refused, ...agent.dropped]"
                        :key="said.code + said.message"
                        data-testid="why"
                      >
                        <code class="font-mono">{{ said.code }}</code>: {{ said.message }}
                      </li>
                    </ul>
                  </td>
                  <td class="px-3 py-2">
                    <code
                      v-if="agent.runs"
                      class="break-all font-mono text-xs"
                      data-testid="runs"
                    ><span class="text-muted">{{ agent.runs.reaches ? 'reaches' : 'runs' }}</span> {{ agent.runs.line }}</code>
                    <span
                      v-else
                      class="text-xs text-dimmed"
                      data-testid="runs-nothing"
                    >nothing: it is given none of it</span>
                  </td>
                  <td
                    class="max-w-sm px-3 py-2 text-xs"
                    data-testid="starts"
                  >
                    {{ startsSaid(agent.starts) }}
                  </td>
                </tr>
              </tbody>
            </table>
          </div>

          <div
            class="flex flex-col gap-2"
            data-testid="holders"
          >
            <h3 class="text-xs font-medium text-muted">
              The projects holding it, and what doctor finds there
            </h3>
            <p
              v-if="examined(snapshot, server).length === 0"
              class="text-xs text-muted"
              data-testid="no-holders"
            >
              No project holds it yet: tick its boxes in Assign.
            </p>
            <ul
              v-else
              class="flex flex-col gap-2"
            >
              <li
                v-for="{ project, copy, findings } in examined(snapshot, server)"
                :key="project.entry ?? project.path"
                class="flex flex-col gap-1"
                data-testid="holder"
              >
                <div class="flex items-center gap-2">
                  <RouterLink
                    :to="projectLink(project.entry ?? project.path)"
                    class="rounded-sm text-sm hover:text-primary focus-visible:outline-2 focus-visible:outline-primary"
                    data-testid="project"
                  >
                    {{ project.name ?? project.entry ?? project.path }}
                  </RouterLink>
                  <StateBadge :state="copy.state" />
                </div>
                <p
                  v-if="findings.length === 0"
                  class="text-xs text-muted"
                  data-testid="no-findings"
                >
                  Doctor finds nothing there.
                </p>
                <ul
                  v-else
                  class="flex flex-col gap-1 text-xs"
                >
                  <li
                    v-for="finding in findings"
                    :key="finding.code + finding.path"
                    class="flex items-start gap-1.5"
                    data-testid="finding"
                  >
                    <UIcon
                      name="i-lucide-stethoscope"
                      class="mt-0.5 size-3.5 shrink-0 text-warning"
                      aria-hidden="true"
                    />
                    <span>
                      <code class="font-mono">{{ finding.code }}</code>: {{ finding.message }}
                      <span class="block text-muted">Fix: {{ finding.fix }}</span>
                    </span>
                  </li>
                </ul>
              </li>
            </ul>
          </div>
        </section>

        <section
          aria-labelledby="server-form-heading"
          class="flex flex-col gap-3"
          data-testid="form"
        >
          <div class="flex flex-wrap items-center gap-2">
            <h2
              id="server-form-heading"
              class="text-base font-semibold text-highlighted"
            >
              {{ editing === undefined ? 'Add a server' : `Change how ${editing} is started or reached` }}
            </h2>
            <UButton
              v-if="editing !== undefined"
              label="Add one instead"
              icon="i-lucide-plus"
              color="neutral"
              variant="ghost"
              size="xs"
              :to="serversLink()"
              data-testid="add-instead"
            />
          </div>
          <p
            v-if="editing !== undefined && !changed"
            class="text-sm text-muted"
            data-testid="no-server"
          >
            The library holds no server called <code class="font-mono">{{ editing }}</code> whose definition reads.
          </p>
          <p
            v-else-if="changed && !changed.given"
            class="text-sm text-muted"
            data-testid="unformed"
          >
            Its definition holds what the form cannot say, so it is changed as text, in its file.
          </p>
          <ServerForm
            v-else-if="changed"
            :key="`edit-${changed.name}`"
            :editing="changed.name"
            :start="changed.given ?? undefined"
            @done="done"
          />
          <ServerForm
            v-else
            key="add"
            @done="done"
          />
        </section>
      </SnapshotGate>
    </template>
  </UDashboardPanel>
</template>
