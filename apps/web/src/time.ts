/*
 * The moments the ledger keeps, in seconds since the epoch, as a person
 * reads them: in this browser's language and time zone, which only it
 * knows, and in a `<time>` element's own spelling beside, for anything
 * that reads the page rather than looks at it.
 */

/** `seconds` as a date and a time, in this browser's words. */
export function when(seconds: number): string {
  return new Date(seconds * 1000).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' })
}

/** `seconds` as a `<time>` element's `datetime` takes it. */
export function instant(seconds: number): string {
  return new Date(seconds * 1000).toISOString()
}
