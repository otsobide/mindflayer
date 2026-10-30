import { computed, toValue, type MaybeRefOrGetter } from 'vue'
import { keepPreviousData, useQuery } from '@tanstack/vue-query'
import type { Cell } from './generated/Cell'
import { useFreshness } from './live'
import { useTransport, type Task } from './transport'

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
 * The lines `task` is, as the server previews them (`POST
 * /api/commands/preview`), for the matrix to show what applying its plan
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
