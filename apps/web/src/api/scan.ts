import { computed, toValue, type MaybeRefOrGetter } from 'vue'
import { useQuery, useQueryClient } from '@tanstack/vue-query'
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
 * the disk.
 *
 * Kept, so an answer outlives the address that asked for it, which is why
 * the page asks again itself rather than trusting what is kept: `forget`
 * drops what a scan of the folders about to be asked for was answered with
 * before somebody's Scan changes the address to them, or Vue Query would
 * draw an answer from before whatever was linked since and ask nothing,
 * and `stale` marks every answer kept once a run has changed what is
 * linked, the one on the page asked for again at once. A `gcTime` of 0 was
 * the other way, and would walk the disk again for a page only opened
 * again.
 */
export function useScan(asked: MaybeRefOrGetter<Asked | undefined>) {
  const transport = useTransport()
  const client = useQueryClient()
  const query = useQuery({
    queryKey: computed(() => ['scan', toValue(asked)]),
    queryFn: () => transport.query({ type: 'scan', ...toValue(asked)! }),
    enabled: computed(() => toValue(asked) !== undefined),
    staleTime: Infinity
  })
  return {
    ...query,
    forget: (wanted: Asked) => client.removeQueries({ queryKey: ['scan', wanted], exact: true }),
    stale: () => client.invalidateQueries({ queryKey: ['scan'] })
  }
}
