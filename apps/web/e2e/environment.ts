/*
 * What every process the end-to-end suite starts runs in: the binaries under
 * test, the temporary folder holding the fixture workspace and the home, and
 * an environment that leads to no workspace or agent home of the
 * developer's. The driver (run.ts), the config and the specs all read it
 * from here, so none of them can start a command some other way.
 */
import { spawn, spawnSync, type ChildProcess } from 'node:child_process'
import { existsSync } from 'node:fs'
import { join } from 'node:path'

/**
 * The variables that lead a command to a real workspace or a real agent
 * home, as the CLI's own tests name them (`common::CLEARED`): an activated
 * workspace, the default one's home, and the agents' folders. The driver
 * starts Playwright without them, so neither the server nor any command a
 * spec runs, nor a worker, ever sees them.
 */
export const CLEARED = [
  'MINDFLAYER_HOME',
  'MINDFLAYER_WORKSPACE',
  'CLAUDE_CONFIG_DIR',
  'CODEX_HOME',
  'OPENCODE_CONFIG_DIR'
] as const

/**
 * Every agent home empty, so nothing the server or a command runs reads a
 * real one, whatever the environment says: the four options the CLI's tests
 * pin (`common::PINNED`) beside `--home` and `--workspace`.
 */
export const NO_AGENT_HOMES = [
  '--claude-config-dir=',
  '--codex-home=',
  '--opencode-config-dir=',
  '--agents-dir='
] as const

/** The variables the driver hands the config and the workers. */
const SCRATCH = 'MINDFLAYER_E2E_SCRATCH'
const BIN = 'MINDFLAYER_E2E_BIN'

/** Where a run keeps what it builds, and what it runs. */
export interface Places {
  /** The temporary folder all of it is in, which the driver removes. */
  scratch: string
  /**
   * The home every command and the server get, as `HOME` and `--home`: made
   * empty, and held to stay that way (run.ts).
   */
  home: string
  /** The fixture workspace, holding `.mindflayer` and the projects it links. */
  workspace: string
  /** The folder holding the `flayer` and `mind` under test. */
  bin: string
}

/** The places for a run in `scratch`, of the binaries in `bin`. */
export function places(scratch: string, bin: string): Places {
  return { scratch, home: join(scratch, 'home'), workspace: join(scratch, 'workspace'), bin }
}

/** What the driver hands every process after it, in the environment. */
export function handed(at: Places): Record<string, string> {
  return { [SCRATCH]: at.scratch, [BIN]: at.bin }
}

/**
 * The places the driver chose, read back from the environment it started
 * Playwright with. Without them the suite was started some other way, which
 * would serve no fixture, or one nobody checks the home of afterwards.
 */
export function given(): Places {
  const scratch = process.env[SCRATCH]
  const bin = process.env[BIN]
  if (!scratch || !bin) {
    throw new Error('the end-to-end suite runs through its driver, which builds the fixture first: run `make web/e2e`')
  }
  for (const variable of CLEARED) {
    if (variable in process.env) {
      throw new Error(`${variable} is set, and it could lead to a real workspace or agent home: run the suite through \`make web/e2e\`, which clears it`)
    }
  }
  return places(scratch, bin)
}

/** The path of the binary called `name` in `bin`. */
export function binary(bin: string, name: 'flayer' | 'mind'): string {
  return join(bin, process.platform === 'win32' ? `${name}.exe` : name)
}

/** Whether both binaries are in `bin`, to run. */
export function built(bin: string): boolean {
  return existsSync(binary(bin, 'flayer')) && existsSync(binary(bin, 'mind'))
}

/**
 * This process's environment without any variable `CLEARED` names: what the
 * driver starts Playwright with, and what every binary is started with
 * besides its home, cleared again there so a helper used some other way
 * cannot let one through.
 */
export function cleared(): Record<string, string> {
  return Object.fromEntries(
    Object.entries(process.env).filter(
      (entry): entry is [string, string] => entry[1] !== undefined && !(CLEARED as readonly string[]).includes(entry[0])
    )
  )
}

/**
 * The environment a binary runs in: `cleared`, with the temporary home as
 * `HOME`. Not an empty one, with which the standard library falls back to
 * the account's real home.
 */
export function environment(at: Places): Record<string, string> {
  return { ...cleared(), HOME: at.home, USERPROFILE: at.home }
}

/**
 * The options every line names its home, its workspace and the agent homes
 * by: `--home` the temporary one, `--workspace` the fixture, or, before
 * there is one to name, empty, which names none and ignores
 * `$MINDFLAYER_WORKSPACE` too, and every agent home empty. Named on every
 * line even though `-C` is under the temporary folder: resolving walks up
 * from there, and only a line that names its workspace cannot be led to
 * another.
 */
