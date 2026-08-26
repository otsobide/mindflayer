# Architecture

## The shape of the tree

Every unit of the product is an app under `apps/`, with its own `Cargo.toml`
and its own `Makefile`. The root `Makefile` delegates (`make <app>/<target>`)
and CI invokes those targets, so adding a front end is adding a directory and
one line to `APPS`, never reshaping the tree.

```
apps/core   mindflayer-core — the engine
apps/cli    mindflayer-cli  — the `mind` and `flayer` binaries
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

It ships **two binaries**, `mind` and `flayer`, and they are two entry points
into one parser rather than one wrapping the other. `flayer <cmd>` and `mind
flayer <cmd>` reach the same function, so there is a single implementation of
every workspace command and the two spellings cannot drift apart or disagree
about an exit code. A wrapper that re-executed `mind` would also have to
forward arguments, stdio and exit codes correctly, which is three chances to
get it wrong in exchange for nothing.

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

### The command surface is split the same way

`mind <cmd>` acts on the project you are standing in; `mind flayer <cmd>`, and
therefore `flayer <cmd>`, acts on the workspace above it. The split decides two
things that used to be guesses:

- **What is in scope.** `mind list` is the project's own skills, never its
  neighbours'. `flayer list` is every registered project. Before the split one
  command tried to be both and its answer changed depending on which directory
  it was run from.
- **How a skill is labelled.** Only the workspace level qualifies a skill by
  the project it came from, because inside a single project that says nothing.

A workspace is in scope for exactly the projects it was **told** to manage. A
project that merely sits inside the workspace directory is not one of them
until it is linked. Guessing from the directory tree would make `flayer list`
answer a different question after an unrelated `mkdir`.

### Why the markers are TOML written from a template

New markers are written from a string template, not serialized. Serializing
would drop the comments, and the comment in `mind.toml` explaining where skills
go is the first documentation anyone opening the file will read.

Editing an existing marker has the same constraint and a harder job, which is
why `toml_edit` is a dependency: `flayer link` rewrites the one `projects`
array and leaves every other byte alone. Reading stays serde's job. The two
halves agree because an edit re-reads the file afterwards rather than trusting
what it believes it wrote, and the write itself goes through a temporary file
and a rename, so a crash leaves either the old config or the new one.

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

## How entries are stored

`flayer link` records a project as a route **relative to the workspace**, so
the workspace and the projects under it can be moved together without the
registry going stale. Entries are written with forward slashes, so a workspace
registered on one platform still resolves on the other, and a command reports
the entry the way the file spells it rather than the way the local separator
would.

The arithmetic is lexical (`apps/core/src/paths.rs`): it never touches the disk
and never resolves symlinks. That keeps a path in the shape the user typed and
lets a route be computed for a directory that need not exist.

But arithmetic on paths is only true when no component is a symlink. If the
workspace root is spelled through one, a `..` climbs out of the link's target
rather than out of the directory the name suggests, and the route points
somewhere that does not exist — `/tmp` is a symlink to `/private/tmp` on every
Mac, so this is not exotic. So `link` **checks its route against the
filesystem** before storing it, and falls back to an absolute path when it does
not land where it should. The check happens there and only there: its answer
decides which spelling to store and is never stored itself, so entries stay
portable rather than frozen to one machine's symlink layout.

Matching is by where an entry **points**, not by how it is spelled, so
`collapse` and `./collapse` are one entry. Arithmetic settles most of it, and
two spellings only the filesystem can equate are settled by canonicalising.

`link` and `unlink` match against **the array they are about to edit**, not
against the copy parsed when the workspace was opened. The marker file is meant
to be editable by hand, so that copy can be stale, and acting on a stale index
is how you remove a project nobody asked you to remove. An edit that changes
nothing does not rewrite the file at all.

`link` is idempotent and `unlink` is not, deliberately. Linking twice means
"make sure this is registered", and it is; the call reports the spelling
already in the file rather than the one it would have written. Unlinking
something that was never there is a typo far more often than a no-op, and
saying so turns a silent success into a fixable mistake. `unlink` removes
*every* entry pointing at the project, because two spellings of one directory
are one project and removing half of them while reporting success would leave
it registered. It takes a path rather than a project because the entry most
worth removing is one whose directory has moved away, and that cannot be opened
as a project any more.

Rewrites go through a temporary file and a rename, so a crash leaves either the
old config or the new one. The original's permissions are copied onto the
temporary first, and a symlinked config is followed to the file it names —
otherwise an edit would quietly widen a chmodded config or detach a shared one.
A hard link is the case this cannot preserve: a rename breaks it, and keeping
it would mean giving up the atomic write.

## Open questions

- **Precedence between projects.** Two projects in one workspace can declare
  the same skill name. Core returns both matches and says nothing about which
  wins, because nothing here yet has to choose. Whatever resolves it (a
  workspace-level override, an explicit order in `flayer.toml`) belongs in core
  when it exists, not in a front end.
- **Creating skills.** Nothing writes a `SKILL.md` yet; they are added by hand.
  A command that scaffolds one is the next piece of work, and the validation
  rules it should satisfy already live in `skill.rs`.
