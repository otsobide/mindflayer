/*
 * What the specs share: the server the config started, a page opened
 * through its launch link as a person opens it, and what the CLI says of the
 * same workspace, to hold the page to.
 */
import { cpSync, mkdtempSync, readdirSync, readFileSync, statSync } from 'node:fs'
import { join, relative } from 'node:path'
import { test as base, expect, type Page } from '@playwright/test'
import type { Envelope } from '../src/api/generated/Envelope'
import type { Listing } from '../src/api/generated/Listing'
import { given, run, serve, type Places } from './environment.ts'
import { assignable, authorable, changeable, updatable } from './fixture.ts'

/** The server under test, as the link it printed says. */
export interface Served {
  /** Where it listens, as a browser writes the origin: `http://127.0.0.1:<port>`. */
  origin: string
  token: string
  /** The link it printed, which trades the token for the cookie. */
  link: string
}

/**
 * The server the webServer started, from what the config's regex read from
 * its link and Playwright put in the environment before it started any
 * worker.
 */
function served(): Served {
  const origin = process.env.MINDFLAYER_E2E_URL
  const token = process.env.MINDFLAYER_E2E_TOKEN
  if (!origin || !token) {
    throw new Error('no server said where it listens: run the suite with `make web/e2e`')
  }
  return { origin, token, link: `${origin}/?token=${token}` }
}

/** What a test's page said that the suite fails on. */
export interface Watch {
  /** Each CSP violation it logged, in its own words or the browser's. */
  violations: string[]
  /** Each error it threw that nothing caught. */
  thrown: string[]
}

/**
 * How a CSP violation reads in a console: as `report` says it, or as the
 * browser does, each engine in words of its own that all name the policy.
 */
const VIOLATION = /csp violation|content security policy/i

/**
 * Run in the page before any script of its own: every violation said in the
 * console in one form. Each engine fires the event, while what it writes in
 * the console by itself differs, and Playwright may not pass on what a
 * browser rather than a script logs. Playwright's own way in, beside the
 * page's scripts, so the CSP that refuses an inline script does not refuse
 * this one.
 */
function report(): void {
  document.addEventListener('securitypolicyviolation', (event) => {
    console.error(`csp violation: ${event.effectiveDirective} refused ${event.blockedURI || 'inline'} on ${location.pathname}`)
  })
}

/** A page of the panel, opened as a person opens it. */
export interface Panel {
  /**
   * Open the launch link, which sets the cookie and lands on the overview,
   * then `path`, and wait for the page to draw what the snapshot says.
   */
  open(path?: string): Promise<void>
}

/** Open `served`'s launch link in `page`, then `path`, as `Panel.open` says. */
async function open(page: Page, served: Served, path: string): Promise<void> {
  const landed = await page.goto(served.link)
  expect(landed?.status(), 'the server serves the panel, which a binary built before it answers with a 503').toBe(200)
  await expect(page, 'the launch link sends the browser on to the overview').toHaveURL(`${served.origin}/`)
  if (path !== '/') {
    await page.goto(new URL(path, served.origin).href)
  }
  await drawn(page)
}

/**
 * A workspace of a test's own, served by a `flayer ui` of its own, for a
 * test that changes what it serves: built by `changeable`, or what the
 * spec's `building` names, beside a copy of
 * it made before anything else touched it (`twin`), on which the test runs
 * the command the panel ran, to hold what each left byte for byte.
 *
 * Of its own, so neither the specs that read the suite's fixture while it
 * changes, nor the same test in the other browser, ever see the change.
 */
export interface Own extends Panel {
  /** Where it is, as every command a test runs on it names it. */
  at: Places
  /** The copy, as every command names it. */
  twin: Places
  served: Served
}

/**
 * What a test's own workspace is built as: `changeable`, for the links and
 * unlinks, unless the spec says `assignable`, for the matrix
 * (`test.use({ building: 'assignable' })`), `updatable`, for updating and
 * removing, or `authorable`, for editing, adding and renaming in the
 * library.
 */
const BUILDERS = { changeable, assignable, updatable, authorable }

export const test = base.extend<{ served: Served, panel: Panel, watch: Watch, own: Own, building: keyof typeof BUILDERS }>({
  building: ['changeable', { option: true }],

  served: async ({}, use) => {
    await use(served())
  },

  // Every test's page, watched from before it opens anything to after the
  // test is done: a violation or an uncaught error on any page fails the
  // test it happened in, whatever that test was about.
  watch: [async ({ page }, use) => {
    const watch: Watch = { violations: [], thrown: [] }
    await page.addInitScript(report)
    page.on('console', (message) => {
      if (VIOLATION.test(message.text())) {
        watch.violations.push(message.text())
      }
    })
    page.on('pageerror', error => watch.thrown.push(error.message))
    await use(watch)
    expect(watch.violations, 'the CSP violations the page logged').toEqual([])
    expect(watch.thrown, 'what the page threw').toEqual([])
  }, { auto: true }],

  panel: async ({ page, served }, use) => {
    await use({ open: (path = '/') => open(page, served, path) })
  },

  // Under the run's temporary folder, which the driver removes, and served
  // with the run's home, which the driver holds to stay as it was made.
  own: async ({ page, building }, use) => {
    const at = given()
    const folder = mkdtempSync(join(at.scratch, 'own-'))
    const mine: Places = { ...at, workspace: join(folder, 'workspace') }
    const twin: Places = { ...at, workspace: join(folder, 'twin') }
    BUILDERS[building](mine)
    cpSync(mine.workspace, twin.workspace, { recursive: true })
    const server = await serve(mine)
    try {
      await use({ at: mine, twin, served: server, open: (path = '/') => open(page, server, path) })
    } finally {
      await server.stop()
    }
  }
})

export { expect }

/**
 * Wait until the page is drawn: its panel there, the snapshot answered, and
 * every font it loads loaded, so a font the CSP refused has been refused by
 * the time a spec looks.
 */
export async function drawn(page: Page): Promise<void> {
  await expect(page.getByRole('main')).toBeVisible()
  await expect(page.getByTestId('reading')).toHaveCount(0)
  await page.evaluate(async () => {
    await document.fonts.ready
  })
}

/**
 * What `flayer list --json` says of the fixture workspace, run as every
 * command of the suite is (environment.ts), in the envelope it prints:
 * version 2, and no warning, since the fixture is built clean.
 */
export function listing(): Listing {
  const at = given()
  const ran = run(at, 'flayer', at.workspace, ['list', '--json'])
  expect(ran.status, `${ran.line}\n${ran.stderr}`).toBe(0)
  const envelope = JSON.parse(ran.stdout) as Envelope<Listing>
  expect(envelope.version).toBe(2)
  expect(envelope.warnings).toEqual([])
  return envelope.data
}

/**
 * Every file under `root` but its `.mind`, by its route, with what it
 * holds: what a project holds, to hold what the panel left in one to what
 * the same lines left in its twin.
 */
export function tree(root: string): Record<string, string> {
  const found: Record<string, string> = {}
  const walk = (folder: string) => {
    for (const name of readdirSync(folder)) {
      const path = join(folder, name)
      if (name === '.mind') {
        continue
      }
      if (statSync(path).isDirectory()) {
        walk(path)
      } else {
        found[relative(root, path)] = readFileSync(path, 'utf8')
      }
    }
  }
  walk(root)
  return found
}
