import { computed, toValue, type MaybeRefOrGetter } from 'vue'
import { useQuery } from '@tanstack/vue-query'
import type { Holder } from './generated/Holder'
import { useFreshness } from './live'
import { useTransport } from './transport'

/** An artifact as a read of it names it: what holds it, and its reference. */
export interface Named {
  holder: Holder
  /** As a command line takes it, `kind:name`, so one name never means two. */
  reference: string
}

/**
 * Every file and folder of the artifact `named` names, as core lists them
 * (`contents::Files`): asked once there is an artifact to ask about, and
 * again when a change reaches its holder, as the snapshot is.
 */
export function useFiles(named: MaybeRefOrGetter<Named | undefined>) {
  const transport = useTransport()
  return useQuery({
    queryKey: computed(() => ['files', toValue(named)]),
    queryFn: () => transport.query({ type: 'files', ...toValue(named)! }),
    enabled: computed(() => toValue(named) !== undefined),
    staleTime: useFreshness()
  })
}

/**
 * One file of it, by the route its listing gives, as text inside JSON
 * (`contents::Contents`): asked once there is a file to ask for.
 */
export function useContents(named: MaybeRefOrGetter<Named | undefined>, file: MaybeRefOrGetter<string | undefined>) {
  const transport = useTransport()
  return useQuery({
    queryKey: computed(() => ['contents', toValue(named), toValue(file)]),
    queryFn: () => transport.query({ type: 'contents', ...toValue(named)!, file: toValue(file)! }),
    enabled: computed(() => toValue(named) !== undefined && toValue(file) !== undefined),
    staleTime: useFreshness()
  })
}
