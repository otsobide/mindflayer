import { inject, type InjectionKey } from 'vue'

/**
 * A read for core to answer, such as the snapshot or a file's contents.
 * `unknown` until the types are generated from Rust (#70).
 */
export type Query = unknown

/** A change, as the backend's `Task`. `unknown` until #70, like `Query`. */
export type Task = unknown

/** The command lines a task is, as somebody would type them; nothing ran. */
export interface Preview {
  lines: string[]
}

/** A task that ran: its lines, and what running them produced. */
export interface Run<Outcome = unknown> {
  lines: string[]
  outcome: Outcome
}

/** Stops a subscription. */
export type Unsubscribe = () => void

/**
 * How the panel reaches the front end behind it, and the only way it does.
 *
 * An interface rather than the HTTP calls themselves, so a desktop shell can
 * answer the same three calls without a server (D-17), and so authentication
 * lives in exactly one place: the implementation. Every request the panel
 * makes goes through one of these; a lint rule keeps `fetch` out of every
 * other file.
 *
 * The server's transcript (`GET /api/transcript`) has no call yet: nothing
 * shows it until the transcript drawer (#76), which adds the call with it.
 */
export interface Transport {
  /** Ask for a read. It changes nothing, so it may be asked again freely. */
  query<Answer = unknown>(query: Query): Promise<Answer>

  /**
   * Hand over a task: `preview` for the lines it would run, which the panel
   * shows first; `run` to queue it behind every task sent before it and wait
   * for its outcome.
   */
  send(task: Task, mode: 'preview'): Promise<Preview>
  send<Outcome = unknown>(task: Task, mode: 'run'): Promise<Run<Outcome>>

  /**
   * Be told whenever what the panel shows may have changed (#74). Each event
   * arrives parsed; nothing is sent until the server has an event stream.
   */
  subscribe(listener: (event: unknown) => void): Unsubscribe
}

/**
 * A request that did not get its answer: the server refused it, answered
 * with something that is not JSON, or could not be reached or stopped
 * before its answer was whole (status 0).
 */
export class TransportError extends Error {
  readonly status: number

  constructor(status: number, message: string) {
    super(message)
    this.name = 'TransportError'
    this.status = status
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
