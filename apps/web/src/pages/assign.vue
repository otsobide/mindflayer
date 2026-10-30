<script setup lang="ts">
import { computed } from 'vue'
import { useRoute } from 'vue-router'
import type { Cell } from '@/api/generated/Cell'
import type { HeldBox } from '@/api/generated/HeldBox'
import type { Offered } from '@/api/generated/Offered'
import type { Project } from '@/api/generated/Project'
import type { Said } from '@/api/generated/Said'
import type { Snapshot } from '@/api/generated/Snapshot'
import { usePlan, usePreview } from '@/api/plan'
import { useSnapshot } from '@/api/snapshot'
import { TransportError, type Task } from '@/api/transport'
import { first, useQueryChange } from '@/address'
import AppHeader from '@/components/AppHeader.vue'
import AssignGrid, { type Line, type Row, type Shown } from '@/components/AssignGrid.vue'
import CommandHint, { flayer, making } from '@/components/CommandHint.vue'
import SnapshotGate from '@/components/SnapshotGate.vue'
import { stateLook } from '@/components/StateBadge.vue'
import { useMarks } from '@/stores/marks'

const route = useRoute()
const change = useQueryChange()

// What the matrix is filtered by and which way round it is are the
// address's (src/address.ts): `?kind=skill&origin=workspace&agent=claude
// &q=deploy&by=project`. Kind, origin and text pick the offers, the agent
// the projects, whichever way round.
const text = computed({
  get: () => first(route.query.q) ?? '',
  set: (q: string) => change({ q })
})
const kind = computed(() => first(route.query.kind))
const origin = computed(() => first(route.query.origin))
const agent = computed(() => first(route.query.agent))
const byProject = computed(() => first(route.query.by) === 'project')

const { data } = useSnapshot()
const marks = useMarks()

/** What names a project to a command and a box: its entry. */
function named(project: Project): string {
  return project.entry ?? project.path
}

/** What a project is called: its name, or its entry when it opens no project. */
function title(project: Project): string {
  return project.name ?? named(project)
}

/**
 * What tells one box from another: its project, its artifact and the
 * offer's origin, apart by a character none of them holds.
 */
function boxKey(project: string, reference: string, origin: string): string {
  return `${project}\u0000${reference}\u0000${origin}`
}

/** An offer as a command line names it, `kind:name`, which a box sends. */
function reference(offered: Offered): string {
  return `${offered.kind}:${offered.name}`
}

/**
 * The snapshot's boxes, found by the box they are of: each one that is not
 * empty, and each kind of each project none can be asked of. Looked up, never
 * worked out: which offer a project holds is core's (`install::holding`).
 */
const found = computed(() => {
  const held = new Map<string, HeldBox>()
  const closed = new Map<string, Said>()
  for (const boxes of data.value?.boxes ?? []) {
    for (const box of boxes.held) {
      held.set(boxKey(boxes.entry, `${box.kind}:${box.name}`, box.origin), box)
    }
    for (const each of boxes.closed) {
      closed.set(`${boxes.entry}\u0000${each.kind}`, each.why)
    }
  }
  return { held, closed }
})

/** Whether the snapshot ticks the box a mark is of. */
function ticked(cell: Omit<Cell, 'wanted'>): boolean {
  return found.value.held.get(boxKey(cell.project, cell.reference, cell.origin))?.ticked ?? false
}

/**
 * Every mark that asks for other than the snapshot shows, in the order
 * marked: what is planned. One the snapshot has caught up with, since a
 * command ran elsewhere, asks for nothing, and is left out.
 */
const selection = computed<Cell[]>(() => marks.marks
  .filter(mark => mark.wanted !== ticked(mark))
  .map(mark => ({ project: mark.project, reference: mark.reference, origin: mark.origin, wanted: mark.wanted })))

const { data: planned, error: refused, isFetching: planning, isPlaceholderData } = usePlan(selection)
/** The plan of the boxes marked now, none while it is asked for or when nothing is marked. */
const plan = computed(() => (selection.value.length > 0 && !isPlaceholderData.value ? planned.value : undefined))

/** The batch the plan's tasks are, sent back as they came, as the install screen applies its marks. */
const batch = computed<Task | undefined>(() => (plan.value && plan.value.tasks.length > 0
  ? { type: 'apply_install', changes: plan.value.tasks }
  : undefined))