export function pinned(at: Places, workspace: 'fixture' | 'none'): string[] {
  return [`--home=${at.home}`, `--workspace=${workspace === 'fixture' ? at.workspace : ''}`, ...NO_AGENT_HOMES]
}

/** What a line printed, and how it exited. */
export interface Ran {
  line: string
  status: number | null
  stdout: string
  stderr: string
}

/**
 * Run `name` with `args` in `directory`, which it is also given as `-C`, and
 * every option `pinned` names, and say what it printed.
 */
export function run(at: Places, name: 'flayer' | 'mind', directory: string, args: string[], workspace: 'fixture' | 'none' = 'fixture'): Ran {
  const line = [...args, '-C', directory, ...pinned(at, workspace)]
  const ran = spawnSync(binary(at.bin, name), line, {
    cwd: directory,
    env: environment(at),
    encoding: 'utf8',
    timeout: 60_000
  })
  if (ran.error) {
    throw ran.error
  }
  return { line: [name, ...line].join(' '), status: ran.status, stdout: ran.stdout, stderr: ran.stderr }
}

/** `run`, failing unless the line exits 0 and says no warning. */
export function succeed(at: Places, name: 'flayer' | 'mind', directory: string, args: string[], workspace: 'fixture' | 'none' = 'fixture'): Ran {
  const ran = run(at, name, directory, args, workspace)
  if (ran.status !== 0 || /^warning:/m.test(ran.stderr)) {
    throw new Error(`\`${ran.line}\` exited ${ran.status}:\n${ran.stdout}${ran.stderr}`)
  }
  return ran
}

/**
 * `flayer ui` as a person starts it over the workspace `at` names, on a
 * port the system picks, with nothing opened: what the config starts the
 * suite's server with, and `serve` a spec's own.
 */
export function ui(at: Places): string[] {
  return ['ui', '--no-open', '--port', '0', '-C', at.workspace, ...pinned(at, 'fixture')]
}

/** A `flayer ui` a spec started, and how to stop it. */
export interface Serving {
  /** Where it listens: `http://127.0.0.1:<port>`. */
  origin: string
  token: string
  /** The link it printed, which trades the token for the cookie. */
  link: string
  /** Ctrl-C, as a person stops it, and wait until it has. */
  stop(): Promise<void>
}

/** The link `flayer ui` prints first on stdout, and the only line there while it serves. */
const LINK = /^(?<origin>http:\/\/127\.0\.0\.1:\d+)\/\?token=(?<token>[0-9a-f]+)$/m

/**
 * Start `flayer ui` over the workspace `at` names, as the config starts the
 * suite's, from that workspace with the environment `environment` gives,
 * and answer once it printed its link: for a spec that changes a workspace
 * of its own, which no other spec reads. Its stdout is read, never
 * printed, so the token stays out of the log.
 */
export function serve(at: Places): Promise<Serving> {
  const child = spawn(binary(at.bin, 'flayer'), ui(at), {
    cwd: at.workspace,
    env: environment(at),
    stdio: ['ignore', 'pipe', 'pipe']
  })
  return new Promise((resolve, refuse) => {
    let printed = ''
    let said = ''
    const timer = setTimeout(() => {
      child.kill('SIGKILL')
      refuse(new Error(`flayer ui printed no link within a minute:\n${said}`))
    }, 60_000)
    child.stderr!.setEncoding('utf8').on('data', (chunk: string) => (said += chunk))
    child.stdout!.setEncoding('utf8').on('data', (chunk: string) => {
      printed += chunk
      const link = LINK.exec(printed)
      if (link?.groups) {
        clearTimeout(timer)
        resolve({ origin: link.groups.origin!, token: link.groups.token!, link: link[0], stop: () => stopped(child) })
      }
    })
    child.on('error', (error) => {
      clearTimeout(timer)
      refuse(error)
    })
    child.on('exit', (code) => {
      clearTimeout(timer)
      refuse(new Error(`flayer ui exited ${code} before it printed a link:\n${said}`))
    })
  })
}

/**
 * `child` stopped with the Ctrl-C a person stops `flayer ui` with, which
 * lets it close every event stream and finish what it runs, and killed if
 * it has not stopped ten seconds later.
 */
function stopped(child: ChildProcess): Promise<void> {
  if (child.exitCode !== null || child.signalCode !== null) {
    return Promise.resolve()
  }
  return new Promise((done) => {
    const timer = setTimeout(() => child.kill('SIGKILL'), 10_000)
    child.on('exit', () => {
      clearTimeout(timer)
      done()
    })
    child.kill('SIGINT')
  })
}
