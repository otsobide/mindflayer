import { computed, toValue, type MaybeRefOrGetter } from 'vue'
import { keepPreviousData, useQuery } from '@tanstack/vue-query'
import type { Draft } from './generated/Draft'
import type { Holder } from './generated/Holder'
import { useFreshness } from './live'
import { useTransport } from './transport'

/**
 * What core says of `draft`, an artifact about to be written in `holder`
 * (the backend's `Query::Validate`, core's `draft::Judged`): why the
 * command writing it would write nothing, by its code and in its words,
 * and what `validate` would say of it once written, in its own. Asked once
 * there is a draft, and again whenever it changes, which the page that asks
 * holds back while somebody is still typing.
 *
 * A read, and the only judge of what is typed: the panel spells no rule of
 * a name, a description or front matter, and shows what it is answered
 * with. Its key starts with `validate`, which `src/api/live.ts` follows,
 * with its holder beside it: a name free a moment ago may be taken since,
 * so a change there, or a command run, asks again. While the next answer
 * is asked for, the last one stays drawn (`isPlaceholderData`).
 */
export function useJudged(holder: MaybeRefOrGetter<Holder>, draft: MaybeRefOrGetter<Draft | undefined>) {
  const transport = useTransport()
  return useQuery({
    queryKey: computed(() => ['validate', { holder: toValue(holder), draft: toValue(draft) }]),
    queryFn: () => transport.query({ type: 'validate', holder: toValue(holder), draft: toValue(draft)! }),
    enabled: computed(() => toValue(draft) !== undefined),
    placeholderData: keepPreviousData,
    staleTime: useFreshness()
  })
}
