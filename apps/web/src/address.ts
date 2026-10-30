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

/** Every value a query key holds, in the order given: none when it is not there. */
export function every(value: LocationQueryValue | LocationQueryValue[] | undefined): string[] {
  const all = Array.isArray(value) ? value : [value]
  return all.filter((one): one is string => typeof one === 'string')
}

/**
 * A function putting `changes` into the address's query, in place of the
 * address it is at, so the back button leaves the page rather than undoing
 * each keystroke of a filter. An empty value, or an empty list, takes its
 * key out; a list is the key given once for each of its values.
 */
export function useQueryChange() {
  const route = useRoute()
  const router = useRouter()
  return (changes: Record<string, string | string[] | undefined>) => {
    const query = { ...route.query, ...changes }
    for (const [key, value] of Object.entries(query)) {
      if (value === undefined || value === '' || (Array.isArray(value) && value.length === 0)) {
        delete query[key]
      }
    }
    void router.replace({ query })
  }
}