const { data: preview, error: unpreviewed } = usePreview(batch)

/** Each box the plan holds back, and why: locked by a code, or clashing with another. */
const heldBack = computed(() => {
  const held = new Map<string, { how: 'locked' | 'conflict', why: Said }>()
  for (const conflict of planned.value?.conflicts ?? []) {
    for (const cell of conflict.cells) {
      held.set(boxKey(cell.project, cell.reference, cell.origin), { how: 'conflict', why: conflict.why })
    }
  }
  for (const locked of planned.value?.locked ?? []) {
    held.set(boxKey(locked.cell.project, locked.cell.reference, locked.cell.origin), { how: 'locked', why: locked.why })
  }
  return held
})

/** The offers the filters keep, grouped by where they are offered from, each group where its first is. */
const groups = computed(() => {
  const needle = text.value.trim().toLowerCase()
  const kept = (data.value?.offers ?? [])
    .filter(offered => !kind.value || offered.kind === kind.value)
    .filter(offered => !origin.value || offered.origin === origin.value)
    .filter(offered => !needle || [offered.name, offered.summary, offered.origin].some(field => field?.toLowerCase().includes(needle)))
  const byOrigin = new Map<string, Offered[]>()
  for (const offered of kept) {
    byOrigin.set(offered.origin, [...(byOrigin.get(offered.origin) ?? []), offered])
  }
  return [...byOrigin.entries()].map(([origin, offers]) => ({ origin, offers }))
})
const offers = computed(() => groups.value.flatMap(group => group.offers))

/** The projects the filter keeps, in the registry's order. */
const projects = computed(() => (data.value?.projects ?? [])
  .filter(project => !agent.value || project.agents.some(each => each === agent.value)))

/** Every place an offer comes from, in the order the snapshot first offers from each, to filter by. */
const origins = computed(() => [...new Set((data.value?.offers ?? []).map(offered => offered.origin))])

function offerLine(offered: Offered): Line {
  return {
    key: JSON.stringify([offered.kind, offered.name, offered.origin]),
    label: offered.name,
    detail: `${offered.kind} from ${offered.origin}${offered.summary ? `: ${offered.summary}` : ''}`,
    group: offered.origin
  }
}

function projectLine(project: Project): Line {
  const agents = project.agents.length > 0 ? project.agents.join(', ') : 'no agent'
  return {
    key: named(project),
    label: title(project),
    detail: project.stale ? `${named(project)}: opens no project` : `${named(project)}, for ${agents}`
  }
}

// Turned round by project, a row is a project and a column an offer, the
// offers' groups drawn along the top; by offer, the other way, each group a
// heading above its offers.
const lines = computed(() => (byProject.value ? projects.value.map(projectLine) : offers.value.map(offerLine)))
const columns = computed(() => (byProject.value
  ? offers.value.map(offerLine)
  : projects.value.map(project => ({ ...projectLine(project), group: undefined }))))
const rows = computed<Row[]>(() => {
  if (byProject.value) {
    return projects.value.map((project, line) => ({ type: 'line', key: named(project), line }))
  }
  let line = 0
  return groups.value.flatMap(group => [
    { type: 'group' as const, key: `group:${group.origin}`, label: group.origin },
    ...group.offers.map(offered => ({ type: 'line' as const, key: offerLine(offered).key, line: line++ }))
  ])
})

/** The offer and the project of the box at a line and a column. */
function boxAt(line: number, column: number): { offered: Offered, project: Project } {
  return byProject.value
    ? { offered: offers.value[column]!, project: projects.value[line]! }
    : { offered: offers.value[line]!, project: projects.value[column]! }
}

