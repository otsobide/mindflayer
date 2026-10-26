import { QueryClient } from '@tanstack/vue-query'
import { TransportError } from './api/transport'

/**
 * Whether a failed query is worth asking again: not when the server refused
 * it, which it will do again, but when it could not be reached or failed
 * itself, and then twice at most.
 */
export function retry(failures: number, error: unknown): boolean {
  const refused = error instanceof TransportError && error.status >= 400 && error.status < 500
  return !refused && failures < 2
}

/**
 * Where everything the server answers is kept: Vue Query knows when an
 * answer is stale and asks again, which a copy kept in a store would not.
 */
export function createQueryClient(): QueryClient {
  return new QueryClient({ defaultOptions: { queries: { retry } } })
}
