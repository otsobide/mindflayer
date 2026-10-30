import { inject, type InjectionKey } from 'vue'
import type { Confirm } from './generated/Confirm'
import type { Contents } from './generated/Contents'
import type { Files } from './generated/Files'
import type { Holder } from './generated/Holder'
import type { Outcome } from './generated/Outcome'
import type { Query } from './generated/Query'
import type { Scanned } from './generated/Scanned'
import type { Snapshot } from './generated/Snapshot'
import type { Task } from './generated/Task'

/*
 * What the panel sends and is answered with is the backend's, generated
 * from Rust into `generated/` (make web/types): a read for core to answer
 * (`Query`), a change (`Task`) and what running one produced (`Outcome`).
 */
export type { Outcome, Query, Task }

/**
 * What each query is answered with, by its `type`. Every query names its
 * answer here: one Rust adds that is not named is an index `Answer` cannot
 * take, so the panel does not compile until it is.
 */
export interface Answers {
  snapshot: Snapshot
  files: Files
  contents: Contents
  scan: Scanned
}

/** What `query` is answered with. */
export type Answer<Asked extends Query> = Answers[Asked['type']]

/**
 * The command lines a task is, as somebody would type them, and what of
 * them the panel has somebody confirm before it runs them, none for a task
 * it runs once asked; nothing ran. Which lines take something away is the
 * backend's to say, in the generated `Confirm`, never the panel's.
 */
export interface Preview {
  lines: string[]
  confirm: Confirm[]
}

/**
 * A task that ran: its lines, and what running them produced, each line's
 * own run among it (the generated `Run`). What a run is answered with, and
 * each entry of the transcript.
 */
export interface Ran {
  lines: string[]
  outcome: Outcome
}

/**
 * What the server's event stream tells (`GET /api/events`), each told
 * apart by its `type`:
 *
 * - `changed`: something the server watches changed on disk, so what is
 *   shown of each holder in `scopes` may be out of date;
 * - `command_started` and `command_finished`: a task's lines started to
 *   run, and stopped, however that went;
 * - `resync`: something may have been missed, as on connecting, so
 *   everything is asked for again;
 * - `watching` and `unwatched`: whether everything the server was told to
 *   watch is, and why not, when a change there shows only once asked for
 *   by hand.
 *
 * Spelled here, beside the server's other replies, rather than generated
 * from Rust: the stream is the server's own, and the server is a crate of
 * its own with no part in `make web/types`, as `{lines, confirm}` and
 * `{lines, outcome}` are. What a change is to is the backend's, a holder as
 * a query names it, so that much is the generated `Holder`.
 */
export type LiveEvent
  = | { type: 'changed', scopes: Holder[] }
    | { type: 'command_started', lines: string[] }
    | { type: 'command_finished', lines: string[] }
    | { type: 'resync' }
    | { type: 'watching' }
    | { type: 'unwatched', reason: string }

/** Stops a subscription. */
export type Unsubscribe = () => void

/**
 * How the panel reaches the front end behind it, and the only way it does.
 *
 * An interface rather than the HTTP calls themselves, so a desktop shell can
 * answer the same four calls without a server (D-17), and so authentication
 * lives in exactly one place: the implementation. Every request the panel
 * makes goes through one of these; a lint rule keeps `fetch` out of every
 * other file.
 */
export interface Transport {
  /** Ask for a read. It changes nothing, so it may be asked again freely. */
  query<Asked extends Query>(query: Asked): Promise<Answer<Asked>>

  /**
   * Hand over a task: `preview` for the lines it would run, which the panel
   * shows first; `run` to queue it behind every task sent before it and wait
   * for its outcome.
   */
  send(task: Task, mode: 'preview'): Promise<Preview>
  send(task: Task, mode: 'run'): Promise<Ran>

  /**
   * Every task the session ran, oldest first, each with what it produced:
   * the server's transcript, which it keeps for as long as it runs.
   */
  transcript(): Promise<Ran[]>

  /**
   * Be told whenever what the panel shows may have changed, or how the
   * server watches for it, until unsubscribed. `resync` comes first, and
   * again whenever something may have been missed, a connection lost and
   * made again among them.
   */
  subscribe(listener: (event: LiveEvent) => void): Unsubscribe
}

/**
 * A request that did not get its answer: the server refused it, answered
 * with something that is not JSON, or could not be reached or stopped
 * before its answer was whole (status 0).
 *
 * With the code of the condition when the backend refused it, as every
 * refusal it reads a request through says one (`{"error", "code"}`): the
 * panel acts on a refusal by its code, as on a run's warnings, and never on
 * its words.
 */
export class TransportError extends Error {
  readonly status: number
  readonly code?: string

  constructor(status: number, message: string, code?: string) {
    super(message)
    this.name = 'TransportError'
    this.status = status
    this.code = code
  }

  /** Whether the server wants the launch token this browser does not hold. */
  get unauthorized(): boolean {
    return this.status === 401
  }
}

export const transportKey: InjectionKey<Transport> = Symbol('transport')

/** The transport the panel was started with. */
export function useTransport(): Transport {
  const transport = inject(transportKey)
  if (!transport) {
    throw new Error('the panel was started without a transport')
  }
  return transport
}
