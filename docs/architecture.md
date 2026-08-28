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
what each kind of artifact is. It reads and writes files, and that is the only I/O it does.
It has no idea a terminal exists.

`mindflayer-cli` parses a command line, asks core for values, and renders them.
It holds no rules about artifacts: not the name limits, not the front matter
format, not which folder a kind lives in.

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

## The kinds, and how a kind is described

An artifact is a skill or a rule, and the difference is a **payload**, not a
field:

```rust
pub enum Declared {
    Skill(SkillManifest),
    Rule,
}
```

`Declared` *is* the kind, so an artifact cannot carry a discriminant that
disagrees with what was parsed — there is only one. A rule that declares a
description is not a state the type can hold, and no `Option` field sits
permanently `None` for one of them.

Where a kind lives and what shape it has is a second, smaller thing:

```rust
pub enum Layout {
    /// One directory per artifact, holding a manifest with a fixed name.
    Directory { manifest: &'static str },   // skills
    /// One file per artifact, at any depth.
    Files { extension: &'static str },      // rules
}
```

Discovery matches on `Layout` and nothing else has to know the difference. The
two shapes decide the nesting rule between them: a skill's directory belongs to
the skill, assets and all, so walking into it would turn its own files into
artifacts — skills are therefore flat. A rule is a loose file, so folders under
`rules/` are free to group, and a rule's name is its **route** without the
extension. `git/no-force-push` and `ci/no-force-push` are two rules; the stem
alone would make them one name for two files.

`Kind` is a closed enum. Every kind ships in this crate, so every `match` is
exhaustive and the compiler is the checklist for adding the next one. A
registry that took kinds at runtime would trade that for an extensibility
nobody has asked for.

### Why a qualifier uses a colon

`skill:commit-style`, not `skill/commit-style`. A rule's name *is* a route, so
the two namespaces would otherwise share a delimiter: `rules/skills/naming.md`
is the rule `skills/naming`, and with a slash qualifier that string would parse
as "the skill `naming`" — a listing printing a name its own `show` rejects, or
worse, resolves to a different artifact. A rules folder grouping rules *about
writing skills* is not an exotic thing to have.

A colon is not a path separator anywhere, and on Windows a filename cannot
contain one at all, so the collision is gone rather than narrowed.

### Where a name comes from

From wherever it is declared. A skill declares one in its front matter, so that
is its name and disagreeing with its directory is a problem `validate` reports.
A rule declares nowhere, so its name is its route. This is why the two are not
symmetric, and the asymmetry is the honest one.

### What a listing shows for something that declares nothing

A skill has a `description`. A rule has the file. Its **opening line** — the
first line carrying any text, with leading `#` stripped — is the closest thing
to a description it has, and it is captured when the file is loaded, because
the file was read anyway.

That one value does double duty: it is the summary a listing prints, and its
absence is the single thing `validate` can say about a rule, since a file with
no line of text has nothing to offer an agent. Storing the fact rather than the
verdict is what keeps `validate` pure — every check it makes was decided when
the file was read, so asking whether something is valid cannot itself fail.

### Adding a third kind

A variant on `Kind`, its folder and layout, a variant on `Declared`, a
constructor on `Artifact`, and a match arm in discovery. Reports need nothing:
columns, labels and counts are all driven from `Kind::ALL` and from what the
catalog actually found.

## The two levels

A **mind project** is a directory carrying `.mind`, the way a repository
carries `.git`. Its skills live in `skills/<name>/SKILL.md` by default. It is
meant to be committed: the skills travel with the code they describe.

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

- **What is in scope.** `mind list` is the project's own artifacts, never its
  neighbours'. `flayer list` is every registered project. Before the split one
  command tried to be both and its answer changed depending on which directory
  it was run from.
- **How an artifact is labelled.** Only the workspace level qualifies one by
  the project it came from, because inside a single project that says nothing.
  The same rule governs the kind: `mind list` grows a kind column, and
  `validate` starts printing `rule/x` instead of `x`, only once more than one
  kind is in play. One rule, applied twice, so a report never spends a column
  on something the reader could not have been confused about.

