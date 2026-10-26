# Commit subjects, rewritten

Each pair is a subject as it is often written, then as the commit-style rules
ask for it, and what changed.

| Instead of | Write | Because |
|---|---|---|
| `Fixed bug` | `core: keep a rule's comments when renaming it` | it names the area and says what now works |
| `cli: Added --home option.` | `cli: take --home to say where the default workspace lives` | imperative, lowercase after the colon, no full stop, and why the option exists |
| `update parse.rs` | `cli: refuse a skill name with two hyphens in a row` | what a user sees, not which file moved |
| `docs: README` | `docs: show the install screen's keys in the README` | a summary says what is new, not where |
| `core and cli: add load, fix list, tidy tests` | three commits, one per step | a summary that needs "and" is several commits |
| `WIP` | whatever the step does, or folded into the commit it belongs to | a subject is read long after the work |

## With a body

```
api: retry a webhook once before reporting it failed

Most failed deliveries were one dropped connection, and reporting each
of those woke somebody up for nothing. One retry clears them. Retrying
until it works was turned down: it would hide an endpoint that is really
down, so a second failure is still reported.
```

The subject says what the commit does; the body says why it is done this
way, and what was turned down.
