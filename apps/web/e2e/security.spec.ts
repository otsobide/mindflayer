import { expect, test } from './panel.ts'

/*
 * The server's guard on the real binary: a request a page of another origin
 * sends carries that origin, which the browser writes and no page can
 * change, and the server refuses it whatever else it holds. Sent from the
 * page's own request context, which holds the cookie the launch link set,
 * so each refusal is of the origin alone: the same request from the panel's
 * own origin is answered.
 */
const FOREIGN = 'http://evil.example'

/** What the panel's page sends beside the cookie with every change. */
const PANEL = { 'Sec-Fetch-Site': 'same-origin', 'X-Mindflayer': '1' }

test('a query sent with the cookie from another origin is refused', async ({ page, panel, served }) => {
  await panel.open('/')
  const ask = (origin: string) =>
    page.request.post(`${served.origin}/api/query`, {
      data: { type: 'snapshot' },
      headers: { ...PANEL, Origin: origin },
      failOnStatusCode: false
    })

  const own = await ask(served.origin)
  expect(own.status(), await own.text()).toBe(200)

  const foreign = await ask(FOREIGN)
  expect(foreign.status()).toBe(403)
  expect(await foreign.json()).toEqual({ error: `this server takes requests only from its own pages, at ${served.origin}` })
})

test('a query sent with the token as Bearer from another origin is refused', async ({ request, served }) => {
  const foreign = await request.post(`${served.origin}/api/query`, {
    data: { type: 'snapshot' },
    headers: { Authorization: `Bearer ${served.token}`, Origin: FOREIGN },
    failOnStatusCode: false
  })
  expect(foreign.status()).toBe(403)
})

test('the panel itself is refused to another origin', async ({ request, served }) => {
  const foreign = await request.get(`${served.origin}/`, { headers: { Origin: FOREIGN }, failOnStatusCode: false })
  expect(foreign.status()).toBe(403)
  const own = await request.get(`${served.origin}/`, { headers: { Origin: served.origin }, failOnStatusCode: false })
  expect(own.status()).toBe(200)
})
