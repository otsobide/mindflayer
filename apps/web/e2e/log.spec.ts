import { mkdtempSync, readdirSync, readFileSync } from 'node:fs'
import { join } from 'node:path'
import type { TestInfo } from '@playwright/test'
import { given } from './environment.ts'
import { close, type Ending, expect, keep, keepStalls, pressure, stalled, test } from './panel.ts'

/*
 * What a test that fails keeps beside its trace: the log its own
 * `mindweaver ui` says on stderr (`--log`), read here while the test runs,
 * as the fixture reads it once the server has stopped (panel.ts). A run
 * from the panel is said in it as the writer thread takes it, as its one
 * line has run, and as it ends, and its request once answered, each after
 * the time of day and with how long it took: what says where a run's time
 * went, which the trace, timing each request from the browser, cannot
 * (#240). Never the token, though the link and every request carried it,
 * nor the body the run was sent, which names its folder: a request is said
 * by its path and query alone.
 *
 * And how long the machine stalled while the test ran, on its processors,
 * its memory and its disk, which says whether a run that took seconds
 * waited on the machine rather than on anything of the panel's: worked out
 * from two readings of Linux's pressure files, as a failed test keeps it
 * (`stalled`), and read from this machine's own where it has them.
 *
 * Each kept, and attached to the report, only for a test that ended other
 * than it was expected to (`keep`), which no test here is: so the
 * fixtures' teardowns (`close`, `keepStalls`) are handed a stand-in for
 * one that failed (`ending`).
 */

/** The time of day a line of the log starts with, in UTC to the millisecond. */
const AT = String.raw`\d\d:\d\d:\d\d\.\d{3}Z`

/** Whether `log` holds a line saying `said`, a pattern with `\d+ms` for each time, after the time of day. */
function holds(log: string[], said: string): boolean {
  const line = new RegExp(`^${AT} ${said}$`)
  return log.some(each => line.test(each))
}

test('a run from the panel is said in its own server\'s log, as it starts, as its line runs and as it ends, and never the token', async ({ page, own }) => {
  const notes = join(own.at.workspace, 'notes')
  await own.open('/projects')

  await page.getByTestId('link-path').fill(notes)
  await page.getByTestId('link').click()
  const dialog = page.getByRole('dialog')
  await dialog.getByTestId('run').click()
  await expect(dialog).toBeHidden()

  // Said once the answer has gone, which reaches the browser before the
  // line reaches this process through the server's stderr.
  await expect.poll(() => holds(own.log(), String.raw`POST /api/v1/tasks 200 \d+ms`)).toBe(true)
  const log = own.log()
  expect(log[0], 'where it serves, first, as ever').toBe(`note: serving the workspace at ${own.at.workspace} on ${new URL(own.served.origin).host} until Ctrl-C`)
  for (const said of [
    String.raw`GET /api/v1/snapshot 200 \d+ms`,
    String.raw`POST /api/v1/tasks/preview 200 \d+ms`,
    String.raw`run: \d+ms waiting its turn, \d+ms working out its 1 line`,
    String.raw`run: line 1 of 1 took \d+ms, \d+ms of it waiting for a workspace's lock`,
    String.raw`run: \d+ms running its 1 line`
  ]) {
    expect(holds(log, said), `${said}, in:\n${log.join('\n')}`).toBe(true)
  }
  const whole = log.join('\n')
  expect(whole).not.toContain(own.served.token)
  expect(whole, 'what the run was sent').not.toContain(notes)
})

/** A pressure file as Linux writes one, its totals in microseconds. */
function file(some: number, full: number): string {
  return `some avg10=1.50 avg60=0.40 avg300=0.10 total=${some}\nfull avg10=0.75 avg60=0.20 avg300=0.05 total=${full}\n`
}

test('how long the machine stalled is said from the totals of two readings, as a failed test keeps it beside its trace', () => {
  const before = { cpu: file(1_000_000, 0), io: file(2_000_000, 1_000_000) }
  const after = { cpu: file(1_230_000, 0), io: file(6_120_000, 4_870_000) }

  const said = stalled(before, after, 6_213).split('\n')

  expect(said.slice(0, 2)).toEqual(['cpu: some 230ms, full 0ms of 6213ms', 'io: some 4120ms, full 3870ms of 6213ms'])
  expect(said.slice(2)).toEqual(['', '/proc/pressure/cpu:', ...file(1_230_000, 0).trimEnd().split('\n'), '', '/proc/pressure/io:', ...file(6_120_000, 4_870_000).trimEnd().split('\n'), ''])
})

/** What a stand-in for a test's `TestInfo` was handed to attach to the report. */
interface Attached {
  name: string
  path?: string
  contentType?: string
}

