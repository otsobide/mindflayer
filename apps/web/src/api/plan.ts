import { computed, toValue, type MaybeRefOrGetter } from 'vue'
import { keepPreviousData, useQuery, useQueryClient } from '@tanstack/vue-query'
import type { Asking } from './generated/Asking'
import type { Cell } from './generated/Cell'
import type { Planned } from './generated/Planned'
import { useFreshness } from './live'
import { useTransport, type Task } from './transport'

/**
 * The batch a plan's tasks are, sent back as they came, as the install
 * screen applies its marks; none when every box is held back and nothing
 * runs. What applying a plan sends, from the matrix or anywhere else that
 * plans a box, so the one batch is spelled once.
 *
 * Each install the plan says waits for a yes (`asks`) is sent as it came,
 * its line installing nothing, unless it is among `confirmed`, those
 * somebody said yes to once their lines were shown: then the task the
 * plan handed beside it carrying the yes goes in its place. The panel sets
 * no yes on a task of its own; one built anywhere else carries none.
 */
export function batchOf(plan: Planned, confirmed: Asking[] = []): Task | undefined {
  const changes = plan.tasks.map((task, at) => {
    const asking = plan.asks.find(each => each.task === at)
    return asking && confirmed.some(each => JSON.stringify(each.confirmed) === JSON.stringify(asking.confirmed))
      ? asking.confirmed
      : task
  })
  return changes.length > 0 ? { type: 'apply_install', changes } : undefined
}

/**
 * What the boxes somebody ticked and unticked amount to (the backend's
 * `Planned`): core's plan of them, each change as the task that runs it,
 * beside what clashes and what is locked. Asked once there is a box to
 * plan, and again whenever the boxes change.
 *
 * A read, and the one place the rules of a batch are said: the matrix
 * draws what it is answered with and sends the tasks back as they came,
 * never a task of its own. Its key starts with `plan`, which
 * `src/api/live.ts` follows: a plan is of the disk as it stands, so a
 * change there, or a command run, may change what the same boxes amount
 * to. While the next plan is asked for, the last one stays drawn
 * (`isPlaceholderData`), and nothing is applied from it.
 */
export function usePlan(selection: MaybeRefOrGetter<Cell[]>) {
  const transport = useTransport()
  return useQuery({
    queryKey: computed(() => ['plan', toValue(selection)]),
    queryFn: () => transport.query({ type: 'plan', selection: toValue(selection) }),
    enabled: computed(() => toValue(selection).length > 0),
    placeholderData: keepPreviousData,
    staleTime: useFreshness()
  })
}

/**
 * Ask for the plan of `selection` once, when somebody chooses it, rather
 * than whenever marks change: for what plans one box at a time, the
 * command palette's install. The matrix's query, under its key, so what it
 * is answered with lives where the matrix's does; asked again whatever is
 * kept, since a plan is of the disk as it stands.
 */
export function usePlanOnce() {
  const transport = useTransport()
  const client = useQueryClient()
  return (selection: Cell[]) => client.fetchQuery({
    queryKey: ['plan', selection],
    queryFn: () => transport.query({ type: 'plan', selection }),
    staleTime: 0
  })
}

/**
 * The lines `task` is, as the server previews them (`POST
 * /api/v1/tasks/preview`), for the matrix to show what applying its plan
 * runs before anybody asks for it: the same preview the runner asks for
 * once Apply is pressed, so what was shown is what the dialog shows. A read
 * too, which runs nothing; asked once there is a task.
 */
export function usePreview(task: MaybeRefOrGetter<Task | undefined>) {
  const transport = useTransport()
  return useQuery({
    queryKey: computed(() => ['preview', toValue(task)]),
    queryFn: () => transport.send(toValue(task)!, 'preview'),
    enabled: computed(() => toValue(task) !== undefined),
    staleTime: useFreshness()
  })
}