/** How the box of `offered` in `project` is drawn: as the snapshot has it, as it was marked, as the plan holds it. */
function shown(offered: Offered, project: Project): Shown {
  const entry = named(project)
  const asked = { project: entry, reference: reference(offered), origin: offered.origin }
  const key = boxKey(entry, asked.reference, asked.origin)
  const closed = found.value.closed.get(`${entry}\u0000${offered.kind}`)
  const held = found.value.held.get(key)
  const mark = marks.find(asked)
  const isTicked = held?.ticked ?? false
  const checked = mark?.wanted ?? isTicked
  const changed = mark !== undefined && mark.wanted !== isTicked
  const back = changed ? heldBack.value.get(key) : undefined
  const said = [`${offered.kind} ${offered.name} from ${offered.origin} in ${title(project)}`]
  if (closed) {
    said.push(`none can go there, ${closed.code}: ${closed.message}`)
  }
  if (held) {
    said.push(held.state)
  }
  if (changed) {
    said.push(checked ? 'marked to be ticked' : 'marked to be unticked')
  }
  if (back) {
    said.push(`held back, ${back.why.code}: ${back.why.message}`)
  }
  return {
    label: said.join('; '),
    closed: closed !== undefined,
    checked,
    changed,
    state: held?.state ?? null,
    held: back?.how ?? null
  }
}

function cell(line: number, column: number): Shown {
  const { offered, project } = boxAt(line, column)
  return shown(offered, project)
}

/** Mark the box at a line and a column the other way round from what it shows. */
function toggle(line: number, column: number) {
  const { offered, project } = boxAt(line, column)
  const asked = { project: named(project), reference: reference(offered), origin: offered.origin }
  const now = marks.find(asked)?.wanted ?? ticked(asked)
  marks.mark({ ...asked, wanted: !now }, ticked(asked))
}

/** Why a plan or its preview was refused, with its code. */
function why(error: unknown): string | undefined {
  if (!error) {
    return undefined
  }
  return error instanceof TransportError && error.code ? `${error.code}: ${error.message}` : String(error)
}

const filtered = computed(() => Boolean(text.value || kind.value || origin.value || agent.value))

function clear() {
  change({ q: undefined, kind: undefined, origin: undefined, agent: undefined })
}

/** The commands a box stands for, for the shell. */
function commands(snapshot: Snapshot) {
  return [
    ...making(snapshot.workspace),
    { line: flayer(snapshot.workspace, 'install -p <PROJECT> --from <ORIGIN> <KIND>:<NAME>'), does: 'a box ticked' },
    { line: flayer(snapshot.workspace, 'uninstall -p <PROJECT> <KIND>:<NAME>'), does: 'a box unticked' }
  ]
}
</script>

