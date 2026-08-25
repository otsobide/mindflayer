# Architecture

## The shape of the tree

Every unit of the product is an app under `apps/`, with its own `Cargo.toml`
and its own `Makefile`. The root `Makefile` delegates (`make <app>/<target>`)
and CI invokes those targets, so adding a front end is adding a directory and
one line to `APPS`, never reshaping the tree.

```
apps/core   mindflayer-core — the engine
apps/cli    mindflayer-cli  — the `mind` binary
```

Only two today. The layout exists because there will be more: a desktop app is
the expected next front end, and it has to sit beside the CLI rather than
around it.

## Where the line between core and a front end falls

`mindflayer-core` knows what a mind project is, what a flayer workspace is, and
what a skill is. It reads and writes files, and that is the only I/O it does.
It has no idea a terminal exists.

`mindflayer-cli` parses a command line, asks core for values, and renders them.
It holds no rules about skills: not the name limits, not the front matter
format, not where `.mind` lives.

The line is there so that when a second front end arrives, it cannot disagree
with the first about what a valid skill is. Every rule that could drift lives
in one crate, and the front ends only choose how to show its answers.

Two consequences worth stating, because they are what the split buys:

- **Sorting happens in core**, not in the renderer. Two front ends listing the
  same projects produce the same order.
- **Rendering happens in the front end**, and the CLI builds its output into a
  string before printing it, which is what lets its tests assert on the exact
  text a user sees.

## The two levels

A **mind project** is a directory carrying `.mind`, the way a repository
carries `.git`. Its skills live in `.mind/skills/<name>/SKILL.md`. It is meant
to be committed: the skills travel with the code they describe.

A **flayer workspace** carries `.mindflayer` and references the mind projects
it manages, so their skills can be handled together. It is the level above, and
it does not have to be a repository at all — the directory your repos happen to
sit in is the usual case.

Both are identified by a **marker file** (`mind.toml`, `flayer.toml`), not by
the directory alone. An empty `.mind` left behind by a failed copy is not a
project, and saying so costs one `is_file()`.

Both are found by walking up from a starting directory, so `mind list` works from
anywhere inside a project, like every git command.

Nothing stops one directory from being both.

### Why the markers are TOML written from a template

They are read with serde and written from a string template, rather than
serialized. Serializing would drop the comments, and the comment in
`mind.toml` explaining where skills go is the first documentation anyone
opening the file will read.

Both carry a `version`. A marker written by a newer Mindflayer is refused with
a message that says so, which is what lets the format change later without an
old binary silently misreading a newer project.

## Failures are collected, not raised

Discovery returns what it found *and* what it could not read. One skill with
broken front matter must not hide the forty next to it, and a stale entry in a
workspace registry must not stop the other projects from being managed.

The CLI prints those on stderr as warnings and exits non-zero: visible, but not
in the way of the answer.

The distinction the code draws is between a problem and an absence. A project
with no `.mind/skills` directory yet is not a failure, it is a project nobody
has added a skill to. A `.mind/skills` that exists and cannot be listed is a
failure.

## Open questions

- **Precedence between projects.** Two projects in one workspace can declare
  the same skill name. Core returns both matches and says nothing about which
  wins, because nothing here yet has to choose. Whatever resolves it (a
  workspace-level override, an explicit order in `flayer.toml`) belongs in core
  when it exists, not in a front end.
- **Registering projects.** `flayer.toml` carries a `projects` list that is
  read but not yet written by any command; a workspace starts empty and the
  list is edited by hand. The command that adds to it is the next piece of
  work, and it is the point where writing the marker file stops being a
  template and needs to preserve comments.
