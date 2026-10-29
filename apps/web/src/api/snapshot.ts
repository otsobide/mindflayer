import { useQuery } from '@tanstack/vue-query'
import { useTransport } from './transport'

/** Where Vue Query keeps the snapshot, which every view reads. */
export const SNAPSHOT_KEY = ['snapshot'] as const

/**
 * The workspace in one read (core's `view::Snapshot`), as the views draw it.
 *
 * One query every view shares, rather than one of its own each, so moving
 * from the overview to a project draws at once from what is cached, and asks
 * again behind it: an answer is stale as soon as it comes, since nothing
 * says when the workspace changed until #74's events will. Everything a
 * view shows is in it, its counts and its order included: a view lays it
 * out and filters it, and works out nothing a query could answer.
 */
export function useSnapshot() {
  const transport = useTransport()
  return useQuery({
    queryKey: SNAPSHOT_KEY,
    queryFn: () => transport.query({ type: 'snapshot' })
  })
}
