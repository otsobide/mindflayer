/*
 * The end-to-end suite's driver, which `make web/e2e` runs with Node:
 *
 *   MINDFLAYER_E2E_BIN=../../target/debug node e2e/run.ts [playwright test options]
 *
 * It does what Playwright's own setup cannot do in the order it needs. The
 * fixture workspace has to be there before the server starts, and Playwright
 * starts its webServer before any globalSetup runs; and the temporary home is
 * held to be untouched once the server has stopped, which Playwright does
 * only after every teardown it runs. So the driver makes the temporary
 * folder, builds the fixture with the binaries under test (fixture.ts), runs
 * `playwright test` with the options it was given and without any variable
 * that leads to a real workspace or agent home (environment.ts), and once
 * Playwright, and the server with it, has stopped, fails unless the home is
 * as it was made. Then it removes the folder.
 *
 * Under the system's temporary folder, and refused there when any folder
 * above it holds a `.mind` or a `.mindflayer`: resolving walks up the tree
 * before it looks at a home, so a fixture inside somebody's project or
 * workspace could reach theirs.
 */
import { spawn } from 'node:child_process'
import { existsSync, lstatSync, mkdirSync, mkdtempSync, readdirSync, rmSync } from 'node:fs'
import { createRequire } from 'node:module'
import { tmpdir } from 'node:os'
import { dirname, join, relative, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { built, cleared, handed, places, type Places } from './environment.ts'
import { build } from './fixture.ts'

const web = fileURLToPath(new URL('..', import.meta.url))

/** The folder above `folder`, itself included, that holds a `.mind` or a `.mindflayer`, if one does. */
function marked(folder: string): string | undefined {
  for (let at = folder; ; at = dirname(at)) {
    if (existsSync(join(at, '.mind')) || existsSync(join(at, '.mindflayer'))) {
      return at
    }
    if (dirname(at) === at) {
      return undefined
    }
  }
}

/**
 * Everything under `folder` and the folder itself, each with its size and
 * when it last changed: a file written and removed again still changes the
 * folder it was in.
 */
function look(folder: string): string[] {
  const seen = [`. ${lstatSync(folder).mtimeMs}`]
  for (const entry of readdirSync(folder, { recursive: true, withFileTypes: true })) {
    const path = join(entry.parentPath, entry.name)
    const stat = lstatSync(path)
    seen.push(`${relative(folder, path)} ${entry.isDirectory() ? 'folder' : `${stat.size} bytes`} ${stat.mtimeMs}`)
  }
  return seen.sort()
}

/** Run `playwright test` with `options` and `env`, and say how it exited. */
function playwright(options: string[], env: Record<string, string>): Promise<number> {
  const cli = createRequire(import.meta.url).resolve('@playwright/test/cli')
  const child = spawn(process.execPath, [cli, 'test', ...options], { cwd: web, env, stdio: 'inherit' })
  // A Ctrl-C reaches Playwright from the terminal as well, and Playwright
  // stops the server and its browsers; the driver waits for that, to check
  // the home and remove the folder after it. A termination sent to the
  // driver alone, as CI cancels a job, is passed on as that same SIGINT,
  // not as a SIGTERM: Playwright handles no SIGTERM, and dying of one would
  // leave `flayer ui`, in a process group of its own, serving a folder the
  // driver then removes.
  const waiting = () => {}
  const relay = () => child.kill('SIGINT')
  process.on('SIGINT', waiting)
  process.on('SIGTERM', relay)
  return new Promise((done, failed) => {
    child.on('error', failed)
    child.on('exit', (code, signal) => {
      process.off('SIGINT', waiting)
      process.off('SIGTERM', relay)
      done(code ?? (signal ? 1 : 0))
    })
  })
}

/** Say what stopped the run, before anything is made, and stop. */
function refuse(message: string): never {
  console.error(`error: ${message}`)
  process.exit(2)
}

/** The binaries' folder, as the Makefile names it, from this folder. */
const bin = resolve(web, process.env.MINDFLAYER_E2E_BIN ?? '../../target/debug')
if (!built(bin)) {
  refuse(`no flayer and mind in ${bin} to test: \`make web/e2e\` builds them there, and E2E_BIN names another folder holding them`)
}
const temporary = tmpdir()
const holder = marked(temporary)
if (holder) {
  refuse(`the temporary folder ${temporary} is inside ${holder}, which holds a .mind or a .mindflayer a command run from the fixture could reach: point TMPDIR elsewhere`)
}

const at: Places = places(mkdtempSync(join(temporary, 'mindflayer-e2e-')), bin)
let status = 1
try {
  mkdirSync(at.home)
  const made = look(at.home)
  console.error(`note: the fixture workspace is ${at.workspace}, and every command's home ${at.home}`)
  build(at)

  status = await playwright(process.argv.slice(2), { ...cleared(), ...handed(at) })

  const now = look(at.home)
  if (now.join('\n') === made.join('\n')) {
    console.error('note: the temporary home is as it was made')
  } else {
    console.error(`error: the temporary home ${at.home} is not as it was made, so something the suite ran wrote there:`)
    for (const line of now.filter(line => !made.includes(line))) {
      console.error(`  ${line}`)
    }
    status ||= 1
  }
} catch (error) {
  console.error(`error: ${error instanceof Error ? error.message : String(error)}`)
} finally {
  rmSync(at.scratch, { recursive: true, force: true })
}
process.exit(status)
