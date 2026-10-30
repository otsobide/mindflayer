/*
 * The workspace the panel is served over, built as somebody would build it:
 * a command at a time, with the binaries under test, so what the panel draws
 * is what those commands made and `flayer list --json` says of it. Nothing is
 * written by hand but the folders `mind init` is run in, as `mkdir` would.
 */
import { mkdirSync } from 'node:fs'
import { join } from 'node:path'
import { succeed, type Places } from './environment.ts'

/** The workspace's own, each as `flayer add` takes it: kind, name, description. */
export const LIBRARY = [
  ['skill', 'review', 'Read a change before it lands'],
  ['skill', 'lint', 'Tidy the code before a commit'],
  ['rule', 'git/commits', 'Write commit messages that say why']
] as const

/** The projects it links, each by its entry. */
export const PROJECTS = ['api', 'web'] as const

/**
 * `workspace`, holding something of each thing the views compare with the
 * listing:
 *
 * - its library holds the skills `review` and `lint` and the rule
 *   `git/commits`;
 * - `api`, for Claude and Codex and local, authors the skill `deploy` and
 *   the rule `style/comments`, and received `review`;
 * - `web`, for Claude and OpenCode and shared, received `review` and `lint`.
 *
 * Neither project is a repository: a local one says, in a note, that nothing
 * keeps its copies out of git, which is no failure. No rule is installed,
 * since a rule's block in an `AGENTS.md` outside a work tree is a warning,
 * that Codex reads it only when started there.
 */
export function build(at: Places): void {
  succeed(at, 'flayer', at.scratch, ['init', at.workspace], 'none')
  for (const [kind, name, description] of LIBRARY) {
    succeed(at, 'flayer', at.workspace, ['add', kind, name, description])
  }

  project(at, 'api', ['--agents', 'claude,codex'])
  const api = join(at.workspace, 'api')
  succeed(at, 'mind', api, ['add', 'skill', 'deploy', 'Ship a release'])
  succeed(at, 'mind', api, ['add', 'rule', 'style/comments', 'Comment the why, not the what'])
  succeed(at, 'flayer', at.workspace, ['install', '-p', 'api', 'skill:review'])

  project(at, 'web', ['--agents', 'claude,opencode', '--shared'])
  for (const name of ['review', 'lint']) {
    succeed(at, 'flayer', at.workspace, ['install', '-p', 'web', `skill:${name}`])
  }
}

/** `name`, a folder of the workspace, made a project with `options` and linked. */
function project(at: Places, name: string, options: string[]): void {
  const folder = join(at.workspace, name)
  mkdirSync(folder)
  succeed(at, 'mind', folder, ['init', ...options])
  succeed(at, 'flayer', at.workspace, ['link', name])
}
