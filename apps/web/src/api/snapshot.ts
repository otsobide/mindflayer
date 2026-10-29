import { useQuery } from '@tanstack/vue-query'
import { useTransport } from './transport'

/** Where Vue Query keeps the snapshot, which every view reads. */
export const SNAPSHOT_KEY = ['snapshot'] as const

/**
 * The workspace in one read (core's `view::Snapshot`), as the views draw it.
 *
 * One query every view shares, rather than one of its own each, so moving
 * from the overview to a project draws at once from what is cached and asks
 * again only when the answer is stale. Everything a view shows is in it, its
 * counts and its order included: a view lays it out and filters it, and
 * works out nothing a query could answer.
 */
export function useSnapshot() {
  const transport = useTransport()
  return useQuery({
    queryKey: SNAPSHOT_KEY,
    queryFn: () => transport.query({ type: 'snapshot' })
  })
}
