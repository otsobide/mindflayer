import type { Held } from '@/api/generated/Held'
import type { Project } from '@/api/generated/Project'
import type { Snapshot } from '@/api/generated/Snapshot'

/** A project holding one of the library's own, and its copy there. */
export interface Holding {
  project: Project
  copy: Held
}

/**
 * Every project holding `held`, one of the library's own, with its copy
 * there, in the registry's order: each copy of that kind and name whose
 * origin is the one the library's own go by, as the snapshot says it
 * (`library.origin`). So a project's own of the same name, or one installed
 * from the shelf, is never taken for it. A filter of what the snapshot
 * lists, comparing words core wrote, and no rule of the panel's.
 */
export function holding(snapshot: Snapshot, held: Pick<Held, 'kind' | 'name'>): Holding[] {
  return snapshot.projects.flatMap(project =>
    project.artifacts
      .filter(copy => copy.kind === held.kind && copy.name === held.name && copy.origin === snapshot.library.origin)
      .map(copy => ({ project, copy }))
  )
}
