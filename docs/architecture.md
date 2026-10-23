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

`try_init` rather than `init`: this is the one command that needs a terminal,
and being run without one deserves a sentence rather than a panic.

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
— `flayer`, `flayer tui`, `flayer install` — is refused if it ever reaches that
path, rather than drawn inside this one.

The exceptions are named, not hidden: the install screen is embedded as it is,
and applying it is the batch `flayer install` applies; re-reading the disk is
not a command, because it changes nothing.

### Split three ways, like the install screen

- `tui/state.rs` — what a key does. It moves cursors, fills forms and returns
  tasks; nothing in it touches a file, so a test drives it with the keys a
  person presses.
- `tui/ui.rs` — what the screen looks like, rendered into an in-memory
  terminal by the tests.
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

It is only ever made by something that writes. Asking whether a project is
managed — `mind` opening its TUI, `mind install` — uses `default_at`, which
opens the home workspace if it exists and never creates it: reading is no
reason to put a marker in somebody's home.

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
- **An edited copy that stays managed.** Editing an installed skill makes it
  its author's, so updates from the shelf stop reaching it — the safe answer,
  not the best one. A hash of what was installed, kept in the ledger, would let
  the install screen tell an untouched copy from an edited one, and offer to
  update the first while leaving the second alone.
- **Disowning from elsewhere.** `install::disown` finds the workspace by
  walking up from the project, so a workspace that links the project from
  somewhere that is not above it keeps its stale record. Every workspace
  linking a project would have to be findable from the project for that to
  change.
- **Long work in the TUI.** A gather holds the screen until it finishes, with a
  box saying what is running, and cannot be cancelled. A background thread
  would change that; it has not been needed yet.
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
