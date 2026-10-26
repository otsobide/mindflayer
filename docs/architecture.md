# Architecture

## The shape of the tree

Every unit of the product is an app under `apps/`, with its own `Cargo.toml`
and its own `Makefile`. The root `Makefile` delegates (`make <app>/<target>`)
and CI invokes those targets, so adding a front end is adding a directory, a
line in `APPS` and the workspace `members`, and its test and build jobs in
CI — never reshaping the tree.

```
apps/core   mindflayer-core — the engine
apps/cli    mindflayer-cli  — the `mind` and `flayer` binaries
```

Only two today. The layout exists because there will be more: the next is a
local server and the web panel it serves
([D-17](#d-17-the-next-front-end-is-a-local-web-panel)), and they have to sit
beside the CLI rather than around it.

## Where the line between core and a front end falls

`mindflayer-core` knows what a mind project is, what a flayer workspace is, and
what each kind of artifact is. It reads and writes files, and the only other
I/O it does is fetching a repository through `gix` when asked to gather. It has
no idea a terminal exists.

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

The TUI is in the same crate, behind the same parser, rather than a front end
of its own: it does everything by running these commands. See [the
TUI](#the-tui).

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
constructor on `Artifact`, and an arm in every match on the kind — discovery,
`create`, `lifecycle::load` and `lifecycle::rename`, and the refusal in
`gather` — which the compiler lists, since the matches are exhaustive. Reports
need nothing:
columns, labels and counts are all driven from `Kind::ALL` and from what the
catalog actually found.

## The two levels

A **mind project** is a directory carrying `.mind`, the way a repository
carries `.git`. Its skills live in `skills/<name>/SKILL.md` by default. It is
meant to be committed: the skills travel with the code they describe.

A **flayer workspace** carries `.mindflayer` and references the mind projects
it manages, so their skills can be handled together. It is the level above, and
it does not have to be a repository at all — the directory your repos happen to
sit in is the usual case. It also holds skills and rules of its own: the ones
that belong to all of its projects, written once.

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
  neighbours'. `flayer list` is the workspace's own and every registered
  project's. Before the split one
  command tried to be both and its answer changed depending on which directory
  it was run from.
- **How an artifact is labelled.** Only the workspace level qualifies one by
  what holds it — a project, or `workspace` for its own — because inside a
  single project that says nothing.
  The same rule governs the kind: `mind list` grows a kind column, and
  `validate` starts printing `rule:x` instead of `x`, only once more than one
  kind is in play. One rule, applied twice, so a report never spends a column
  on something the reader could not have been confused about.

A workspace is in scope for exactly the projects it was **told** to manage. A
project that merely sits inside the workspace directory is not one of them
until it is linked. Guessing from the directory tree would make `flayer list`
answer a different question after an unrelated `mkdir`.

### Every command, at both levels

What can be done to a project can be done from the workspace, and the other
way round where it means anything. `flayer add`, `edit`, `rename`, `remove` and
`templates` act on the workspace's own artifacts, or on one of its projects
with `-p`; `mind link`, `unlink`, `install` and `uninstall` act on the project
you are in, against the workspace above it. `flayer list`, `show` and
`validate` read everything — the workspace's own and every project's — or one
project with `-p`. Reads look everywhere; a change needs one place, and
without `-p` that place is the workspace.

There is one implementation of each. `mind add` and `flayer add -p alpha` are
the same function given the same project, and `flayer add` is that function
given the workspace's own holder, so the levels differ in what they hold and
never in the rules. What only has meaning at one level — the shelf, `gather`,
`scan` — is reachable from the other as `mind flayer <cmd>`, which is the
command itself rather than a copy of it.

### The workspace's own artifacts

They are held the way a project holds its own: in directories the marker
names (`[directories]` in `flayer.toml`, `skills` and `rules` beside the
projects by default), found by the same catalog, created, renamed and removed
by the same functions. `FlayerWorkspace::own` hands the workspace out as a
`MindProject`-shaped holder for exactly that reason — everything that touches an
artifact asks its holder where it is, what it is called and where each kind
lives, and a workspace answers those as a project does — so there is no second
copy of any rule to drift.

Unlike a project's, the folders are not made by `flayer init`. A workspace sits
among other people's repositories, and puts nothing beside them until there is
something to put: the first artifact added makes its folder.

A project holds what is in its own folders, because that is what its agents
read; the workspace's artifacts are not inherited out of sight. They reach a
project by being installed into it — the install screen and `mind install` offer
them beside the shelf — and are then managed there like anything else
Mindflayer installed.

When the workspace's root is itself a project it manages, a folder both keep a
kind in is the project's (`own_kinds`), and a catalog counts each file once
however many holders can see it: otherwise every artifact there would be listed
twice. Reports name a workspace's own artifact's holder `workspace`, told by
where the file sits rather than by the root it was found from, since those two
share a root.

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
with no skills folder yet is not a failure, it is a project nobody has added a
skill to. A skills folder that exists and cannot be listed is a
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
than inherited whole from `git`, and it is not solved yet:
[D-16](#d-16-credentials-come-from-the-front-end-never-from-a-prompt) decides
how.

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

Four things are recorded: the **sources** gathered from, the **artifacts** on
the shelf and which source each came from, the **installations** — which skill
Mindflayer put into which project — and an **action log**. The log keeps
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

### The folder is named twice, on purpose

Gathering keeps the folder name the **source** used. Installing files the copy
under the name the skill **declares**. That looks inconsistent and is not.

On the shelf, renaming would repair a symptom and hide it: a skill whose
directory disagrees with its front matter is something `validate` exists to
report, and the shelf is a record of what a source actually contained. Inside a
project the same mismatch is not a report, it is a skill that lists fine and
never loads — the directory name is how an agent finds what it was told to
invoke. So the copy is filed under the declared name, and the disagreement
stays where it belongs, upstream.

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

### Two places offer, one command takes

What can be installed comes from the workspace's own skills, offered first,
the workspaces it loads (below), and the shelf (`install::Offer`). An installation from the shelf records
the source it came from; one of the workspace's own records none, because it
came from nowhere but the workspace.

`mind install <skill>` and `flayer install -p <project> <skill>` install one
without the screen, through `install::offered`: the one candidate of that name,
or an error naming every place that offers it when there is more than one —
`--from` then says which, because which of two lands in the project's one folder
of that name is not something to guess.

### Loading: a third place, read live

`flayer load <path>` adds a third place that offers: the own artifacts of
another flayer workspace, `install::Offer::Loaded`, between the workspace's own
and the shelf. Its entry sits in a `loaded` array in `flayer.toml`, beside
`projects` and edited the same way (`edit_array`), rather than in the ledger:
it is configuration a person may read and edit, like the registry, and a load
must not create a database a workspace that never gathered does not have.

Three decisions shape it:

- **Live, not copied.** An offer points at the artifact where the loaded
  workspace keeps it, so the next install of it takes whatever is there now.
  That is the difference from gathering, which snapshots a repository onto the
  shelf; a loaded workspace is usually one on the same disk that is still
  being worked on. An installation from it records no source id — like the
  workspace's own — because there is no shelf row to point at.
- **One level deep.** Only a loaded workspace's own artifacts are offered,
  never what it loads, so two workspaces loading each other is not a loop, and
  what a workspace offers is what its marker names, not a graph.
- **Into the workspace that is not the source.** Run from inside the workspace
  being loaded, the first workspace found walking up is that one, and a
  workspace cannot load itself. `FlayerWorkspace::locate_or_default_except`
  walks on past it — and past every path given — to the next one, or the
  default in your home; loading the home workspace from inside it is refused.

A loaded entry's origin is `load:<entry>`: disjoint by construction from
`workspace` and from any shelf source's address — even a repository gathered
from the very directory that is also loaded. What `gather list` prints is what
`--from` takes; `install::resolve_from` also turns the path given to `load`,
typed from anywhere, into that origin.

A loaded workspace is read live and is usually somebody else's, which puts
installing's trust in the name a `SKILL.md` declares under strain. So:

- **A name must be one folder name** (`install::is_folder_name`). It is joined
  onto the project's folder, and replacing starts with deleting: `..` would
  delete the project, an absolute path anything at all. Such an artifact is
  not offered from anywhere, and `install`/`uninstall` refuse it outright.
- **Symlinks are followed only inside where the artifact comes from**
  (`copy::tree`'s `within`): the loaded workspace, the workspace's own root,
  the clone being gathered. One pointing out is refused, not copied, because
  copying it is how a credential ends up in a committed project.
- **Never one folder onto another it contains.** Equal paths are left alone
  and recorded as nothing; one inside the other is refused (`Overlaps`).
- **Never over what a loaded workspace keeps as its own.** A project can be
  the workspace it loads, sharing a folder; a record from before the load
  would otherwise let install overwrite, and uninstall delete, that
  workspace's own work (`KeptByALoad`).
- **A folder by that name is somebody's, finished or not.** Standing counts
  any path there, not only one with a manifest, so a folder of notes started
  by hand is never replaced.

What a load cannot offer — a workspace that has gone, an artifact that cannot
be read, a name that is refused — is collected by `install::offered_by_loads`
and said as `warning:` lines wherever offers are shown or used, and the
install screen reports a row it could not apply and carries on with the rest.
Two spellings of one loaded directory are one source; `unload_all` and
`unlink_all` rewrite the marker once, for all of the paths or none.

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

`try_init` rather than `init`, here and in the TUI: these are the commands that
need a terminal, and being run without one deserves a sentence rather than a
panic.

## The TUI

`flayer` on its own opens a screen over the whole workspace. It is not a
second front end beside the CLI: it lives in the CLI crate, and it **does
everything by running CLI commands**.

It is at both levels too. The workspace's own artifacts are its first row,
marked `◆`, and a key on that row runs the `flayer` command where a key on a
project's row runs `mind -C <project>`: a task carries its holder, and the
holder decides the level of the line it becomes. `mind` on its own opens the
same screen on the project it was run in — inside the workspace that manages
it, where its neighbours and its installs are, or on the project alone when
none does. Alone, the keys that belong to a workspace say so instead of
running, and `l` is `mind link`, after which the screen is the workspace's.

### Every change is a command line

A key that asks for work produces a `Task`, and `Task::commands` turns it into
the words a person would type — `flayer link beta`, `mind -C beta add skill
deploy 'Ship it'`. Those words are parsed by `Cli` and `FlayerCli`, the parsers
the binaries use, and run through `run` and `run_flayer_cli`, the functions the
binaries call. The TUI has no code path of its own for linking, adding or
gathering, and it cannot grow one, because there is nowhere in it to put one.

That is what turns "every feature has a command and a place in the TUI" from a
promise into a property. A feature is written once, as a command, and the TUI
gains it by producing that command's words. A test parses every task's lines —
free text shaped to break a naive command line included — so a task that could
not be typed cannot be written.

Two things follow, and both are deliberate:

- **A form shows what it is about to run, as it is typed**, as the command
  line. The TUI teaches the CLI rather than standing in front of it.
- **Closing the TUI prints a transcript**: every command that changed
  something, with what each said. The session can be read afterwards and
  repeated in a script. Commands that only read — show, validate, the shelf —
  are left out, because they changed nothing.

A `-C` in those words is relative to the workspace root, which is how someone
reading the transcript takes it, so it is resolved against the root rather than
against wherever the process started. A command that opens a screen of its own
— `flayer`, `flayer tui`, `flayer minds`, `flayer install` with no skill named,
`mind`, `mind tui` — is refused if it ever reaches that path, rather than drawn
inside this one.

The exceptions are named, not hidden: the install screen is embedded as it is,
and applying it is the batch `flayer install` applies; re-reading the disk is
not a command, because it changes nothing.

### Split three ways, like the install screen

- `tui/state.rs` — what a key does. It moves cursors, fills forms and returns
  tasks; nothing in it touches a file, so a test drives it with the keys a
  person presses.
- `tui/ui.rs` — what the screen looks like, rendered into an in-memory
  terminal by the tests.
- `tui/minds.rs` — the minds screen's rows and ticks, and the task they amount
  to; like `state.rs`, it touches nothing.
- `tui.rs` — the loop, reading the disk into what the screen shows, and running
  tasks. `step` is everything the loop does between a key and the next frame,
  and it is public so a test can do exactly that without a terminal.

The loop draws a box saying what is running before it runs it, because a
gather is the network and a frozen screen reads as a hung one. Keys pressed
while it ran are dropped: they were aimed at a screen that has since changed.

`mind edit` is the one command that needs the terminal itself, because an
editor draws a screen of its own. For it the loop steps aside — raw mode off,
back to the normal screen — runs the command as it runs any other, and builds
its own screen again from scratch when the editor closes. The task says so
(`takes_terminal`), so the loop does not have to know which commands are
editors.

### What it asks before it writes

The link form offers what `flayer scan` finds under the workspace. A git
repository that is not a mind project yet can be picked, but making it one
writes into it, so `mind init` is asked about, not assumed — and a typed path
that turns out to be a plain directory is asked about the same way. Unlinking
asks too, and says that nothing on disk is deleted. A command that fails from a
form leaves the form open with the reason in it, so a typo is fixed where it
was made rather than typed out again.

### The minds screen

`m`, or `flayer minds`, lists every project the workspace links, ticked, and
every one `flayer scan` finds under it that it does not, unticked — in one
list by path, because the question it answers is "which of these", not "which
kind". Ticking only marks. Applying turns the marks into lines like any other
task: `mind -C <path> init` for each repository that is not a project yet, then
one `flayer link` and one `flayer unlink`, each with every path at once. So the
screen needed `link` and `unlink` to take several paths, and they take them
**all or nothing**: every path is checked before any entry is written, so a
refused one leaves the registry as it was rather than half changed by a
command that reports failure.

## The default workspace

A workspace is found by walking up, and when nothing up there has one, the
workspace-level commands fall back to the one at the default home:
`FlayerWorkspace::locate_or_default` makes `~/.mindflayer` the first time it
is needed. Home sits above most of what a person works on, so it is the one
directory where a workspace can manage projects anywhere, and making it on
demand means `flayer link ~/code/x` works before anyone has chosen a directory.

It is made by any workspace command that finds none up the tree, reading ones
included: `flayer list`, `flayer scan`, `mind install` and opening `flayer`'s
TUI all go through `locate_or_default`, so the first of them run outside every
workspace leaves a `~/.mindflayer` behind. The exceptions are the commands
that only list or take away a load — `flayer load` on its own, `unload` — which
use `locate_except`, and `mind` opening its TUI, which uses `default_at`: both
open the home workspace if it exists and never create it. Whether reading
should create it at all is an [open question](#open-questions).

Where home is comes from the front end, not from core: `--home`, then
`$MINDFLAYER_HOME`, then the user's home directory, and empty means there is
none. Core takes it as a parameter, which keeps it free of the environment and
is what lets the tests run every command with the fallback switched off.

## Creating and changing artifacts

`mind add` writes the least that passes `validate`: a skill's front matter and
a heading, a rule's opening line. The name is checked by
`artifact::name_issues`, the function `validate` itself uses, so a name the tool
creates is one the tool accepts. The front matter is serialized rather than
formatted, because a description is free text, and one containing `: ` would
otherwise be written as YAML that does not parse back into what was typed.

Nothing is written over. A skill's folder is made with `create_dir`, so the
check that it is not already there and the making of it are one step, and a
folder that exists but is not a skill counts as there: it is somebody's. What
was written is read back rather than trusted.

A created artifact is its author's work, so it is not recorded in the ledger.
To `install` it is foreign: never overwritten by a shelf entry of the same
name, never removed by unticking one.

### Templates live inside the markers

`.mind/templates/` and `.mindflayer/templates/`, not beside the code: the
artifacts are beside the code because agents read them, and nothing but
Mindflayer reads a template. A workspace's templates count for the projects it
manages, found by walking up, the rule every workspace-wide thing follows; of
two templates of one kind and name, the closer wins.

A template's front matter keeps whatever it says except `name` and
`description`, which `mind add` always sets: the template's lines for those
two are dropped, continuation lines and all, before anything parses the rest —
so a template may write `name: {{name}}`, which is not YAML anybody could read,
and still work. The result is parsed back before the first file is written, so
a template with broken front matter costs nothing but the error.

### Finding what to change

`edit`, `rename` and `remove` find an artifact by what it declares, and then by
where its name would put it on disk (`lifecycle::find`). The second is what
lets a skill whose front matter does not parse be opened and fixed: it is left
out of every listing, and it is the one most in need of opening. Only a name
that passes the name check is turned into a place, so `../x` cannot reach out
of the project. More than one match is an error: changing the wrong artifact is
not something to guess about. The TUI lists unreadable files in the same way,
by the name their place gives them (`lifecycle::identify`), so they can be
reached with a key rather than only read about in a warning.

### Rename rewrites one line

A skill's name is in two places, its folder and its front matter, and they have
to move together or `validate` fails. The front matter is somebody's writing,
so it is not re-serialized: the one `name:` line is replaced and every other
byte kept (`frontmatter::with_key`). A name spread over several lines is not
touched, and neither is a manifest where the replacement would change anything
but the name — the result is parsed back and compared, field by field, before
anything is written. The folder moves first and the manifest is replaced
through a temporary file; if the second fails, the folder moves back.

### Remove is a dry run until it is told

`mind remove` without `--yes` deletes nothing and says what it would have, the
way `git clean` refuses without `-f`. It is the one command here that cannot be
taken back, so it is asked for twice: once by naming what, once by saying yes.
The TUI asks with a question and then runs the command with `--yes`, so the
transcript shows the yes that was given.

### A record must not outlive the copy it is about

The ledger's record of an installation is what gives the install screen leave
to overwrite and delete. Remove, rename or edit an installed skill and the copy
it was about is gone, or is no longer what was installed; a record left behind
would be inherited by whatever is written there next, and the install screen
would then delete somebody's work because a box looked untidy. So each of them
drops the record (`install::disown`), and `mind add` drops any stale one under
the name it writes. An installed skill that is edited becomes its author's.

## Scanning

`flayer scan` walks down from the workspace root looking for mind projects and
git repositories, and says which of them the registry already points at. It
links nothing — a workspace manages what it was told to — it only makes the
telling easier, which is what the TUI's link form is built on.

The walk is bounded, because a workspace can sit at the top of a home
directory. It goes three levels down (`repo`, `org/repo`,
`github.com/org/repo`), breadth first, so a budget of listed folders runs out
on the deepest rather than on a whole neighbouring tree, and never into hidden
folders, symlinks, `node_modules` or `target`. A repository is a leaf, since its
folders are its own — except the workspace root, which is also where the others
are kept. A scan that runs out of budget says so rather than passing for
complete.

## Decisions

The roadmap in [#15](https://github.com/otsobide/mindflayer/issues/15) rests on
twenty-three decisions: twenty-two validated by the owner on 2026-10-26, and
[D-20](#d-20-opencode-subagents-pending), still pending. They are written down
here so that the PRs carrying them out do not reopen them: a change that
disagrees with one starts by changing the decision, here, with the owner. Each
says what was chosen, why, what was turned down, and which issues it unblocks;
the pending one says instead what holds until the owner chooses.

They describe what is to be built, not what is. Until the issues a decision
unblocks have landed, the sections above still describe the code as it stands,
and each of those issues rewrites the sections it changes.

### D-01: A project's `.mind` ignores itself

`mind init` writes a `.mind/.gitignore` holding `*`, as Python's venv does, and
`mind init --shared` leaves it out for a team that wants its marker committed.
The rule lives inside the folder Mindflayer owns, so it needs no git repository
at init time, survives a copy and goes when `.mind` does. Rejected: keeping
`.mind` committed, as today, against the owner's ask for an ignored one; an
anchored `/.mind/` line in `$GIT_COMMON_DIR/info/exclude`, which stays the
mechanism for installed copies (D-02) because they have no Mindflayer folder to
hold a `.gitignore`; and a line in the repository's `.gitignore`, which edits a
team file. A fresh clone has no `.mind`, and `scan` already offers plain
repositories. Unblocks #29, #35 and #38.

### D-02: Installs stay local unless a project shares them

A project's settings name a share mode, `local` or `shared`. New projects are
local; projects upgraded from v2 are shared, because they were committed by
design. Local mode keeps installed copies and entries out of git through
anchored entries in a Mindflayer block of `info/exclude`, and edits a tracked
`AGENTS.md`, `CLAUDE.md` or MCP config only when the project sets
`edit_tracked`; otherwise Claude gets its rules through ignored
`.claude/rules/mindflayer` files, and the agents left unreached are reported.
That gives quiet projects, keeps the team case one flag away, and changes no
committed team file behind anyone's back. Rejected: always shared, which leaves
every install for the team to commit; and always local with marked edits to
tracked files, which puts Mindflayer's block in a team's diffs. It rests on
agents reading git-ignored files, which none documents and #17 confirms on real
binaries. Local copies never reach teammates or cloud runs, and reach another
worktree only for Claude, and only when that worktree has no `.claude/skills`
of its own. Unblocks #17, #32, #34, #37, #38, #43, #44, #45 and #94.

### D-03: Rules reach every agent through `AGENTS.md`

Assigned rules are rendered between `<!-- mindflayer:begin -->` and
`<!-- mindflayer:end -->` in the project's root `AGENTS.md`, one section per
route. Where Claude would not read that file, a managed `@AGENTS.md` import
goes into the project's own `CLAUDE.md`, or
`.claude/rules/mindflayer/<route>.md` when no file may be edited (D-02); a rule
that declares `paths:` goes only there. One core predicate (#44) decides,
walking every ancestor directory and ignoring the user memory file. The root
`AGENTS.md` is the only per-project file all three agents read, and Codex has
no include mechanism. Claude 2.1.277+ reads it natively only when no
`CLAUDE.md`, `.claude/CLAUDE.md` or `CLAUDE.local.md` sits in the start
directory or above it; the import covers the other case, and Claude before
2.1.277. Rejected: per-agent files, under which Claude reads every rule twice
when there is no `CLAUDE.md`; and OpenCode's `instructions` globs with Codex's
`developer_instructions`, unverified and honoured only in trusted projects.
Mindflayer never creates a `CLAUDE.local.md`, which silently disables
`AGENTS.md`, and warns when an `AGENTS.override.md` hides the file from Codex
or the chain nears Codex's 32 KiB budget. Unblocks #17, #42, #43, #44 and #45.

### D-04: An edited copy is detected, not disowned

The ledger keeps a content hash per target. An untouched copy stays managed and
can be updated; one whose hash differs is Edited, reported, and never
overwritten or deleted without `--force`. `edit` and `rename` keep the record,
and removing an installed copy uninstalls it. A record from before hashes
counts as current only when the copy matches its offer byte for byte, and as
Edited otherwise. This keeps "never overwrite what somebody wrote" while
updates still reach the copies nobody touched, and it catches a hand edit made
outside Mindflayer, which today goes unseen. Rejected: today's rule, which
stops updates at the first edit; overwriting on update, which loses the edit;
and a three-way merge, too complex for skills that ship scripts. Unblocks #23,
#30, #47, #48, #50 and #93.

### D-05: A workspace's library lives in `.mindflayer`

New workspaces keep their own artifacts in `.mindflayer/library/<kind>`.
Existing ones keep the explicit `[directories]` in their `flayer.toml` until
`flayer migrate --library` moves them, and a kind added later (MCP) defaults to
`.mindflayer/library/<kind>` in every workspace, whatever its `flayer.toml`
says. `flayer.toml` already spells its directories out, so the new default
needs no format bump; no agent scans `.mindflayer`, which keeps
instruction-shaped files out of the directories above projects; and a
venv-style flayer becomes one folder. Rejected: beside the projects, as today,
which puts `~/skills` and `~/rules` in the home directory; and moving only the
home workspace's library, which leaves two layouts to explain. Unblocks #52,
#53 and #86.

### D-06: Projects keep authoring beside the code

Amended by the owner. Projects keep authoring their own skills and rules beside
the code, and `mind init` still creates `skills/` and `rules/` by default. The
v3 project marker is a JSON file, `.mind/settings.json`, which names those
folders (`directories`) beside the project's agents, share mode and id, so any
tool or editor can read and change it; `flayer.toml`, and v2 projects, keep the
TOML of [the markers](#why-the-markers-are-toml-written-from-a-template).
Installs go to the agent folders the project targets (D-07), not to the
authored ones. That keeps every existing workflow, makes the workspace the
default source, and keeps the marker minimal. Rejected: receiving only, which
would strand what v2 projects authored; and today's model unchanged, against
vision points 2 and 8 (the flayer as the source, a minimal `.mind`). The plan
made the folders opt-in (`--skills`, `--rules`); the owner prefers them to
exist from the start. Unblocks #29, #33, #35 and #59.

### D-07: One copy per agent folder

Amended by the owner. An installed skill is a real copy in
`.agents/skills/<name>`, which Codex and OpenCode read, and an identical one in
`.claude/skills/<name>` for Claude, staged and written in one step; the ledger
records one installation with several targets. That layout is the default, and
a project's `.mind/settings.json` may override it per kind, for example with
one `.skills` folder for every agent (#40). Copies fit today's install model
and the Windows CI, survive the hub moving, and keep gathered content
contained. OpenCode sees the skill twice, which is harmless only while the
copies are identical, so an install is refused if either target holds foreign
content. Rejected for now: a relative symlink in `.claude/skills`, which may
come later as a Unix opt-in if #17 shows Claude loads a symlinked skill once;
and symlinks into the hub, which break projects when `~/.mindflayer` moves and
leak its path into repositories. `.codex/skills` and `~/.codex/skills`,
deprecated or undocumented, are never targeted. Unblocks #17, #28, #34 and #40.

### D-08: An explicit or activated workspace wins

The workspace is `-W/--workspace DIR`, then `$MINDFLAYER_WORKSPACE` (set by
`flayer activate`), then the first one walking up, then the home default. An
explicit `-W` wins even when empty, and empty means none, following the
`--home=` pattern so test helpers can pin it; `--home` and `$MINDFLAYER_HOME`
keep meaning where the default lives. One core function decides, fed by the
front ends, and an explicit or activated path that is not a workspace is an
error, never a silent fallback. Activation exists to override location, as a
virtualenv does. Rejected: walking up first, under which an activated flayer
silently loses to any workspace above the current directory; reusing `--home`,
which confuses where the default lives with which workspace to use; and a
direnv-style activation file, hidden state in repositories meant to stay
minimal. Unblocks #54, #55 and #56.

### D-09: Project settings live in the project

A project's agents, share mode and `edit_tracked` live in its own v3 marker,
`.mind/settings.json` (D-06), ignored by default (D-01). They describe the
files physically in the project, and several workspaces may link one project,
so keeping them beside it means every workspace sees the same targets.
Rejected: `[[project]]` tables in `flayer.toml`, which let two workspaces
disagree about one project's files; and the ledger, which hides editable
configuration in a database. Unblocks #29 and #37.

### D-10: An MCP server is one owned entry per agent

Each server is one versioned, neutral definition in the library,
`mcp/<name>.toml`, shaped as a subset of the registry's `server.json` with
typed inputs. Installing renders it as exactly one entry in each target agent's
project config, and ownership is a ledger row holding a semantic fingerprint of
that entry; project scope only, for now. This reuses the catalog and the
create, rename, remove and validate lifecycle, maps registry entries easily,
and touches one key per agent file. A marker key inside the entry cannot work,
since OpenCode rejects unknown keys, `codex --strict-config` fails on them and
`.mcp.json` allows no comments, so the fingerprint is the only ownership
signal. Rejected: storing `server.json` verbatim; copying whole config files,
which overwrites the user's other servers; and delegating to the agents' CLIs,
which reach only Codex's user scope, where `codex mcp add` re-serializes every
entry, though they may return for user scope (D-22). Unblocks #18, #86, #89,
#93 and #94.

### D-11: MCP secrets are environment references

A secret is written as a reference in each agent's own syntax (Claude `${VAR}`;
Codex `env_vars`, `env_http_headers` and `bearer_token_env_var`; OpenCode
`{env:VAR}`), and its value is never stored. A literal in a secret input is
refused, and so, for that agent, is a shape it cannot express: in Codex, secret
arguments, a secret inside a URL, and composite secret headers other than
`Authorization: Bearer`. References work in all three agents without Mindflayer
running and keep committed files safe, and refusing rather than writing a
literal keeps the guarantee where Codex has no interpolation. Rejected: values
under `~/.mindflayer/secrets` read through OpenCode's `{file:...}`, which
Claude and Codex cannot match; and, for now, a `mind mcp run` wrapper reading
the OS keychain, which may come later as an opt-in but makes every teammate
depend on the binary and does not cover remote headers. Unblocks #18, #86, #87,
#90, #91 and #92.

### D-12: OpenCode gets the V2 MCP shape

The owner runs OpenCode 2.1.3, so Mindflayer writes a minimal V2
`mcp.servers.<name>` entry, which V1 also reads since 1.18.24, and the V1
`mcp.<name>` shape only when the target file already uses it. `codemode`,
`protocol` and `timeout.startup` are left out unless asked for, because V1
warns on them. Following a file's existing shape avoids mixing two in one file.
Rejected: always the V1 shape; and running `opencode --version` on every write,
which would tie core to a binary. Unblocks #92.

### D-13: Sources have names, and a marketplace is a parent

Every gathered source (closed issue #7) gets a unique, editable name in
`[A-Za-z0-9._-]`, `owner-repo` for git and the catalog's own name for a
marketplace by default, and `--from <name>` takes it. Its identity is a
canonical, credential-free URL plus a non-null reference key. A marketplace is
a parent source with one pinned child per plugin entry, gathered only when
selected (`--plugin`, or a confirmed `--all`), and refreshing or removing the
parent cascades. What a listing shows is then exactly what a command takes;
each child keeps its own revision and its own failure, since entries point at
different repositories and pins; a token in a URL never reaches
`mindflayer.db`; and the duplicate hole where SQLite's UNIQUE ignores a NULL
reference closes. Rejected: a `name@source` spelling, when `@` already means
plugin@marketplace in Claude and skill@version in `gh`; and keeping URL
strings. claude-community lists 2283 entries, so gathering a marketplace whole
by default would clone thousands of repositories. Unblocks #98, #100 and #103.

### D-14: Core owns HTTP, behind a seam

Core owns catalog URLs, paging, caching and parsing behind an injectable `Http`
trait. The default is a blocking client reusing `reqwest` with rustls, already
in `Cargo.lock`, and tests pass a fake. Paging, retries and mapping are rules,
so they belong where the CLI, the TUI and the server cannot disagree about
them, and a synchronous client fits a synchronous core. Rejected: front ends
fetching the bytes for core, which spreads those rules across them; and calling
HTTP with no seam, which takes the tests online. Carrying it out amends [the
line between core and a front
end](#where-the-line-between-core-and-a-front-end-falls) and AGENTS.md's "only
I/O is files". Unblocks #105, #106 and #108.

### D-15: Skill search is GitHub plus opt-in finders

`flayer gather search` queries GitHub code search when a token is supplied,
plus opt-in finders, marked unstable, for skills.sh's `/api/search` and
api.claude-plugins.dev; the owner reads those two services' terms first.
Services that need a key or attribution (SkillsMP, Glama, PulseMCP, skills.sh
v1) are excluded, because they bring terms and attribution obligations. A
finder only returns addresses that resolve to git sources, so an endpoint that
changes breaks nothing already gathered, and its failure is a warning.
Rejected: GitHub alone, and no finders at all, both of which leave the skill
directories out (vision point 5). Unblocks #108.

### D-16: Credentials come from the front end, never from a prompt

The front end supplies a credential provider, `--token-env VAR` or
`gh auth token`, handed to `gix` through a credentials callback that never
prompts. Failing that, the git-config helpers that `gix` runs itself are tried,
then the system ssh binary for ssh URLs. A token is never persisted. This
copies Claude Code's own non-interactive model, and a prompt would hang the TUI
and the server, so the never-prompting callback lands early (#26) and the
providers later (#110), which also documents the ssh-binary and git-helper
dependencies in [Why `gix`](#why-gix-rather-than-running-git). Rejected: HTTPS
tokens only, which leaves ssh remotes out; and shelling out to `git`, which
gives up what `gix` was chosen for. Unblocks #26 and #110.

### D-17: The next front end is a local web panel

`flayer ui` and `mind ui` serve a Vue single-page app embedded in the same
binary, behind a default-on cargo feature, `ui`. A library crate,
`apps/server`, runs axum on 127.0.0.1 and handles transport only: it is generic
over a `Backend` trait with associated `Query`, `Task` and `Outcome` types, so
it only moves JSON, and the CLI implements the trait. There is one binary to
install, it works in any browser, the associated types keep the Cargo graph
free of a cli/server cycle without an extra crate, and the server's tests use a
fake `Backend`. Rejected: a separate protocol crate for `Task` and the outcome
types, which the associated types make unnecessary; a Tauri desktop app first,
while Tauri 3 is alpha, though a `Transport` interface in the frontend keeps a
later Tauri shell cheap; and a separate server binary or daemon, the re-exec
wrapper [the line between core and a front
end](#where-the-line-between-core-and-a-front-end-falls) already rejects. #68
measures the feature's binary size and compile time, and makes it opt-in if the
cost is too high. Unblocks #65, #68 and #69.

### D-18: The panel writes by running commands

As in the TUI, the panel's reads are typed queries that core answers, and its
writes are typed Tasks turned into CLI command lines and run through the same
parser on one worker thread, each line shown before it runs. That extends
[every change is a command line](#every-change-is-a-command-line) to a second
front end, so the three cannot disagree, and gives the panel a transcript the
user can replay. Four things come first: the remaining CLI-side orchestration
moves into core (#23, #54), CLI warnings carry core error codes (#63), batch
rules for assignment are planned in core (#78) rather than reimplemented in
TypeScript, and every Task from the browser passes a path policy (#67).
Rejected: a core service layer the server calls directly, which duplicates
every command's entry point; and shelling out to the `flayer` binary, which
needs the argument forwarding already rejected for the two binaries. Unblocks
#63, #67, #76 and #78.

### D-19: The panel is built on Nuxt UI

The panel uses Nuxt UI 4: MIT, built on Reka UI and Tailwind v4, with a
dashboard layout, a command palette, a virtualized data table and colour mode,
and an official dashboard-vue template on Vue 3.5, Vite 8 and TypeScript 6. It
covers the control-panel pieces out of the box under a GPL-compatible licence,
so it needs the least custom code. Rejected: shadcn-vue, the same base with
component source copied into the repository, which can still be compared on one
screen of the #69 scaffold during M1 to M3; PrimeVue 5, whose proprietary,
keyed licence is incompatible with GPL-3.0-only; and Naive UI, with
accessibility gaps open since 2021. Unblocks #69.

### D-20: OpenCode subagents (pending)

Not decided. Whether "agentes de OpenCode" means the OpenCode tool or also its
subagent definitions is open, and the owner, who mainly uses Claude Code, has
not chosen. Until then OpenCode is supported as a tool, receiving skills, rules
and MCP servers as Codex does, and no subagent-definition work is planned. The
other option would manage subagents as one kind per agent
(`.claude/agents/*.md`, `.opencode/agents/*.md`, `.codex/agents/*.toml`),
because the three formats are incompatible and share no location; skills
already act as slash commands in all three agents. Unblocks nothing yet;
choosing subagents would add kinds, through [Adding a third
kind](#adding-a-third-kind).

### D-21: The panel's views follow the capabilities they show

The views are built after the core capabilities they reuse: agents (M1), rules
and updates (M2) and the hub (M3), so they sit on the final data model and need
no rework. The server shell, its guards and the scaffold can be built in
parallel during M1 to M3, so the owner can review the look (D-19) early, and
MCP and connectors then ship in core, CLI, TUI and web together. The snapshot
lists kinds from `Kind::ALL`, so MCP reaches the panel without frontend rework.
Rejected: views right after M0, on today's data model, which are reworked when
the hub and the agent targets land; and the panel last, after MCP and
connectors, which leaves the owner's top wish to the end. Unblocks #65, #66 and
#69, which need nothing from M1 to M3, and #61, which needs only #36; the views
wait for the snapshot (#62).

### D-22: User-scope installs come later

Installing into user scope (`~/.agents/skills`, `~/.claude/skills`, user MCP
config) comes after M7, as a follow-up milestone, behind an explicit `--user`.
It writes `~/.agents/skills/<name>` and `~/.claude/skills/<name>`, never a
`manifest.json` in `~/.claude/skills`, and never anything under
`~/.claude/skills/synced` or `.trash`. Meanwhile `flayer doctor` (#96) reports
project skills that shadow user-scope skills, or are shadowed by them, with
different contents. One name at both scopes resolves differently in each agent
(Claude prefers the personal one, OpenCode V2 lets `~/.agents` beat a project's
`.claude`, Codex shows both), `~/.agents/skills` already holds other tools'
skills, and Claude before 2.1.280 moved skills listed in
`~/.claude/skills/manifest.json` to `.trash`. Project scope keeps ownership per
project, where the ledger can track it. Rejected: project scope forever, which
gives up the hub's promise of skills for every project; and user scope now, as
the home workspace's default. Unblocks #17, #28, #94 and #96.

### D-23: What a project received is an installed view

The catalog stays about authored payloads: the library's and a project's own
files and folders, MCP definition files included. What a project has received
through an `AGENTS.md` block or an agent config entry is an installed view,
built from the ledger plus the target files, which status and the snapshot
show. `edit`, `rename` and `remove` aimed at an installed section or entry are
refused, with a hint to uninstall, or to change the library copy and update,
and foreign entries found in agent configs are listed as foreign. "Discovery
matches on `Layout` and nothing else has to know" then stays true for authored
files: a section or an entry is never authored in place but rendered from a
library artifact, and editing one in place is exactly what the Edited state
(D-04) detects. Rejected: new `Layout` variants, which would make discovery
parse `AGENTS.md` blocks and agent configs; and treating installed sections and
entries as artifacts editable in place, which lets three agents' config formats
leak into the lifecycle code. Unblocks #33, #45, #62, #93, #94 and #95.

## Open questions

- **Precedence between projects.** Two projects in one workspace can declare
  the same skill name. Core returns both matches and says nothing about which
  wins, because nothing here yet has to choose. Whatever resolves it (a
  workspace-level override, an explicit order in `flayer.toml`) belongs in core
  when it exists, not in a front end.
- **Replaying the install screen.** One install at a time has a command now,
  but what the screen applies is a batch, and its transcript line is
  `flayer install` rather than the `install` and `uninstall` lines it amounts
  to. Printing the batch as those lines would make a session with the screen in
  it as repeatable as one without.
- **Installing rules.** Installing copies a directory, so only skills can be
  installed — from the shelf, and now from the workspace's own. The workspace's
  own rules have no way into a project yet; a rule is one file, filed by its
  route, and copying one means answering where its folders go.
  [D-03](#d-03-rules-reach-every-agent-through-agentsmd) answers it: a rule
  reaches a project as its section of a managed block in `AGENTS.md`, with
  `.claude/rules/mindflayer` as Claude's fallback, and #45 carries it out.
- **An edited copy that stays managed.** Editing an installed skill makes it
  its author's, so updates from the shelf stop reaching it — the safe answer,
  not the best one. A hash of what was installed, kept in the ledger, would let
  the install screen tell an untouched copy from an edited one, and offer to
  update the first while leaving the second alone.
  [D-04](#d-04-an-edited-copy-is-detected-not-disowned) chose that hash, one
  per target, and #47 carries it out.
- **Disowning from elsewhere.** `install::disown` finds the workspace by
  walking up from the project, so a workspace that links the project from
  somewhere that is not above it keeps its stale record. Every workspace
  linking a project would have to be findable from the project for that to
  change.
- **Reading commands create the home workspace.** `flayer list`, `show`,
  `validate`, `scan`, `templates`, `gather list`, `minds`, the `flayer` TUI and
  `mind install`/`uninstall` all make `~/.mindflayer` when nothing up the tree
  has one, though only something that writes needs it to exist. `default_at`
  already opens without creating; moving the reads onto it, and leaving
  `locate_or_default` to the commands that write, would mean a first `flayer
  list` outside every workspace no longer puts a marker in somebody's home.
- **Long work in the TUI.** A gather holds the screen until it finishes, with a
  box saying what is running, and cannot be cancelled. A background thread
  would change that; it has not been needed yet.
- **Gathering rules.** Only skills are gatherable. A rule is a loose file at any
  depth, so harvesting one means deciding what its name is relative to — the
  same question `Catalog::take_files` answers inside a project, asked of a
  folder that is not one.
- **Private repositories.** `gix` is given no credentials, so a private source
  fails at the fetch. What it should be given is decided in
  [D-16](#d-16-credentials-come-from-the-front-end-never-from-a-prompt): a
  provider the front end supplies, through a callback that never prompts, then
  the git-config helpers `gix` runs itself, then the system ssh binary. #26 and
  #110 carry it out.
- **Rules that want to declare something.** Today a rule has no front matter,
  so a leading `---` in one is content. If rules later want metadata, that
  becomes a breaking reading of files written now. The escape hatch is cheap
  and deliberate: `Declared::Rule` is a variant, so giving it a payload is a
  local change. [D-03](#d-03-rules-reach-every-agent-through-agentsmd) settles
  the if: a rule that declares `paths:` goes only to `.claude/rules/mindflayer`,
  so rules will carry front matter, and #41 gives `Declared::Rule` that payload.
