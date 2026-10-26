import { describe, expect, it } from 'vitest'
import { TransportError } from '@/api/transport'
import { retry } from '@/query'

describe('asking the server again', () => {
  it('does not ask again what the server refused', () => {
    for (const status of [400, 401, 404, 422]) {
      expect(retry(0, new TransportError(status, 'refused')), String(status)).toBe(false)
    }
  })

  it('asks twice more a server that failed or could not be reached', () => {
    for (const status of [0, 500, 502]) {
      const error = new TransportError(status, 'failed')
      expect([0, 1, 2].map(failures => retry(failures, error)), String(status)).toEqual([true, true, false])
    }
  })
})
