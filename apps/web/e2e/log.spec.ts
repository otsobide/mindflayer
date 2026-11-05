import { join } from 'node:path'
import { expect, pressure, stalled, test } from './panel.ts'

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

test('the machine\'s own pressure reads as a failed test keeps it, where Linux says it', () => {
  const before = pressure()
  test.skip(Object.keys(before).length === 0, 'no /proc/pressure on this system')

  const said = stalled(before, pressure(), 0).split('\n')

  for (const resource of Object.keys(before)) {
    expect(said, resource).toContainEqual(expect.stringMatching(new RegExp(`^${resource}: some \\d+ms(, full \\d+ms)? of 0ms$`)))
  }
})
