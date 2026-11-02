import { computed, toValue, type MaybeRefOrGetter } from 'vue'
import { useQuery } from '@tanstack/vue-query'
import { useTransport } from './transport'

/**
 * A folder the explorer opens: its path, sent as typed or as a listing
 * spelled it for the server to hold to the path policy, none for the
 * workspace's root; whether hidden folders are listed; and which time the
 * explorer was opened (`visit`), so each opening reads the folder again.
 */
export interface Opened {
  path: string | null
  hidden: boolean
  visit: number
}

/**
 * The folders directly inside the one `opened` names (the backend's
 * `Query::Folders`, core's `browse::Folders`): what each is, whether the
 * workspace links it, the trail up to it and the launch's home, asked for
 * only while `enabled`, the explorer open.
 *
 * Its key starts with `folders`, which `src/api/live.ts` does not follow:
 * a listing reads folders the server does not watch, anywhere on the disk,
 * and no event says anything of them. So each opening of the explorer is
 * a key of its own, by its `visit`, and reads every folder it shows again,
 * and what an opening read is dropped once nothing shows it
 * (`gcTime: 0`), as what the header may switch to is read again each time
 * its menu opens. Walking back to a folder within one opening shows what
 * was read of it and reads it again behind it (`staleTime: 0`).
 */
export function useFolders(opened: MaybeRefOrGetter<Opened>, enabled: MaybeRefOrGetter<boolean>) {
  const transport = useTransport()
  return useQuery({
    queryKey: computed(() => ['folders', toValue(opened)]),
    queryFn: () => {
      const { path, hidden } = toValue(opened)
      return transport.query({ type: 'folders', path, hidden })
    },
    enabled: computed(() => toValue(enabled)),
    staleTime: 0,
    gcTime: 0
  })
}
