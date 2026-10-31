import { computed, toValue, type MaybeRefOrGetter } from 'vue'
import { useQuery } from '@tanstack/vue-query'
import { useFreshness } from './live'
import { useTransport } from './transport'

/** Where Vue Query keeps the snapshot, which every view reads. */
export const SNAPSHOT_KEY = ['snapshot'] as const

/**
 * The workspace in one read (core's `view::Snapshot`), as the views draw it.
 *
 * One query every view shares, rather than one of its own each, so moving
 * from the overview to a project draws at once from what is cached. It is
 * asked for again when the server says the workspace changed (`listen`),
 * and only then while the server watches it; when it does not, an answer is
 * stale as soon as it comes, and each page asks again behind what it drew.
 * Everything a view shows is in it, its counts and its order included: a
 * view lays it out and filters it, and works out nothing a query could
 * answer.
 *
 * `enabled` false reads what is kept and asks for nothing, for what is
 * drawn over every page only now and then, the command palette, which
 * would otherwise ask for the snapshot on a page that asks for none.
 */
export function useSnapshot(enabled: MaybeRefOrGetter<boolean> = true) {
  const transport = useTransport()
  return useQuery({
    queryKey: SNAPSHOT_KEY,
    queryFn: () => transport.query({ type: 'snapshot' }),
    enabled: computed(() => toValue(enabled)),
    staleTime: useFreshness()
  })
}
