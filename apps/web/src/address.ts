import { useRoute, useRouter, type LocationQueryValue } from 'vue-router'

/*
 * What a page is filtered, sorted or pointed at by lives in the address's
 * query, so a link to it, or a reload, opens it the same way.
 */

/** The value a query key holds, its first when it is given twice. */
export function first(value: LocationQueryValue | LocationQueryValue[] | undefined): string | undefined {
  const one = Array.isArray(value) ? value[0] : value
  return one ?? undefined
}

/**
 * A function putting `changes` into the address's query, in place of the
 * address it is at, so the back button leaves the page rather than undoing
 * each keystroke of a filter. An empty value takes its key out.
 */
export function useQueryChange() {
  const route = useRoute()
  const router = useRouter()
  return (changes: Record<string, string | undefined>) => {
    const query = { ...route.query, ...changes }
    for (const [key, value] of Object.entries(query)) {
      if (value === undefined || value === '') {
        delete query[key]
      }
    }
    void router.replace({ query })
  }
}
