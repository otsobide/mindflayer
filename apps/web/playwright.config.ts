import { defineConfig, devices } from '@playwright/test'
import { binary, given, ui } from './e2e/environment.ts'

/*
 * The panel end to end: the binary under test serving the fixture workspace
 * its driver built (e2e/run.ts, which `make web/e2e` runs), and the specs in
 * e2e/ driving it in Chromium and in WebKit, the engine a desktop shell
 * would draw the panel with.
 */
const at = given()

/** A word as a POSIX shell reads it back: the webServer's command is one. */
function quoted(word: string): string {
  return `'${word.replaceAll('\'', '\'\\\'\'')}'`
}

export default defineConfig({
  testDir: 'e2e',
  outputDir: 'test-results',
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  // None: a test that passes only on a second go says something about the
  // panel or the server, and a retry would hide it.
  retries: 0,
  reporter: process.env.CI ? [['list'], ['github'], ['html', { open: 'never' }]] : 'list',
  use: {
    // What a failure is read from: every action, the page at each step, and
    // what it asked of the server. Kept only for a test that failed, which
    // CI uploads.
    trace: 'retain-on-failure'
  },
  projects: [
    { name: 'chromium', use: { ...devices['Desktop Chrome'] } },
    { name: 'webkit', use: { ...devices['Desktop Safari'] } }
  ],
  // `flayer ui` as a person starts it, on a port the system picks, so a run
  // never meets another on a fixed one. The link it prints first is the only
  // way to know which, so it is read with a regex, whose named groups
  // Playwright puts in the environment every worker starts with, upper
  // case: MINDFLAYER_E2E_URL and MINDFLAYER_E2E_TOKEN (e2e/panel.ts). The
  // link is never printed: the token stays out of the log, as the server
  // keeps it out of its own.
  webServer: {
    name: 'flayer ui',
    // `exec`, so the shell Playwright starts it through gives its place to
    // the server, and the Ctrl-C below reaches the server alone.
    command: `exec ${[binary(at.bin, 'flayer'), ...ui(at)].map(quoted).join(' ')}`,
    // From the fixture, so not even the process's own folder is in the
    // repository, which sits under the developer's home.
    cwd: at.workspace,
    // Over this process's environment, which the driver cleared of every
    // variable leading to a real workspace or agent home.
    env: { HOME: at.home, USERPROFILE: at.home },
    wait: { stdout: /^(?<mindflayer_e2e_url>http:\/\/127\.0\.0\.1:\d+)\/\?token=(?<mindflayer_e2e_token>[0-9a-f]+)$/m },
    stdout: 'ignore',
    stderr: 'pipe',
    timeout: 60_000,
    // Ctrl-C, as a person stops it, which lets it finish what it was doing
    // and close every event stream, rather than the kill Playwright sends
    // otherwise.
    gracefulShutdown: { signal: 'SIGINT', timeout: 10_000 }
  }
})
