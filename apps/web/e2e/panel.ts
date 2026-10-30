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

/** A page of the panel, opened as a person opens it. */
export interface Panel {
  /**
   * Open the launch link, which sets the cookie and lands on the overview,
   * then `path`, and wait for the page to draw what the snapshot says.
   */
  open(path?: string): Promise<void>
}

export const test = base.extend<{ served: Served, panel: Panel }>({
  served: async ({}, use) => {
    await use(served())
  },

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

/** Wait until the page is drawn: its panel there, and the snapshot answered. */
export async function drawn(page: Page): Promise<void> {
  await expect(page.getByRole('main')).toBeVisible()
  await expect(page.getByTestId('reading')).toHaveCount(0)
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
