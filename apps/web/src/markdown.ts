import { Lexer, type Token, type Tokens } from 'marked'
import { h, type VNode, type VNodeChild } from 'vue'

/*
 * Markdown somebody else wrote, as the panel draws it: a skill's body, which
 * a repository the panel has never seen may have filled with anything.
 *
 * Drawn from marked's tokens, an element at a time, never from HTML: no
 * string is handed to the browser to parse, so nothing in the text becomes
 * an element the panel did not make. marked's own renderer, put on the page
 * with v-html, was the other way, which would trust a sanitizer to have
 * caught every way HTML can run something, and which writes a table's
 * alignment as an inline style the CSP refuses. Here:
 *
 * - Raw HTML is shown as the text it is, in a code span or block, so it
 *   reads as markup and never runs or styles anything.
 * - No element gets a `style` or an event attribute, and no class but the
 *   panel's own.
 * - A link to another site stays a link, opened in a tab of its own with no
 *   referrer. Any other, to a file of the skill's above all, is its text
 *   alone: the panel never links to an artifact's file, and a `javascript:`
 *   or `data:` URL never reaches an attribute.
 * - An image is never loaded: its description is shown in its place. The
 *   CSP would refuse one from another site, and one beside the skill is a
 *   file the server does not serve.
 */

/** The protocols a link keeps its address for: another site, or a mail. */
const OPEN = new Set(['https:', 'http:', 'mailto:'])

/** The heading level a document's own headings start below. */
const SECTION = 2

/**
 * `markdown` as the elements that draw it.
 *
 * Its headings start one level below the section it is drawn in, and never
 * go down more than one level at a time, whatever the document skips, so
 * the page's outline stays one a screen reader can follow.
 */
export function rendered(markdown: string): VNode[] {
  const tokens = new Lexer({ gfm: true }).lex(markdown)
  const outline = { level: SECTION }
  return blocks(tokens, outline)
}

/** Where the last heading drawn left the outline. */
interface Outline {
  level: number
}

function blocks(tokens: Token[], outline: Outline): VNode[] {
  return tokens.flatMap(token => block(token, outline))
}

function block(token: Token, outline: Outline): VNode[] {
  switch (token.type) {
    case 'space':
    case 'def':
      return []
    case 'paragraph':
      return [h('p', { class: 'my-2' }, inline((token as Tokens.Paragraph).tokens))]
    case 'heading': {
      const heading = token as Tokens.Heading
      const level = Math.max(SECTION + 1, Math.min(SECTION + heading.depth, outline.level + 1, 6))
      outline.level = level
      return [h(`h${level}`, { class: HEADING[level] }, inline(heading.tokens))]
    }
    case 'code':
      return [h('pre', { class: PRE, tabindex: 0 }, h('code', (token as Tokens.Code).text))]
    case 'blockquote':
      return [h('blockquote', { class: 'my-2 border-s-2 border-default ps-3 text-muted' }, blocks((token as Tokens.Blockquote).tokens, outline))]
    case 'list':
      return [list(token as Tokens.List, outline)]
    case 'table':
      return [table(token as Tokens.Table)]
    case 'hr':
      return [h('hr', { class: 'my-4 border-default' })]
    case 'html':
      return [h('pre', { class: PRE, tabindex: 0 }, h('code', (token as Tokens.HTML).text.trimEnd()))]
    case 'text': {
      // Loose text at the level of blocks: a tight list item's.
      const text = token as Tokens.Text
      return [h('span', text.tokens ? inline(text.tokens) : text.text)]
    }
    default:
      // A token no extension here makes: what was written, as text.
      return [h('p', { class: 'my-2' }, token.raw)]
  }
}

