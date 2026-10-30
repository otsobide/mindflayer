import { afterEach, describe, expect, it } from 'vitest'
import { instant, when } from '@/time'
import { fixture, render, texts, unmount } from './panel'

afterEach(unmount)

describe('the sources', () => {
  it('lists each source by its address, with the origin --from takes for what it supplied', async () => {
    const snapshot = fixture('workspace')
    const [source] = snapshot.sources

    const { page } = await render('/sources', snapshot)

    const card = page.get('[data-testid="source"]')
    expect(card.get('[data-testid="kind"]').text()).toBe(source!.kind)
    expect(card.get('[data-testid="url"]').text()).toBe(source!.url)
    expect(card.get('[data-testid="origin"]').text()).toBe(source!.origin)
    expect(card.get('[data-testid="subdirectory"]').text()).toBe(source!.subdirectory)
    expect(card.get('[data-testid="directory"]').text()).toBe(source!.directory)
    expect(card.get('[data-testid="revision"] code').attributes('title')).toBe(source!.revision)
    expect(card.get('[data-testid="reference"]').text()).toBe('whatever the remote\'s HEAD points at')
  })

  it('says the branch asked for, and the origin core spells with it', async () => {
    const snapshot = fixture('workspace')
    Object.assign(snapshot.sources[0]!, { reference: 'main', origin: 'https://github.com/acme/skills#main' })

    const { page } = await render('/sources', snapshot)

    expect(page.get('[data-testid="reference"]').text()).toBe('main')
    expect(page.get('[data-testid="origin"]').text()).toBe('https://github.com/acme/skills#main')
  })

  it('names what each supplied of the shelf, and says when it supplied nothing still there', async () => {
    const snapshot = fixture('workspace')
    snapshot.sources.push({ ...snapshot.sources[0]!, url: 'https://github.com/acme/rules', origin: 'https://github.com/acme/rules', directory: 'github.com-acme-rules' })

    const { page } = await render('/sources', snapshot)

    const [skills, rules] = page.findAll('[data-testid="source"]')
    expect(texts(skills!, '[data-testid="shelved"]')).toEqual(snapshot.shelf.map(shelved => `${shelved.kind}:${shelved.name}`))
    expect(rules!.get('[data-testid="supplied"]').text()).toBe('nothing on the shelf now')
  })

  it('names the gather that would add one while there is none', async () => {
    const snapshot = fixture('unmade')

    const { page } = await render('/sources', snapshot)

    expect(texts(page.get('[data-testid="empty"]'), 'code')).toEqual([
      `$ flayer init ${snapshot.workspace.path}`,
      `$ flayer -W ${snapshot.workspace.path} gather git <URL>`
    ])
  })

  it('lights the sources in the sidebar', async () => {
    const { page } = await render('/sources', fixture('workspace'))

    expect(page.get('[data-testid="sidebar"] nav a[aria-current="page"]').text()).toBe('Sources')
  })
})

describe('what happened lately', () => {
  it('lists the ledger\'s latest actions, newest first, as the snapshot holds them', async () => {
    const snapshot = fixture('workspace')

    const { page } = await render('/sources', snapshot)

    const entries = page.findAll('[data-testid="activity"] [data-testid="entry"]')
    expect(entries).toHaveLength(snapshot.history.length)
    entries.forEach((entry, index) => {
      const logged = snapshot.history[index]!
      expect(entry.get('[data-testid="action"]').text()).toBe(logged.action)
      expect(entry.get('[data-testid="target"]').text()).toBe(logged.target)
      expect(entry.get('[data-testid="outcome"]').text()).toBe(logged.outcome)
      expect(entry.get('[data-testid="detail"]').text()).toBe(logged.detail)
      expect(entry.get('[data-testid="at"]').text()).toBe(when(logged.at))
      expect(entry.get('[data-testid="at"]').attributes('datetime')).toBe(instant(logged.at))
    })
  })

  it('says a failure in words, and an outcome it has no look for by its word', async () => {
    const snapshot = fixture('workspace')
    snapshot.history = [
      { at: 1792850800, action: 'gather', target: 'https://github.com/acme/private', outcome: 'failed', detail: 'https://github.com/acme/private: no credential for it' },
      { at: 1792850790, action: 'adopt', target: null, outcome: 'skipped', detail: null }
    ]

    const { page } = await render('/sources', snapshot)

    expect(texts(page, '[data-testid="outcome"]')).toEqual(['failed', 'skipped'])
    const [, adopt] = page.findAll('[data-testid="entry"]')
    expect(adopt!.find('[data-testid="target"]').exists()).toBe(false)
    expect(adopt!.find('[data-testid="detail"]').exists()).toBe(false)
  })

  it('says when the ledger has logged nothing', async () => {
    const { page } = await render('/sources', fixture('unmade'))

    expect(page.get('[data-testid="no-activity"]').text()).toBe('The ledger has logged nothing yet.')
  })
})
