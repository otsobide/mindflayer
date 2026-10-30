/*
 * What the specs share: the server the config started, a page opened
 * through its launch link as a person opens it, and what the CLI says of the
 * same workspace, to hold the page to.
 */
import { test as base, expect, type Page } from '@playwright/test'
import type { Envelope } from '../src/api/generated/Envelope'
import type { Listing } from '../src/api/generated/Listing'
import { given, run } from './environment.ts'

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

export const test = base.extend<{ served: Served, panel: Panel, watch: Watch }>({
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
    await use({
      async open(path = '/') {
        const landed = await page.goto(served.link)
        expect(landed?.status(), 'the server serves the panel, which a binary built before it answers with a 503').toBe(200)
        await expect(page, 'the launch link sends the browser on to the overview').toHaveURL(`${served.origin}/`)
        if (path !== '/') {
          await page.goto(new URL(path, served.origin).href)
        }
        await drawn(page)
      }
    })
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
