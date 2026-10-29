/*
 * What the suites that draw the views share: the snapshots and the other
 * answers a test of the CLI wrote, a transport that answers with them, and
 * the panel mounted as a browser would mount it.
 */
import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { createMemoryHistory, type Router } from 'vue-router'
import App from '@/App.vue'
import type { Snapshot } from '@/api/generated/Snapshot'
import { TransportError, type Query, type Transport } from '@/api/transport'
import { createPanel } from '@/panel'
import library from './fixtures/library.json'
import unmade from './fixtures/unmade.json'
import workspace from './fixtures/workspace.json'

/**
 * The snapshots in tests/fixtures, which `make web/fixtures` writes from
 * workspaces a test of the CLI builds by running the commands
 * (apps/cli/tests/fixtures_test.rs): `workspace` holds something of
 * everything a view shows, `unmade` is a home where nothing is made.
 */
const FIXTURES = { workspace, unmade }

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
 * with the answer `answered` holds for it, refusing one it holds none for
 * as the server refuses what it cannot answer; each query written down.
 */
export function answering(snapshot: Snapshot, answered: Answered[] = answers()) {
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
    subscribe: () => () => {}
  } as unknown as Transport
  return { queries, transport }
}

/** A transport refusing every query with `error`. */
export function refusing(error: Error): Transport {
  return {
    query: () => Promise.reject(error),
    send: () => new Promise(() => {}),
    subscribe: () => () => {}
  } as unknown as Transport
}

let mounted: VueWrapper | undefined

export interface Rendered {
  page: VueWrapper
  router: Router
}

/** The panel at `path`, asking `transport`, once what it asked is answered. */
export async function render(path: string, transport: Transport | Snapshot): Promise<Rendered> {
  const asked = 'query' in transport ? transport : answering(transport).transport
  const panel = createPanel({ transport: asked, history: createMemoryHistory() })
  await panel.router.push(path)
  mounted = mount(App, { global: { plugins: [panel] }, attachTo: document.body })
  await flushPromises()
  return { page: mounted, router: panel.router }
}

/** Take down what `render` mounted. */
export function unmount() {
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
