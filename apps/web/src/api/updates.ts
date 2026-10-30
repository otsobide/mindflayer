import { useQuery } from '@tanstack/vue-query'
import { useFreshness } from './live'
import { useTransport } from './transport'

/**
 * What updating every project the workspace links would do (core's
 * `update::Updates`): each installation an update says something of, how
 * it stands, what an update does with it, and why it leaves one it leaves,
 * as `flayer update --all` says without `--yes`.
 *
 * A query of its own beside the snapshot, asked for only by the page of
 * updates: telling what an update does reads where each installation came
 * from again, which the snapshot, read again on every change, does not.
 * Its key is `updates`, which `src/api/live.ts` follows: what an update
 * would do is of the disk as it stands, so a change there, or a command
 * run, may change it.
 */
export function useUpdates() {
  const transport = useTransport()
  return useQuery({
    queryKey: ['updates'],
    queryFn: () => transport.query({ type: 'updates' }),
    staleTime: useFreshness()
  })
}