A workspace is in scope for exactly the projects it was **told** to manage. A
project that merely sits inside the workspace directory is not one of them
until it is linked. Guessing from the directory tree would make `flayer list`
answer a different question after an unrelated `mkdir`.

### Where a project keeps its artifacts

`.mind` is the marker and the configuration. The artifacts themselves sit
beside the code, in directories the marker names:

```toml
[directories]
skills = "skills"
rules = "rules"
```

Beside the code rather than inside `.mind`, because these are files the
**agents** read, and an agent looking for skills does not know what a `.mind`
is. Every project already has a place its agents look — `.claude/skills`,
`docs/rules` — and the marker is how a project says which, so Mindflayer
follows the repository rather than the repository following Mindflayer.

The table is keyed by the kind's folder name, which is the plural spelling the
CLI already accepts, so there is no second table to keep in step. A key this
build does not recognise is carried along rather than rejected, for the reason
unknown front matter keys are.

`init` writes every kind's directory even when it is the default. The answer to
"where do this project's skills go" then lives in the file somebody opens
rather than in a function they have to find.

A directory has to be inside the project. An absolute path, or one that climbs
out with `..`, describes a project whose artifacts are not the project's, and
that contradicts the one thing a mind project is for. `mind init --skills
/etc/skills` is refused rather than made, before anything is written.

Because this changed where a project's artifacts are, `FORMAT_VERSION` is 2 and
a marker written before it is read the way it was written: version 1 had no
such question, so every kind lived inside `.mind`, and reading one with today's
default would point it at directories it never had and list nothing without
saying why. `DIRECTORIES_VERSION` is what draws that line.

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

## Gathering

A workspace has a **shelf**: artifacts collected from somewhere else, held by
the workspace and belonging to no project yet.

```
.mindflayer/
  flayer.toml
  mindflayer.db
  cache/<source>/            the clone
  skills/<source>/<name>/    what was taken out of it
```

Gathering fills that shelf and stops. Nothing is written into a mind project,
because which of the gathered skills a project should carry is a separate
decision made by somebody, and a `git clone` that edited repositories on the
way past would be the wrong kind of convenient. It also keeps the two halves
independently testable: what a source yields does not depend on what any
project wants.

### Everything is namespaced by its source

Two repositories may both offer `commit-style`, and both are worth having:
choosing between them is what the shelf exists to make possible. So each source
owns a folder, named after its URL and recorded once, and the same name from a
different source is a different thing rather than a collision. This is the same
answer `Catalog::find` gives inside a project — return both, decide nothing —
one level up.

The folder a skill lands in keeps the name the **source** gave it, not the name
the skill declares. When those disagree that is something `validate` reports;
renaming the folder on the way in would repair the symptom and hide it.

### The clone is kept, and re-made

The clone stays under `cache/` so a gather can be looked at afterwards, and
because what a source actually contained outlives the report about it.

It is re-cloned rather than fetched into. The clone is shallow, so a re-clone
costs about what a fetch would, and it is one code path instead of three:
clone, fetch, and reconcile a checkout somebody may have edited. Gathering is
not something anybody runs in a loop.

Placing an artifact **replaces** its folder rather than merging into it, so a
file the source deleted does not survive as a leftover of an older revision.
An artifact that has not changed is left alone entirely, down to its
modification times, which is what lets a second gather answer the only question
worth asking the second time: what moved.

Failures are collected, as everywhere else: one skill with broken front matter
is a warning beside the forty that came through, not a reason to be told
nothing.

### Why `gix` rather than running `git`

Nothing depends on a `git` being installed, or on which `git` is first on the
PATH, and core keeps doing its own I/O rather than supervising a process. The
cost is that authentication for private repositories is ours to solve rather
than inherited from a credential helper, and it is not solved yet.

`gix`'s error types are large and would become part of this crate's public API
if they were carried, tying its version to gix's, so `GitError` keeps what they
said and not what they were.

### Why the ledger is SQLite

Everything else Mindflayer writes is a file a person opens: TOML with comments
explaining itself. `mindflayer.db` is not, and the exception is deliberate. It
answers questions a file cannot — which of two identically named skills came
from which repository, at which revision, and what happened the last four times
a gather ran — and answering them from a flat file would mean writing a query
engine badly.

