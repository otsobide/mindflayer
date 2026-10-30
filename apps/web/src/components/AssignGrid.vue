<script lang="ts">
import type { State } from '@/api/generated/State'

/** One project or one offer along an edge of the matrix. */
export interface Line {
  /** What tells it from the others along its edge. */
  key: string
  /** What it is called where it is drawn. */
  label: string
  /** What else is said of it: an offer's kind and origin, a project's entry. */
  detail: string
  /** The group it is in along the top, an offer's origin, drawn above it. */
  group?: string
}

/** A row of the matrix: one of its lines, or the heading of the group below it. */
export type Row
  = | { type: 'group', key: string, label: string }
    | { type: 'line', key: string, line: number }

/**
 * How one box is drawn. Every word of it is the snapshot's or the plan's,
 * put together by the page: the grid only draws it.
 */
export interface Shown {
  /** What the box is and how it stands, said for whoever cannot see it. */
  label: string
  /** Whether no box of its kind can be asked of its project, as core says. */
  closed: boolean
  /** What it shows: ticked as the snapshot ticks it, or as it was marked. */
  checked: boolean
  /** Whether it was marked, and so shows other than the snapshot ticks it. */
  changed: boolean
  /** How the name stands in the project, where it holds something of it. */
  state: State | null
  /** Whether the plan holds it back: locked, or clashing with another box. */
  held: 'locked' | 'conflict' | null
}

// The sizes every row and every column has, in pixels, which the classes
// below spell too: `h-9` a row, `w-10` a box's column, `w-72` the column
// naming the rows.

/** How tall a row is. */
const ROW = 36
/** How wide a box's column is. */
const CELL = 40
/** How wide the column naming each row is. */
const HEAD = 288
/** How tall the band naming the groups along the top is. */
const BAND = 20
/** How tall the labels along the top are, written up the page. */
const LABELS = 128
</script>

<script setup lang="ts">
import { computed, nextTick, ref, watch } from 'vue'
import { observeElementRect, useVirtualizer, type Virtualizer } from '@tanstack/vue-virtual'
import { stateLook } from './StateBadge.vue'

const props = defineProps<{
  /** The grid's name, for whoever cannot see it. */
  label: string
  /** Every state a box can stand in, as the snapshot lists them, whose icons it draws. */
  states: State[]
  /** What the column of row names is headed by. */
  corner: string
  rows: Row[]
  /** The lines the rows name, by their place. */
  lines: Line[]
  columns: Line[]
  /** How the box of a line and a column is drawn, each by its place. */
  cell: (line: number, column: number) => Shown
}>()

const emit = defineEmits<{
  /** The box of a line and a column was pressed. */
  toggle: [line: number, column: number]
}>()

const scroller = ref<HTMLElement | null>(null)

/**
 * The scroller's size, or the window's while nothing has laid it out: where
 * nothing lays a page out, as in a test, the grid then draws a screen's
 * worth, as a browser would, rather than the few rows a box of no height
 * holds.
 */
function measured(instance: Virtualizer<HTMLElement, Element>, report: (rect: { width: number, height: number }) => void) {
  return observeElementRect(instance, rect => report({
    width: rect.width || window.innerWidth,
    height: rect.height || window.innerHeight
  }))
}

const grouped = computed(() => props.columns.some(column => column.group !== undefined))
/** How tall the heading along the top is: the labels, and the groups above them when there are any. */
const top = computed(() => LABELS + (grouped.value ? BAND : 0))

// Rows and columns both drawn only where they show, each laid out from the
// one size every one of them has, so nothing is measured: a matrix of
// hundreds of offers by hundreds of projects draws a screen's worth of
// boxes. Each starts past the heading it scrolls under.
const rowsVirtualizer = useVirtualizer(computed(() => ({
  count: props.rows.length,
  getScrollElement: () => scroller.value,
  estimateSize: () => ROW,
  overscan: 3,
  paddingStart: top.value,
  scrollPaddingStart: top.value,
  observeElementRect: measured
})))
const columnsVirtualizer = useVirtualizer(computed(() => ({
  horizontal: true,
  count: props.columns.length,
  getScrollElement: () => scroller.value,
  estimateSize: () => CELL,
  overscan: 2,
  paddingStart: HEAD,
  scrollPaddingStart: HEAD,
  observeElementRect: measured
})))

const shownRows = computed(() => rowsVirtualizer.value.getVirtualItems())
const shownColumns = computed(() => columnsVirtualizer.value.getVirtualItems())
const width = computed(() => columnsVirtualizer.value.getTotalSize())
const height = computed(() => rowsVirtualizer.value.getTotalSize())

