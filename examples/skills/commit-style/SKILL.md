---
name: commit-style
description: 'Writes commit messages in the form "<area>: <imperative summary>"'
---

# Commit style

A commit message opens with one line, the subject:

```
<area>: <imperative summary>
```

- **The area** is the part of the project the change is in, in lowercase: an
  app, a crate, a package, or `docs`, `build`, `ci` or `chore` for what is
  none of those. Use the areas the project already uses: AGENTS.md or
  CONTRIBUTING may list them, and `git log --format=%s -30` shows them. A
  change that has to span two names both, as `core, cli:`.
- **The summary** says what the change does for someone using or reading the
  project, not which files it touched: `cli: refuse a name with two hyphens
  in a row`, not `cli: update parse.rs`.
- **Imperative**, completing "this commit will…": `add`, `fix`, `refuse`,
  never `added` or `adds`. Lowercase after the colon, no full stop at the
  end, and the whole line within about 72 characters.

## The body

When the subject cannot say why, leave a blank line and explain it, wrapped
at 72 columns: what was wrong, the choice made, and the alternative turned
down. Leave it out when the subject says everything. Trailers come last,
after another blank line, and only the ones the project asks for.

## One step per commit

Each commit is one coherent step that builds and passes the tests on its
own: a refactor, then a new type with its tests, then the wiring, then the
docs. A summary that needs "and" is usually two commits. Stage files by
path rather than everything at once, so nothing unrelated rides along.

Before committing, read the subject against the rules above.
`references/examples.md` has subjects rewritten from what is usually
written to what these rules ask for, for when one is hard to get right.
