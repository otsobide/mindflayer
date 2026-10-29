/*
 * What the suites that draw the views share: the snapshots a test of the
 * CLI wrote, a transport that answers with one, and the panel mounted as a
 * browser would mount it.
 */
import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { createMemoryHistory, type Router } from 'vue-router'
import App from '@/App.vue'
import type { Snapshot } from '@/api/generated/Snapshot'
import type { Transport } from '@/api/transport'
import { createPanel } from '@/panel'
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

/** A transport answering every query with `snapshot`, and writing each down. */
export function answering(snapshot: Snapshot) {
  const queries: unknown[] = []
  const transport = {
    query: (query: unknown) => {
      queries.push(query)
      return Promise.resolve(structuredClone(snapshot))
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