function list(token: Tokens.List, outline: Outline): VNode {
  const items = token.items.map(item =>
    h('li', [
      ...(item.task ? [h('code', { class: CODE }, item.checked ? '[x]' : '[ ]'), ' '] : []),
      ...blocks(item.tokens.filter(each => each.type !== 'checkbox'), outline)
    ])
  )
  if (token.ordered) {
    const start = typeof token.start === 'number' && token.start !== 1 ? { start: token.start } : {}
    return h('ol', { class: 'my-2 list-decimal ps-6', ...start }, items)
  }
  return h('ul', { class: 'my-2 list-disc ps-6' }, items)
}

/** A cell's alignment, by a class of the panel's rather than a style. */
const ALIGNED: Record<string, string> = { left: 'text-left', center: 'text-center', right: 'text-right' }

function cell(tag: 'th' | 'td', token: Tokens.TableCell): VNode {
  return h(
    tag,
    { class: ['border border-default px-2 py-1', ALIGNED[token.align ?? 'left']], ...(tag === 'th' ? { scope: 'col' } : {}) },
    inline(token.tokens)
  )
}

function table(token: Tokens.Table): VNode {
  return h('div', { 'class': 'my-2 overflow-x-auto', 'tabindex': 0, 'role': 'region', 'aria-label': 'A table in the document' }, [
    h('table', { class: 'text-sm' }, [
      h('thead', { class: 'bg-elevated/40' }, [h('tr', token.header.map(each => cell('th', each)))]),
      h('tbody', token.rows.map(row => h('tr', row.map(each => cell('td', each)))))
    ])
  ])
}

function inline(tokens: Token[] | undefined): VNodeChild[] {
  return (tokens ?? []).map(span)
}

function span(token: Token): VNodeChild {
  switch (token.type) {
    case 'text': {
      const text = token as Tokens.Text
      return text.tokens ? inline(text.tokens) : text.text
    }
    case 'escape':
      return (token as Tokens.Escape).text
    case 'strong':
      return h('strong', { class: 'font-semibold text-highlighted' }, inline((token as Tokens.Strong).tokens))
    case 'em':
      return h('em', inline((token as Tokens.Em).tokens))
    case 'del':
      return h('del', inline((token as Tokens.Del).tokens))
    case 'codespan':
      return h('code', { class: CODE }, (token as Tokens.Codespan).text)
    case 'br':
      return h('br')
    case 'link':
      return link(token as Tokens.Link)
    case 'image': {
      const image = token as Tokens.Image
      return h('span', { 'class': 'italic text-muted', 'data-image': '' }, `[image: ${image.text || 'no description'}]`)
    }
    case 'html':
      return h('code', { class: CODE }, (token as Tokens.Tag).text)
    default:
      return token.raw
  }
}

/** A link, kept only to another site or a mail; otherwise its text. */
function link(token: Tokens.Link): VNode {
  const href = opened(token.href)
  if (href === undefined) {
    return h('span', { 'data-unlinked': '' }, inline(token.tokens))
  }
  return h(
    'a',
    { href, title: href, target: '_blank', rel: 'noopener noreferrer nofollow', class: 'text-primary underline underline-offset-2' },
    inline(token.tokens)
  )
}

/** `href` as an absolute address with a protocol in `OPEN`, or none. */
function opened(href: string): string | undefined {
  try {
    const url = new URL(href)
    return OPEN.has(url.protocol) ? url.href : undefined
  } catch {
    // Relative, which only a file of the skill's own or a part of this
    // page could be.
    return undefined
  }
}

const CODE = 'rounded bg-muted px-1 py-0.5 font-mono text-[0.85em]'

const PRE = 'my-2 overflow-x-auto rounded-md border border-default bg-muted p-3 font-mono text-xs'

const HEADING: Record<number, string> = {
  3: 'mt-4 mb-2 text-base font-semibold text-highlighted',
  4: 'mt-3 mb-1.5 text-sm font-semibold text-highlighted',
  5: 'mt-3 mb-1 text-sm font-medium text-highlighted',
  6: 'mt-2 mb-1 text-xs font-medium uppercase text-muted'
}
