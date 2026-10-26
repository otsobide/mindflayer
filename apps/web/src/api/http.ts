/*
 * The one file allowed to use `fetch` and `EventSource` (see eslint.config.ts).
 */
import { TransportError, type Preview, type Query, type Run, type Task, type Transport } from './transport'

/**
 * Sent with every request.
 *
 * `X-Mindflayer` is a header no form and no plain link can add, so a request
 * carrying it was made by a script on a page the browser let set it: the
 * server's own, since it answers no CORS preflight. JSON, because that is
 * all the server reads, and it too forces the preflight on any other origin.
 */
const HEADERS = {
  'Accept': 'application/json',
  'Content-Type': 'application/json',
  'X-Mindflayer': '1'
}

/**
 * The transport for a panel the local server serves, over HTTP: the server's
 * query, preview and commands endpoints, and the event stream #74 adds, all
 * under `/api`. Its transcript has no call yet (see `Transport`).
 *
 * Relative URLs, so the panel talks to whichever server sent it, on
 * whatever port it took, or to the Vite dev server that proxies `/api` to
 * one. The launch token travels in the cookie the launch link set, which is
 * `HttpOnly`: the panel never holds the token, and only the browser sends it.
 */
export function httpTransport(): Transport {
  return {
    query<Answer>(query: Query) {
      return post<Answer>('/api/query', query)
    },

    send(task: Task, mode: 'preview' | 'run') {
      return mode === 'preview'
        ? post<Preview>('/api/commands/preview', task)
        : post<Run>('/api/commands', task)
    },

    subscribe(listener) {
      // Same origin, so the cookie goes with it; a GET changes nothing, so
      // it needs neither the header nor the JSON body EventSource cannot send.
      const source = new EventSource('/api/events')
      source.onmessage = (message: MessageEvent<string>) => {
        const event = parse(message.data)
        if (event !== undefined) {
          listener(event)
        }
      }
      return () => source.close()
    }
  } as Transport
}

async function post<Answer>(path: string, body: unknown): Promise<Answer> {
  let response: Response
  try {
    response = await fetch(path, {
      method: 'POST',
      // The default, spelled out: the cookie is the panel's credential, and
      // it must never be offered to another origin.
      credentials: 'same-origin',
      headers: HEADERS,
      body: JSON.stringify(body)
    })
  } catch {
    throw new TransportError(0, 'the server cannot be reached: is `flayer ui` still running?')
  }
  // The status came first, so the body can still be lost to a server that
  // stops halfway through it: that is no answer either.
  let text: string
  try {
    text = await response.text()
  } catch {
    throw new TransportError(0, 'the server stopped before it finished answering: is `flayer ui` still running?')
  }
  const reply = parse(text)
  if (!response.ok) {
    throw new TransportError(response.status, refusal(response, reply))
  }
  // Every answer the server gives is JSON, so one that is not came from
  // something else on the way, and passing it on as `undefined` would read
  // as an empty answer rather than a broken one.
  if (reply === undefined) {
    throw new TransportError(response.status, `the server answered ${response.status} with something that is not JSON`)
  }
  return reply as Answer
}

/** What the server said was wrong, which it says as `{"error": "..."}`. */
function refusal(response: Response, reply: unknown): string {
  if (response.status === 401) {
    return 'this browser does not hold the launch token: open the link `flayer ui` printed'
  }
  if (typeof reply === 'object' && reply !== null && 'error' in reply && typeof reply.error === 'string') {
    return reply.error
  }
  return `the server answered ${response.status} ${response.statusText}`.trimEnd()
}

/** JSON, or nothing for a body that is not. */
function parse(text: string): unknown {
  try {
    return JSON.parse(text) as unknown
  } catch {
    return undefined
  }
}