/** The groups along the top over the columns drawn, each from where its first starts to where its last ends. */
const bands = computed(() => {
  const found: { key: string, label: string, start: number, size: number }[] = []
  for (const column of shownColumns.value) {
    const group = props.columns[column.index]!.group ?? ''
    const last = found.at(-1)
    if (last && last.label === group) {
      last.size = column.end - last.start
    } else {
      found.push({ key: `${group}:${column.index}`, label: group, start: column.start, size: column.size })
    }
  }
  return found
})

/** A box as it is drawn: where it is, how it looks, and what it shows. */
interface Drawn {
  key: string
  column: number
  start: number
  box: Shown
  look: string
  /** The icon in its corner: the plan's when it holds the box back, else its state's. */
  badge: { icon: string, tint: string } | undefined
}

/**
 * Every row drawn, with every box of it drawn, worked out once a change of
 * what is drawn rather than once each place the template reads one: a
 * screen's worth of boxes is hundreds of them.
 */
const drawn = computed(() => shownRows.value.map((shown) => {
  const row = props.rows[shown.index]!
  const line = row.type === 'line' ? props.lines[row.line]! : undefined
  const boxes: Drawn[] = row.type === 'line'
    ? shownColumns.value.map((column) => {
        const box = props.cell(row.line, column.index)
        return { key: props.columns[column.index]!.key, column: column.index, start: column.start, box, look: look(box), badge: badge(box) }
      })
    : []
  return { index: shown.index, start: shown.start, row, line, boxes }
}))

// The keyboard moves one box at a time, as a grid's does, and only one box
// is ever in the page's tab order: the one last moved to, or, when that one
// is scrolled away, the first drawn. Space or Enter presses it, as a
// button's.

/** The box last moved to or pressed, by the place of its row and its column. */
const active = ref({ row: 0, column: 0 })
/** Whether the box last moved to is to have the focus once it is drawn. */
let focusing = false

const lineRows = computed(() => props.rows.flatMap((row, index) => (row.type === 'line' ? [index] : [])))

const tabbable = computed(() => {
  const rows = shownRows.value.filter(row => props.rows[row.index]!.type === 'line')
  const columns = shownColumns.value
  const { row, column } = active.value
  if (rows.some(shown => shown.index === row) && columns.some(shown => shown.index === column)) {
    return active.value
  }
  return rows[0] && columns[0] ? { row: rows[0].index, column: columns[0].index } : undefined
})

function focusActive() {
  const { row, column } = active.value
  const box = scroller.value?.querySelector<HTMLElement>(`[data-cell="${row}:${column}"]`)
  if (box) {
    focusing = false
    box.focus()
  }
}

// A box scrolled to is drawn once the virtualizers catch up with the scroll.
watch([shownRows, shownColumns], async () => {
  if (focusing) {
    await nextTick()
    focusActive()
  }
})

// What the filters leave may hold fewer rows and columns than before.
watch(() => [lineRows.value, props.columns.length] as const, ([rows, columns]) => {
  const { row, column } = active.value
  active.value = {
    row: rows.includes(row) ? row : rows[0] ?? 0,
    column: Math.min(column, Math.max(columns - 1, 0))
  }
})

async function go(row: number, column: number) {
  active.value = { row, column }
  rowsVirtualizer.value.scrollToIndex(row, { align: 'auto' })
  columnsVirtualizer.value.scrollToIndex(column, { align: 'auto' })
  focusing = true
  await nextTick()
  focusActive()
}

/** How many rows a page up or down moves by: about a screen's worth. */
const PAGE = 10

/** The row and the column of the box `target` is, or is in, read off it. */
function placeOf(target: EventTarget | null): [number, number] | undefined {
  const box = target instanceof Element ? target.closest<HTMLElement>('[data-cell]') : null
  const place = box?.dataset.cell?.split(':').map(Number)
  return place?.length === 2 ? place as [number, number] : undefined
}

// One listener for every box, rather than a few on each: a screen's worth
// of boxes is hundreds of them.

function key(event: KeyboardEvent) {
  const place = placeOf(event.target)
  if (!place) {
    return
  }
  const [row, column] = place
  const rows = lineRows.value
  const here = rows.indexOf(row)
  const last = props.columns.length - 1
  const moves: Record<string, () => [number, number]> = {
    ArrowUp: () => [rows[Math.max(here - 1, 0)]!, column],
    ArrowDown: () => [rows[Math.min(here + 1, rows.length - 1)]!, column],
    ArrowLeft: () => [row, Math.max(column - 1, 0)],
    ArrowRight: () => [row, Math.min(column + 1, last)],
    PageUp: () => [rows[Math.max(here - PAGE, 0)]!, column],
    PageDown: () => [rows[Math.min(here + PAGE, rows.length - 1)]!, column],
    Home: () => (event.ctrlKey ? [rows[0]!, 0] : [row, 0]),
    End: () => (event.ctrlKey ? [rows.at(-1)!, last] : [row, last])
  }
  const move = moves[event.key]
  if (!move) {
    return
  }
  event.preventDefault()
  void go(...move())
}

