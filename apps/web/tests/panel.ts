/*
 * What the suites that draw the views share: the snapshots and the other
 * answers a test of the CLI wrote, a transport that answers with them, and
 * the panel mounted as a browser would mount it.
 */
import { useToast } from '@nuxt/ui/composables'
import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { createMemoryHistory, type Router, type RouteRecordRaw } from 'vue-router'
import App from '@/App.vue'
import type { Snapshot } from '@/api/generated/Snapshot'
import { TransportError, type LiveEvent, type Preview, type Query, type Ran, type Task, type Transport } from '@/api/transport'
import { createPanel } from '@/panel'
import assigning from './fixtures/assigning.json'
import candidates from './fixtures/candidates.json'
import commands from './fixtures/commands.json'
import library from './fixtures/library.json'
import planned from './fixtures/plans.json'
import scan from './fixtures/scan.json'
import unmade from './fixtures/unmade.json'
import workspace from './fixtures/workspace.json'

/**
 * The snapshots in tests/fixtures, which `make web/fixtures` writes from
 * workspaces a test of the CLI builds by running the commands
 * (apps/cli/tests/fixtures_test.rs): `workspace` holds something of
 * everything a view shows, `unmade` is a home where nothing is made, and
 * `candidates` is `workspace` with more beside its projects, the one the
 * scan in tests/fixtures/scan.json looked through: two projects and two
 * repositories nobody linked, `old`, linked and moved since, and `legacy`,
 * linked, which a `mind.toml` marks; and `assigning`, a workspace to assign
 * in, which the matrix draws: one name offered from two places, the shelf,
 * a copy of each state an install stands in, a project nothing can go into
 * and an entry that opens none.
 */
const FIXTURES = { workspace, unmade, candidates, assigning }

export type Fixture = keyof typeof FIXTURES

/**
 * A fixture as the server would answer it, copied, so a test may change its
 * copy. Imported as JSON, whose words TypeScript reads as any string, so it
 * is said to be the snapshot it was written as.
 */
export function fixture(name: Fixture): Snapshot {
  return structuredClone(FIXTURES[name]) as unknown as Snapshot
}

/** A query, and what the server answers it with. */
export interface Answered {
  query: Query
  answer: unknown
}

/**
 * What the workspace fixture's server answers when each of its library's
 * own is opened (tests/fixtures/library.json): the files of each, and each
 * file of them, written as the snapshots are. Copied, so a test may change
 * its copy.
 */
export function answers(): Answered[] {
  return structuredClone(library) as unknown as Answered[]
}

/**
 * What the workspace fixture's server answers when its page of what could
 * be linked scans it (tests/fixtures/scan.json): the workspace's folder, to
 * core's own depth, with what there is to find beside its projects, written
 * as the snapshots are. Copied, so a test may change its copy.
 */
export function scans(): Answered[] {
  return structuredClone(scan) as unknown as Answered[]
}

/**
 * What the assigning fixture's server answers when its matrix plans a few
 * selections of boxes (tests/fixtures/plans.json), each in the order the
 * boxes were marked: `format` into `api` and `docs`, the library's `deploy`
 * in place of the team's in `api`, and a selection that clashes and is
 * locked beside a change that goes ahead. Copied, so a test may change its
 * copy.
 */
export function plans(): Answered[] {
  return structuredClone(planned) as unknown as Answered[]
}

/** `value` with every object's keys in one order, to compare as text. */
function sorted(value: unknown): unknown {
  if (Array.isArray(value)) {
    return value.map(sorted)
  }
  if (typeof value === 'object' && value !== null) {
    return Object.fromEntries(Object.entries(value).sort(([a], [b]) => a.localeCompare(b)).map(([key, each]) => [key, sorted(each)]))
  }
  return value
}

/**
 * A transport answering the snapshot with `snapshot`, and any other query
 * with the answer `answered` holds for it, what opening the library's own
 * and scanning are answered with unless a test says otherwise, refusing
 * one it holds none for as the server refuses what it cannot answer; each
 * query written down.
 */
export function answering(snapshot: Snapshot, answered: Answered[] = [...answers(), ...scans()]) {
  const queries: Query[] = []
  const transport = {
    query: (query: Query) => {
      queries.push(query)
      if (query.type === 'snapshot') {
        return Promise.resolve(structuredClone(snapshot))
      }
      const asked = JSON.stringify(sorted(query))
      const found = answered.find(each => JSON.stringify(sorted(each.query)) === asked)
      return found
        ? Promise.resolve(structuredClone(found.answer))
        : Promise.reject(new TransportError(422, `not_found: nothing answers ${asked}`))
    },
    send: () => new Promise(() => {}),
    transcript: () => Promise.resolve([]),
    subscribe: () => () => {}
  } as unknown as Transport
  return { queries, transport }
}

