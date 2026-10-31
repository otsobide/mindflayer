import type { MaybeRef } from 'vue'
import { useQuery } from '@tanstack/vue-query'
import { useTransport } from './transport'

/**
 * Where Vue Query keeps what the header may switch to, which
 * `src/api/live.ts` marks stale with the rest.
 */
export const WORKSPACES_KEY = ['workspaces'] as const

/**
 * Every workspace the panel may switch to (the backend's `Query::Workspaces`,
 * core's `known::switchable`): those this machine knows and the one the
 * panel was launched on, each with whether it is in use, the launch's, the
 * rule it is chosen by once switched to, and whether a switch to it is
 * offered, all as core says them.
 *
 * Asked only while `enabled`, the switcher open, and again each time it
 * opens: a list beside the snapshot rather than in it, since which one the
 * panel was launched on is the launch's, and read from the markers of
 * workspaces nothing watches, so a name changed there since shows the next
 * time it opens.
 */
export function useWorkspaces(enabled: MaybeRef<boolean>) {
  const transport = useTransport()
  return useQuery({
    queryKey: WORKSPACES_KEY,
    queryFn: () => transport.query({ type: 'workspaces' }),
    enabled,
    staleTime: 0
  })
}