function pressed(event: MouseEvent) {
  const place = placeOf(event.target)
  if (!place) {
    return
  }
  const [row, column] = place
  active.value = { row, column }
  const line = props.rows[row]!
  const box = event.target instanceof Element ? event.target.closest('[data-cell]') : null
  if (line.type === 'line' && box?.getAttribute('aria-disabled') !== 'true') {
    emit('toggle', line.line, column)
  }
}

function focused(event: FocusEvent) {
  const place = placeOf(event.target)
  if (place) {
    active.value = { row: place[0], column: place[1] }
  }
}

/** A box's look, which says nothing by its colour alone: what it shows has a shape or an icon too. */
function look(box: Shown): string {
  const what = box.closed
    ? 'cursor-not-allowed border-transparent text-dimmed'
    : box.checked
      ? 'border-primary bg-primary text-inverted'
      : 'border-accented bg-default'
  return box.changed ? `${what} outline-2 outline-dashed outline-primary` : what
}

/** The icons a box is drawn with beside its state's: ticked, held back, clashing. */
const CHECK = 'i-lucide-check'
const LOCK = 'i-lucide-lock'
const CLASH = 'i-lucide-triangle-alert'

/** Every icon a box may show, each drawn once. */
const icons = computed(() => [...new Set([CHECK, LOCK, CLASH, stateLook(null).icon, ...props.states.map(state => stateLook(state).icon)])])

/** The id the icon `name` is drawn under, once for every box. */
function sprite(name: string): string {
  return `matrix-${name}`
}

/**
 * The text colour of each of a state's colours, spelled whole so the
 * stylesheet holds every one: a class put together from a word would be
 * one no build finds.
 */
const TINTS: Record<string, string> = {
  primary: 'text-primary',
  secondary: 'text-secondary',
  success: 'text-success',
  info: 'text-info',
  warning: 'text-warning',
  error: 'text-error',
  neutral: 'text-muted'
}

function badge(box: Shown): Drawn['badge'] {
  if (box.held) {
    return { icon: box.held === 'locked' ? LOCK : CLASH, tint: 'text-error' }
  }
  if (box.state) {
    const { icon, color } = stateLook(box.state)
    return { icon, tint: TINTS[String(color ?? 'neutral')] ?? 'text-muted' }
  }
  return undefined
}
</script>

