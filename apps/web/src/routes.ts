import type { RouteLocationRaw } from 'vue-router'
import type { Kind } from '@/api/generated/Kind'

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

/** The library's page of one kind. */
export function libraryLink(kind: Kind): RouteLocationRaw {
  return { path: '/library', query: { kind } }
}

/**
 * Where one of the library's own is shown: its kind and its name in the
 * query, for the reason a project's entry is. A rule's name is a route,
 * `git/commits`, which a path would read as two segments, and a name
 * gathered from somewhere else can hold anything.
 */
export function artifactLink(kind: Kind, name: string): RouteLocationRaw {
  return { path: '/artifact', query: { kind, name } }
}

/**
 * Where one of the library's own is edited: the editor of the file it is
 * read from, named as its page names it.
 */
export function editLink(kind: Kind, name: string): RouteLocationRaw {
  return { path: '/edit', query: { kind, name } }
}

/** Where one of a kind is added to the library: from a template, or from nothing. */
export function newLink(kind: Kind): RouteLocationRaw {
  return { path: '/new', query: { kind } }
}