It sits in `.mindflayer/`, beside the marker it belongs to, so it travels with
the workspace and two workspaces never share a history. It carries its schema
version in SQLite's own `user_version` header field, and a database from the
future is refused exactly as a marker file from the future is.

Three things are recorded: the **sources** gathered from, the **artifacts** on
the shelf and which source each came from, and an **action log**. The log keeps
failures too — a log of only what worked cannot answer the question anybody
opens it with. Timestamps are Unix seconds, which needs no date library and
which SQLite renders with `datetime(at, 'unixepoch')`.

A source's shelf folder is stored rather than derived: two URLs can reduce to
the same readable name, and where a source's files went is a fact about the
past that must not move when the naming rule changes.

## Installing

Gathering fills the shelf; installing is the other half, and the only thing in
Mindflayer that writes into a mind project. A skill is copied into the
directory that project's marker names, which is the whole point of that marker
carrying one.

### It only manages what it installed

The ledger records every installation against `(project, kind, name)`, and that
record is what separates a file this tool put there from one somebody wrote. A
skill present but unrecorded is `Standing::Foreign`, and Foreign is inert in
both directions: never overwritten, never deleted, and the caller is told so
rather than obeyed quietly.

Without that, an install screen is a thing that can delete a colleague's work
because a checkbox looked untidy. The rule costs one query and removes the
whole class.

A project has one directory per artifact name, so two shelf entries offering
`commit-style` cannot both be installed. The screen settles it by unticking the
other rather than letting both apply and the second win — a conflict resolved
where somebody can see it happening.

### The screen, and why it is three files

`flayer install` is a two-column screen: projects on the left, the shelf as
seen from the project under the cursor on the right. It is split so that only
one of its three parts needs a terminal:

- `install/state.rs` — what a key does. Marking a box is a statement of intent
  and touches nothing; it is a plain state machine a test drives by pressing
  the keys a person would.
- `install/ui.rs` — what the screen looks like. Rendered into an in-memory
  terminal by the tests, so the exact text a user sees is asserted on, the same
  way every other command's output is.
- `install.rs` — the loop that reads a real keyboard, and the batch that
  carries out what was marked. The loop is the only part with no test, and it
  holds nothing but the loop for that reason.

Everything marked is applied in one batch, and what that batch did is printed
afterwards as an ordinary `Outcome` — so a report from the screen reads like a
report from any other command, warnings on stderr and a non-zero exit when
something was left alone.

`try_init` rather than `init`: this is the one command that needs a terminal,
and being run without one deserves a sentence rather than a panic.

## Open questions

- **Precedence between projects.** Two projects in one workspace can declare
  the same skill name. Core returns both matches and says nothing about which
  wins, because nothing here yet has to choose. Whatever resolves it (a
  workspace-level override, an explicit order in `flayer.toml`) belongs in core
  when it exists, not in a front end.
- **Installing without a screen.** `flayer install` is interactive only, so it
  cannot run in CI, from a script, or under another agent. The batch it applies
  is already a plain function over a plan; what is missing is a way to say that
  plan on a command line.
- **Creating artifacts.** Nothing writes a `SKILL.md` or a rule from scratch;
  they are added by hand or gathered. `mind add <kind> <name>` is where the
  kinds stop sharing a code path: a skill needs a manifest scaffolded and a
  rule needs an empty file.
- **Gathering rules.** Only skills are gatherable. A rule is a loose file at any
  depth, so harvesting one means deciding what its name is relative to — the
  same question `Catalog::take_files` answers inside a project, asked of a
  folder that is not one.
- **Private repositories.** `gix` is given no credentials, so a private source
  fails at the fetch. What it should be given — an ssh agent, a token from the
  environment, the platform keychain — is undecided.
- **Rules that want to declare something.** Today a rule has no front matter,
  so a leading `---` in one is content. If rules later want metadata, that
  becomes a breaking reading of files written now. The escape hatch is cheap
  and deliberate: `Declared::Rule` is a variant, so giving it a payload is a
  local change.