<template>
  <!--
    A grid of boxes, each a checkbox in a cell, drawn a screen at a time:
    `aria-rowcount` and `aria-colcount` say how many there are, and each row
    and cell drawn says where it is among them.
  -->
  <div
    ref="scroller"
    class="relative max-h-[min(70vh,48rem)] shrink-0 overflow-auto rounded-lg border border-default"
    data-testid="matrix"
    @keydown="key"
    @click="pressed"
    @focusin="focused"
  >
    <!--
      Each icon a box shows, drawn once from the bundle, as every icon is,
      and shown in each box that needs it by reference: a component for an
      icon in every box of a screen's worth would be most of the time it
      takes to draw one.
    -->
    <svg
      class="pointer-events-none absolute size-0 overflow-hidden"
      aria-hidden="true"
      focusable="false"
      data-testid="matrix-icons"
    >
      <UIcon
        v-for="icon in icons"
        :id="sprite(icon)"
        :key="icon"
        :name="icon"
      />
    </svg>
    <div
      role="grid"
      :aria-label="label"
      :aria-rowcount="rows.length + 1"
      :aria-colcount="columns.length + 1"
      class="relative"
      :style="{ width: `${width}px`, height: `${height}px` }"
    >
      <div
        role="rowgroup"
        class="sticky top-0 z-20 border-b border-default bg-default"
        :style="{ width: `${width}px`, height: `${top}px` }"
      >
        <div
          role="row"
          aria-rowindex="1"
          class="relative flex h-full"
          data-testid="matrix-head"
        >
          <div
            role="columnheader"
            aria-colindex="1"
            class="sticky left-0 z-10 flex w-72 shrink-0 items-end border-r border-default bg-default px-3 pb-2 font-mono text-xs text-muted"
          >
            {{ corner }}
          </div>
          <div
            v-for="band in grouped ? bands : []"
            :key="band.key"
            aria-hidden="true"
            class="absolute left-0 top-0 h-5 truncate border-b border-l border-default px-1 font-mono text-[10px] leading-5 text-muted"
            :title="band.label"
            :style="{ transform: `translateX(${band.start}px)`, width: `${band.size}px` }"
            data-testid="matrix-band"
          >
            {{ band.label }}
          </div>
          <div
            v-for="column in shownColumns"
            :key="columns[column.index]!.key"
            role="columnheader"
            :aria-colindex="column.index + 2"
            class="absolute bottom-0 left-0 flex h-32 w-10 justify-center pb-2"
            :title="`${columns[column.index]!.label}: ${columns[column.index]!.detail}`"
            :style="{ transform: `translateX(${column.start}px)` }"
            data-testid="matrix-column"
          >
            <span
              class="rotate-180 truncate font-mono text-xs text-highlighted [writing-mode:vertical-rl]"
              data-testid="matrix-column-label"
            >{{ columns[column.index]!.label }}</span>
            <span class="sr-only">, {{ columns[column.index]!.detail }}</span>
          </div>
        </div>
      </div>

      <div role="rowgroup">
        <div
          v-for="row in drawn"
          :key="row.row.key"
          role="row"
          :aria-rowindex="row.index + 2"
          class="absolute left-0 top-0 flex h-9"
          :class="row.line ? 'hover:bg-elevated/25' : 'bg-elevated/50'"
          :style="{ transform: `translateY(${row.start}px)`, width: `${width}px` }"
          :data-testid="row.line ? 'matrix-row' : 'matrix-group'"
        >
          <div
            v-if="!row.line"
            role="rowheader"
            aria-colindex="1"
            class="sticky left-0 z-10 flex w-72 items-center gap-2 truncate px-3 font-mono text-xs font-medium text-highlighted"
          >
            <UIcon
              name="i-lucide-package-open"
              class="size-3.5 shrink-0 text-muted"
            />
            <span
              class="truncate"
              :title="row.row.type === 'group' ? row.row.label : ''"
              data-testid="matrix-group-label"
            >{{ row.row.type === 'group' ? row.row.label : '' }}</span>
          </div>
          <template v-else>
            <div
              role="rowheader"
              aria-colindex="1"
              class="sticky left-0 z-10 flex w-72 shrink-0 flex-col justify-center border-r border-default bg-default px-3"
            >
              <span
                class="truncate font-mono text-sm text-highlighted"
                data-testid="matrix-row-label"
              >{{ row.line.label }}</span>
              <span
                class="truncate text-xs text-muted"
                data-testid="matrix-row-detail"
              >{{ row.line.detail }}</span>
            </div>
            <div
              v-for="drawnBox in row.boxes"
              :key="drawnBox.key"
              role="gridcell"
              :aria-colindex="drawnBox.column + 2"
              class="absolute left-0 top-0 flex h-9 w-10 items-center justify-center"
              :style="{ transform: `translateX(${drawnBox.start}px)` }"
            >
              <button
                type="button"
                role="checkbox"
                :aria-checked="drawnBox.box.checked"
                :aria-disabled="drawnBox.box.closed || undefined"
                :aria-label="drawnBox.box.label"
                :title="drawnBox.box.label"
                :tabindex="tabbable?.row === row.index && tabbable.column === drawnBox.column ? 0 : -1"
                :data-cell="`${row.index}:${drawnBox.column}`"
                :data-state="drawnBox.box.state ?? undefined"
                :data-held="drawnBox.box.held ?? undefined"
                :data-changed="drawnBox.box.changed || undefined"
                class="relative flex size-6 items-center justify-center rounded-md border outline-offset-2 focus-visible:ring-2 focus-visible:ring-primary focus-visible:ring-offset-4 focus-visible:ring-offset-(--ui-bg)"
                :class="drawnBox.look"
                data-testid="box"
              >
                <!--
                  One drawing for whatever the box shows, in the box's own
                  pixels: the tick in the middle, and the badge of its state
                  or of the plan's hold on it over its corner, on a disc of
                  the page's colour, so it reads over the box's edge.
                -->
                <svg
                  v-if="(drawnBox.box.checked && !drawnBox.box.closed) || drawnBox.badge"
                  class="pointer-events-none absolute inset-0 size-6 overflow-visible"
                  viewBox="0 0 24 24"
                  aria-hidden="true"
                >
                  <use
                    v-if="drawnBox.box.checked && !drawnBox.box.closed"
                    :href="`#${sprite(CHECK)}`"
                    x="4"
                    y="4"
                    width="16"
                    height="16"
                  />
                  <g
                    v-if="drawnBox.badge"
                    :class="drawnBox.badge.tint"
                  >
                    <circle
                      cx="22"
                      cy="2"
                      r="8"
                      class="fill-(--ui-bg)"
                    />
                    <use
                      :href="`#${sprite(drawnBox.badge.icon)}`"
                      x="15.5"
                      y="-4.5"
                      width="13"
                      height="13"
                    />
                  </g>
                </svg>
                <span
                  v-else-if="drawnBox.box.closed"
                  aria-hidden="true"
                >&middot;</span>
              </button>
            </div>
          </template>
        </div>
      </div>
    </div>
  </div>
</template>