/**
 * A stand-in for the `TestInfo` of a test that ended as `status`, expected
 * to end as `expected`: its files in a folder of its own under the run's
 * temporary one, which the driver removes, and what it was handed to attach
 * in `attached`, rather than in this test's own report.
 */
function ending(status: TestInfo['expectedStatus'], expected: TestInfo['expectedStatus'] = 'passed'): { info: Ending, folder: string, attached: Attached[] } {
  const folder = mkdtempSync(join(given().scratch, 'ending-'))
  const attached: Attached[] = []
  const info: Ending = {
    status,
    expectedStatus: expected,
    outputPath: (...parts: string[]) => join(folder, ...parts),
    attach: (name, options) => {
      attached.push({ name, path: options?.path, contentType: options?.contentType })
      return Promise.resolve()
    }
  }
  return { info, folder, attached }
}

test('what a test keeps beside its trace is kept and attached only when it ended other than it was expected to', async () => {
  for (const [status, expected] of [['failed', 'passed'], ['timedOut', 'passed'], ['interrupted', 'passed'], ['passed', 'failed']] as const) {
    const ended = ending(status, expected)

    await keep(ended.info, 'kept.txt', () => 'said\n')

    const path = join(ended.folder, 'kept.txt')
    expect(readFileSync(path, 'utf8'), `${status}, expected ${expected}`).toBe('said\n')
    expect(ended.attached, `${status}, expected ${expected}`).toEqual([{ name: 'kept.txt', path, contentType: 'text/plain' }])
  }
  for (const [status, expected] of [['passed', 'passed'], ['failed', 'failed'], ['skipped', 'skipped']] as const) {
    const ended = ending(status, expected)

    await keep(ended.info, 'kept.txt', () => {
      throw new Error(`asked what to keep of a test that ended ${status}, as expected`)
    })

    expect(readdirSync(ended.folder), `${status}, expected ${expected}`).toEqual([])
    expect(ended.attached, `${status}, expected ${expected}`).toEqual([])
  }
})

test('a test that fails keeps all its own server said beside its trace once the server has stopped, and never the token', async ({ page, own }) => {
  await own.open('/')
  // Off the panel before its server stops, so nothing the page would ask
  // of a server that is gone fails the test.
  await page.goto('about:blank')

  const passed = ending('passed')
  await close(passed.info, own.served)
  expect(readdirSync(passed.folder), 'what a test that passed keeps').toEqual([])
  expect(passed.attached).toEqual([])

  const failed = ending('failed')
  await close(failed.info, own.served)
  const path = join(failed.folder, 'mindweaver-ui.log')
  expect(failed.attached).toEqual([{ name: 'mindweaver-ui.log', path, contentType: 'text/plain' }])
  const kept = readFileSync(path, 'utf8')
  expect(kept, 'all it said, as it has stopped').toBe(own.served.said())
  const log = kept.split('\n').filter(line => line !== '')
  expect(log[0], 'where it serves, first, as ever').toBe(`note: serving the workspace at ${own.at.workspace} on ${new URL(own.served.origin).host} until Ctrl-C`)
  for (const said of [
    String.raw`GET /\?token=<redacted> 303 \d+ms`,
    String.raw`GET /api/v1/snapshot 200 \d+ms`
  ]) {
    expect(holds(log, said), `${said}, in:\n${kept}`).toBe(true)
  }
  expect(kept).not.toContain(own.served.token)
})

test('a test that fails keeps how long the machine stalled while it ran beside its trace, where Linux says it', async () => {
  const silent = ending('failed')
  await keepStalls(silent.info, {}, Date.now())
  expect(readdirSync(silent.folder), 'what a system with no /proc/pressure keeps').toEqual([])
  expect(silent.attached).toEqual([])

  const before = pressure()
  test.skip(Object.keys(before).length === 0, 'no /proc/pressure on this system')
  const passed = ending('passed')
  await keepStalls(passed.info, before, Date.now())
  expect(readdirSync(passed.folder), 'what a test that passed keeps').toEqual([])

  const failed = ending('failed')
  await keepStalls(failed.info, before, Date.now())

  const path = join(failed.folder, 'pressure.txt')
  expect(failed.attached).toEqual([{ name: 'pressure.txt', path, contentType: 'text/plain' }])
  const kept = readFileSync(path, 'utf8').split('\n')
  for (const resource of Object.keys(before)) {
    expect(kept, resource).toContainEqual(expect.stringMatching(new RegExp(`^${resource}: some \\d+ms(, full \\d+ms)? of \\d+ms$`)))
    expect(kept, resource).toContain(`/proc/pressure/${resource}:`)
  }
})
