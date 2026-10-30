import type { MaybeRef } from 'vue'
import { useQuery } from '@tanstack/vue-query'
import { useTransport } from './transport'

/**
 * Where Vue Query keeps the transcript, which `src/api/live.ts` marks stale
 * once a run finishes, and on a resync.
 */
export const TRANSCRIPT_KEY = ['transcript'] as const

/**
 * Every task the session ran, oldest first, with what each produced: the
 * server's transcript (`GET /api/transcript`), runs from every tab and
 * script holding the token, not this panel's alone.
 *
 * Asked for only while `enabled`, the drawer open, so a page that never
 * opens it asks for nothing, and asked again each time it opens and each
 * time a run finishes while it is: stale at once, since it is a read of
 * the server's memory, nothing a watch on the disk would tell.
 */
export function useTranscript(enabled: MaybeRef<boolean>) {
  const transport = useTransport()
  return useQuery({
    queryKey: TRANSCRIPT_KEY,
    queryFn: () => transport.transcript(),
    enabled
  })
}