/** A task, its preview, and what its run is answered with. */
export interface Commanded {
  task: Task
  preview: Preview
  ran: Ran
}

/**
 * What the workspace fixture's server answers when each of a few tasks is
 * previewed, then run, in turn (tests/fixtures/commands.json, written as the
 * snapshots are): a link, an unlink to confirm, a validate that fails with
 * a code, a removal everywhere. Copied, so a test may change its copy.
 */
export function commanded(): Commanded[] {
  return structuredClone(commands) as unknown as Commanded[]
}

/** A task as `send` was handed it, and how. */
export interface Sent {
  task: Task
  mode: 'preview' | 'run'
}

/**
 * A transport answering queries as `answering` does, with `answered`
 * when given, and each task of `known` as the server answered it: its preview, and its run, told as
 * started and finished on the stream before it is answered, as the server
 * tells them, and kept in the transcript. Any other task is refused with a
 * code, as the backend refuses one. Each task sent is written down, and
 * `finishing` holds a run's finish back until the test lets it go.
 */
export function commanding(snapshot: Snapshot = fixture('workspace'), known: Commanded[] = commanded(), answered?: Answered[]) {
  const { queries, transport } = answering(snapshot, answered)
  const sent: Sent[] = []
  const ran: Ran[] = []
  const listeners: ((event: LiveEvent) => void)[] = []
  const tell = (event: LiveEvent) => [...listeners].forEach(listener => listener(event))
  const control = { finishing: undefined as Promise<void> | undefined }
  const commanding = {
    ...transport,
    async send(task: Task, mode: 'preview' | 'run') {
      // As JSON, as the HTTP transport sends it: what the runner hands over
      // may be one of Vue's proxies, which structuredClone refuses.
      sent.push({ task: JSON.parse(JSON.stringify(task)) as Task, mode })
      const asked = JSON.stringify(sorted(task))
      const found = known.find(each => JSON.stringify(sorted(each.task)) === asked)
      if (!found) {
        throw new TransportError(422, `nothing answers ${asked}`, 'not_found')
      }
      if (mode === 'preview') {
        return structuredClone(found.preview)
      }
      tell({ type: 'command_started', lines: found.ran.lines })
      await control.finishing
      ran.push(structuredClone(found.ran))
      tell({ type: 'command_finished', lines: found.ran.lines })
      return structuredClone(found.ran)
    },
    transcript: () => Promise.resolve(structuredClone(ran)),
    subscribe(listener: (event: LiveEvent) => void) {
      listeners.push(listener)
      return () => {
        listeners.splice(listeners.indexOf(listener), 1)
      }
    }
  } as unknown as Transport
  return { queries, sent, ran, control, tell, transport: commanding }
}

/** A transport refusing every query with `error`. */
export function refusing(error: Error): Transport {
  return {
    query: () => Promise.reject(error),
    send: () => new Promise(() => {}),
    transcript: () => Promise.reject(error),
    subscribe: () => () => {}
  } as unknown as Transport
}

let mounted: VueWrapper | undefined

export interface Rendered {
  page: VueWrapper
  router: Router
}

/**
 * The panel at `path`, asking `transport`, once what it asked is answered;
 * with `routes` beside its own, for a test to mount a page of its own in
 * the panel's frame.
 */
export async function render(path: string, transport: Transport | Snapshot, routes: RouteRecordRaw[] = []): Promise<Rendered> {
  const asked = 'query' in transport ? transport : answering(transport).transport
  const panel = createPanel({ transport: asked, history: createMemoryHistory() })
  for (const route of routes) {
    panel.router.addRoute(route)
  }
  await panel.router.push(path)
  mounted = mount(App, { global: { plugins: [panel] }, attachTo: document.body })
  await flushPromises()
  return { page: mounted, router: panel.router }
}

/**
 * Take down what `render` mounted, and the toasts it left: Nuxt UI keeps
 * them in a state of its own, which outlives the app.
 */
export function unmount() {
  mounted?.vm.$.appContext.app.runWithContext(() => useToast().clear())
  mounted?.unmount()
  mounted = undefined
  document.body.innerHTML = ''
}

/** What can be searched for elements: a mounted page, or an element in it. */
interface Searched {
  findAll(selector: string): { text(): string }[]
}

/** The text of each element `selector` finds, trimmed and with its spaces run together. */
export function texts(within: Searched, selector: string): string[] {
  return within.findAll(selector).map(found => found.text().replace(/\s+/g, ' ').trim())
}