<template>
  <UDashboardPanel
    id="assign"
    role="main"
    data-testid="assign"
  >
    <template #header>
      <AppHeader
        title="Assign"
        icon="i-lucide-grid-3x3"
      />
    </template>

    <template #body>
      <SnapshotGate v-slot="{ snapshot }">
        <div
          v-if="snapshot.offers.length === 0 || snapshot.projects.length === 0"
          class="flex flex-col items-start gap-3"
          data-testid="empty"
        >
          <p class="text-muted">
            <template v-if="snapshot.offers.length === 0">
              {{ snapshot.workspace.name }} offers nothing to install yet: add to its library, gather onto its shelf, or load another workspace.
            </template>
            <template v-else>
              {{ snapshot.workspace.name }} links no project to install into yet.
            </template>
          </p>
          <CommandHint
            :commands="snapshot.offers.length === 0
              ? [
                ...making(snapshot.workspace),
                { line: flayer(snapshot.workspace, 'add skill <NAME> <DESCRIPTION>'), does: 'one of its own' },
                { line: flayer(snapshot.workspace, 'gather git <URL>'), does: 'skills from a repository, onto the shelf' }
              ]
              : [...making(snapshot.workspace), { line: flayer(snapshot.workspace, 'link <PATH>'), does: 'a project, to install into' }]"
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
              placeholder="Filter offers by name, summary or origin"
              aria-label="Filter the offers by name, summary or origin"
              size="sm"
              class="w-full sm:w-72"
              data-testid="filter-text"
            />
            <UFieldGroup
              role="group"
              aria-label="Turn the matrix round"
              size="sm"
            >
              <UButton
                label="by offer"
                color="neutral"
                :variant="byProject ? 'outline' : 'subtle'"
                :icon="byProject ? undefined : 'i-lucide-check'"
                :aria-pressed="!byProject"
                class="font-mono"
                data-testid="by-offer"
                @click="change({ by: undefined })"
              />
              <UButton
                label="by project"
                color="neutral"
                :variant="byProject ? 'subtle' : 'outline'"
                :icon="byProject ? 'i-lucide-check' : undefined"
                :aria-pressed="byProject"
                class="font-mono"
                data-testid="by-project"
                @click="change({ by: 'project' })"
              />
            </UFieldGroup>
            <UFieldGroup
              role="group"
              aria-label="Show offers of one kind"
              size="sm"
            >
              <UButton
                label="every kind"
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
                :label="each"
                color="neutral"
                :variant="kind === each ? 'subtle' : 'outline'"
                :icon="kind === each ? 'i-lucide-check' : undefined"
                :aria-pressed="kind === each"
                class="font-mono"
                :data-testid="`filter-kind-${each}`"
                @click="change({ kind: each })"
              />
            </UFieldGroup>
            <UFieldGroup
              role="group"
              aria-label="Show projects one agent works in"
              size="sm"
            >
              <UButton
                label="every agent"
                color="neutral"
                :variant="agent ? 'outline' : 'subtle'"
                :icon="agent ? undefined : 'i-lucide-check'"
                :aria-pressed="!agent"
                class="font-mono"
                data-testid="filter-agent-all"
                @click="change({ agent: undefined })"
              />
              <UButton
                v-for="each in snapshot.agents"
                :key="each"
                :label="each"
                color="neutral"
                :variant="agent === each ? 'subtle' : 'outline'"
                :icon="agent === each ? 'i-lucide-check' : undefined"
                :aria-pressed="agent === each"
                class="font-mono"
                :data-testid="`filter-agent-${each}`"
                @click="change({ agent: each })"
              />
            </UFieldGroup>
          </div>

          <div
            class="flex flex-wrap items-center gap-2"
            role="group"
            aria-label="Show offers from one place"
            data-testid="filter-origin"
          >
            <UButton
              label="every origin"
              color="neutral"
              size="sm"
              :variant="origin ? 'outline' : 'subtle'"
              :icon="origin ? undefined : 'i-lucide-check'"
              :aria-pressed="!origin"
              class="font-mono"
              data-testid="filter-origin-all"
              @click="change({ origin: undefined })"
            />
            <UButton
              v-for="each in origins"
              :key="each"
              :label="each"
              color="neutral"
              size="sm"
              :variant="origin === each ? 'subtle' : 'outline'"
              :icon="origin === each ? 'i-lucide-check' : undefined"
              :aria-pressed="origin === each"
              class="max-w-full truncate font-mono"
              data-testid="filter-origin-each"
              @click="change({ origin: each })"
            />
          </div>

          <section
            v-if="marks.marks.length > 0"
            class="flex flex-col gap-3 rounded-lg border border-default p-3"
            aria-labelledby="plan-title"
            data-testid="plan"
          >
            <div class="flex flex-wrap items-center gap-2">
              <h2
                id="plan-title"
                class="text-sm font-semibold text-highlighted"
              >
                Planned
              </h2>
              <UBadge
                :label="`${selection.length} marked`"
                color="neutral"
                variant="soft"
                size="sm"
                class="font-mono tabular-nums"
                data-testid="marked"
              />
              <span
                v-if="planning"
                class="text-xs text-muted"
                data-testid="planning"
              >asking for the plan</span>
              <div class="ms-auto flex gap-2">
                <UButton
                  label="Clear the marks"
                  icon="i-lucide-eraser"
                  color="neutral"
                  variant="outline"
                  size="sm"
                  data-testid="clear-marks"
                  @click="marks.clear()"
                />
              </div>
            </div>

            <p
              v-if="why(refused)"
              class="text-sm text-error"
              data-testid="plan-refused"
            >
              The plan was refused, {{ why(refused) }}
            </p>

            <template v-if="plan">
              <div v-if="plan.tasks.length > 0">
                <h3 class="mb-1 text-xs font-medium text-muted">
                  What runs
                </h3>
                <ul
                  v-if="preview"
                  class="flex flex-col gap-1 rounded-md bg-muted px-3 py-2 font-mono text-xs"
                  data-testid="lines"
                >
                  <li
                    v-for="line in preview.lines"
                    :key="line"
                    class="break-all"
                    data-testid="line"
                  >
                    $ {{ line }}
                  </li>
                </ul>
                <p
                  v-else-if="why(unpreviewed)"
                  class="text-sm text-error"
                  data-testid="preview-refused"
                >
                  {{ why(unpreviewed) }}
                </p>
              </div>
              <p
                v-else
                class="text-sm text-muted"
                data-testid="nothing-runs"
              >
                Nothing runs: every box marked is held back.
              </p>

              <div v-if="plan.conflicts.length > 0">
                <h3 class="mb-1 text-xs font-medium text-muted">
                  Asked both ways, so neither runs
                </h3>
                <ul
                  class="flex flex-col gap-1 text-sm"
                  data-testid="conflicts"
                >
                  <li
                    v-for="conflict in plan.conflicts"
                    :key="`${conflict.project}:${conflict.kind}:${conflict.name}`"
                    class="flex items-start gap-2"
                    data-testid="conflict"
                  >
                    <UIcon
                      name="i-lucide-triangle-alert"
                      class="mt-0.5 size-4 shrink-0 text-error"
                    />
                    <span>
                      <code class="font-mono text-xs">{{ conflict.project }} {{ conflict.kind }}:{{ conflict.name }}</code>,
                      <code class="font-mono text-xs">{{ conflict.why.code }}</code>: {{ conflict.why.message }}
                    </span>
                  </li>
                </ul>
              </div>

              <div v-if="plan.locked.length > 0">
                <h3 class="mb-1 text-xs font-medium text-muted">
                  Held back
                </h3>
                <ul
                  class="flex flex-col gap-1 text-sm"
                  data-testid="locks"
                >
                  <li
                    v-for="locked in plan.locked"
                    :key="JSON.stringify(locked.cell)"
                    class="flex items-start gap-2"
                    data-testid="lock"
                  >
                    <UIcon
                      name="i-lucide-lock"
                      class="mt-0.5 size-4 shrink-0 text-error"
                    />
                    <span>
                      <code class="font-mono text-xs">{{ locked.cell.project }} {{ locked.cell.reference }} from {{ locked.cell.origin }}</code>,
                      <code class="font-mono text-xs">{{ locked.why.code }}</code>: {{ locked.why.message }}
                    </span>
                  </li>
                </ul>
              </div>
            </template>
          </section>

          <AssignGrid
            v-if="rows.length > 0 && columns.length > 0"
            class="w-fit max-w-full"
            :label="byProject ? 'Every project against every offer' : 'Every offer against every project'"
            :corner="byProject ? 'project \\ offer' : 'offer \\ project'"
            :states="snapshot.states"
            :rows="rows"
            :lines="lines"
            :columns="columns"
            :cell="cell"
            @toggle="toggle"
          />

          <div
            v-else
            class="flex flex-col items-start gap-2"
            data-testid="no-match"
          >
            <p class="text-muted">
              Nothing matches these filters.
            </p>
            <UButton
              v-if="filtered"
              label="Clear the filters"
              icon="i-lucide-filter-x"
              color="neutral"
              variant="outline"
              size="sm"
              @click="clear"
            />
          </div>

          <ul
            class="flex flex-wrap gap-x-4 gap-y-1 text-xs text-muted"
            aria-label="What a box shows"
            data-testid="legend"
          >
            <li class="flex items-center gap-1">
              <UIcon
                name="i-lucide-check"
                class="size-3.5"
              />
              ticked: the project holds it from there
            </li>
            <li class="flex items-center gap-1">
              <span class="mx-1 inline-block size-3 rounded-sm outline-2 outline-dashed outline-primary" />
              marked, not applied yet
            </li>
            <li
              v-for="state in snapshot.states"
              :key="state"
              class="flex items-center gap-1"
            >
              <UIcon
                :name="stateLook(state).icon"
                class="size-3.5"
              />
              {{ state }}
            </li>
            <li class="flex items-center gap-1">
              <UIcon
                name="i-lucide-lock"
                class="size-3.5"
              />
              held back by the plan
            </li>
            <li class="flex items-center gap-1">
              <UIcon
                name="i-lucide-triangle-alert"
                class="size-3.5"
              />
              clashes with another box
            </li>
            <li class="flex items-center gap-1">
              <span aria-hidden="true">&middot;</span>
              nothing of its kind can go into the project
            </li>
          </ul>

          <p class="text-muted">
            Every offer against every project. A ticked box is one the project holds from there, as core says. Tick or untick boxes, and the plan above says what that runs, and what it holds back, before anything does.
          </p>
          <CommandHint :commands="commands(snapshot)" />
        </template>
      </SnapshotGate>
    </template>
  </UDashboardPanel>
</template>
