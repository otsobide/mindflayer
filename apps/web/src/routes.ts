import type { RouteLocationRaw } from 'vue-router'

/**
 * Where a project's page is: its entry, as the registry spells it, in the
 * query rather than the path.
 *
 * An entry is a route from the workspace, so it can hold `/` and `..`, and
 * be `.` or `..` alone, for a project at the workspace's root or above it.
 * A browser resolves dot segments in a path before the router sees them,
 * escaped or not, so `/projects/..` would open `/`; a query is never
 * resolved, and a reload brings back exactly the entry it named.
 */
export function projectLink(entry: string): RouteLocationRaw {
  return { path: '/project', query: { entry } }
}
