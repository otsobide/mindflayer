import { computed, toValue, type MaybeRefOrGetter } from 'vue'
import { useQuery } from '@tanstack/vue-query'
import { useTransport } from './transport'

/**
 * A scan as somebody asked for it: the folders to look under, each sent as
 * typed for the server to hold to the path policy, and how many levels
 * down, none for core's own depth.
 */
export interface Asked {
  roots: string[]
  depth: number | null
}

/**
 * What could be linked under the folders `asked` names, as `flayer scan
 * --suggest` finds it (the backend's `Scanned`): asked for once somebody
 * asks, and never again on its own.
 *
 * Its key starts with `scan`, which `src/api/live.ts` does not follow, so
 * no event the server streams asks for it again: a scan walks the disk far
 * past what the server watches, and one run on every change would be a
 * crawl each time anything moved. Never stale, for the same reason, so
 * neither a page opened again nor the window coming back into focus walks
 * the disk; a page asks again itself (`refetch`), as after a run that
 * changed what is linked.
 */
export function useScan(asked: MaybeRefOrGetter<Asked | undefined>) {
  const transport = useTransport()
  return useQuery({
    queryKey: computed(() => ['scan', toValue(asked)]),
    queryFn: () => transport.query({ type: 'scan', ...toValue(asked)! }),
    enabled: computed(() => toValue(asked) !== undefined),
    staleTime: Infinity
  })
}
