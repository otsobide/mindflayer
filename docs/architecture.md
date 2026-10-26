# Architecture

## The shape of the tree

Every unit of the product is an app under `apps/`, with its own `Cargo.toml`
and its own `Makefile`. The root `Makefile` delegates (`make <app>/<target>`)
and CI invokes those targets, so adding a front end is adding a directory, a
line in `APPS` and the workspace `members`, and its test and build jobs in
CI — never reshaping the tree.

```
apps/core    mindflayer-core   — the engine
apps/cli     mindflayer-cli    — the `mind` and `flayer` binaries
apps/server  mindflayer-server — the local server behind the web panel
```

The layout exists because there would be more than two, and the server is the
first of the apps [D-17](#d-17-the-next-front-end-is-a-local-web-panel) adds;
the web panel it serves comes next, as `apps/web`. Both sit beside the CLI
rather than around it.

## Where the line between core and a front end falls

`mindflayer-core` knows what a mind project is, what a flayer workspace is, and
what each kind of artifact is. It reads and writes files, and the only other
I/O it does is fetching a repository through `gix` when asked to gather. It has
no idea a terminal exists.

`mindflayer-cli` parses a command line, asks core for values, and renders them.
It holds no rules about artifacts: not the name limits, not the front matter
format, not which folder a kind lives in, not when a change drops the record
of an install.

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

The local server is a crate of its own, and it holds no rules either: it
carries JSON between the browser and whichever front end implements its
`Backend`, and depends on neither core nor the CLI. See [the local
server](#the-local-server).

The line is there so that when a second front end arrives, it cannot disagree
with the first about what a valid skill is. Every rule that could drift lives
in one crate, and the front ends only choose how to show its answers.

Two consequences worth stating, because they are what the split buys:

- **Sorting happens in core**, not in the renderer. Two front ends listing the
  same projects produce the same order.
- **Rendering happens in the front end**, and the CLI builds its output into a
  string before printing it, which is what lets its tests assert on the exact
  text a user sees.

### How the CLI crate is laid out

The grammar is in `parse.rs`, the work in `commands.rs` and a module per
family of commands under `commands/`, and what a command produced, with the
helpers every report is laid out with, in `render.rs`. `CliError` has
`error.rs` to itself, `homes.rs` decides where the default workspace's home and
each agent's own folder are, and gathers what `-W` and `$MINDFLAYER_WORKSPACE`
say (`Lookup`), and `lib.rs` only says what is public.

The split is by family. One file holding the parser, every command and the
rendering was where every change to a command met every other, so pieces of
work on different commands collided in it. One module per command would
scatter what a family shares instead: `load` and `unload` report the same way,
and `list`, `show` and `validate` read one catalog. `commands.rs` dispatches,
and asks core what a command acts on, the one thing every family needs: which
workspace is core's `resolve` to decide (see [which workspace a command
uses](#which-workspace-a-command-uses)), and which of its projects `-p` names
is `FlayerWorkspace::pick`.

## The kinds, and how a kind is described

An artifact is a skill or a rule, and the difference is a **payload**, not a
field:

```rust
pub enum Declared {
    Skill(SkillManifest),
    Rule(RuleManifest),
}
```

`Declared` *is* the kind, so an artifact cannot carry a discriminant that
disagrees with what was parsed — there is only one. Each payload says what its
kind requires: a skill without a description is not a state the type can hold,
a rule's front matter is optional all through, and no `Option` field on
`Artifact` sits permanently `None` for one kind because only the other has it.

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
A rule's front matter has no name in it — a `name:` there passes through like
any key Mindflayer does not read — so its name is its route. This is why the
two are not symmetric, and the asymmetry is the honest one.

### What a listing shows for a rule

A skill has a `description`. A rule has its body: the file, less any front
matter. The body's **opening line** — the first line carrying any text, with
leading `#` stripped — is what a listing shows, and it is captured when the file
is loaded, because the file was read anyway. A rule without front matter is
summarised exactly as it always was.

Not the file's opening line, which for a rule with front matter is a fence, so
every scoped rule would be listed as `---`. And not a `description` the rule
declares either: the body is what an agent is given once the front matter is
taken off, which is how a rule reaches the `AGENTS.md` block (#42), so its
opening line is what the agent meets first, and a listing that shows something
else would describe the rule by a line no agent reads.

That one value does double duty: it is the summary a listing prints, and its
absence is the one thing that makes a rule invalid, since a body with no line of
text has nothing to offer an agent. Storing the fact rather than the verdict is
what keeps `validate` pure — every check it makes was decided when the file was
read, so asking whether something is valid cannot itself fail.

### What a skill declares

A skill's front matter is the Agent Skills format
([agentskills.io/specification](https://agentskills.io/specification)), the
one Claude Code, Codex and OpenCode all read, and `SkillManifest` holds the
fields it defines and no others:

| Key | | Held to |
|---|---|---|
| `name` | required | lowercase letters and digits joined by single hyphens (`^[a-z0-9]+(-[a-z0-9]+)*$`), at most 64 characters, and the same as its folder |
| `description` | required | 1 to 1024 characters |
| `license` | optional | |
| `compatibility` | optional | 1 to 500 characters |
| `metadata` | optional | a map from strings to strings |
| `allowed-tools` | optional | tools separated by spaces |

Every other key — Claude Code's `paths` and `disable-model-invocation`, say —
belongs to an agent, and passes through: not modelled, not rejected, and kept
byte for byte by everything that writes a manifest, since `rename` rewrites one
line and gathering and installing copy files. Modelling them would make this
crate the place where three agents' formats are reconciled, which is not a job
anybody asked it to take on.

`allowed-tools` is split on spaces, as the specification writes it, and on
commas, as Claude Code first wrote it and older skills still do — never inside
parentheses, so a permission pattern such as `Bash(git add *)` stays one tool.
A YAML list is read too.

`metadata` is read leniently. The specification makes it a map of strings, but
a shipping agent writes nested maps and booleans in it
([agentskills#514](https://github.com/agentskills/agentskills/issues/514)),
and failing the whole manifest over one entry would leave the skill out of
every listing. So the string entries are read, the others are named for
`validate` to report, and nothing is rewritten: what is ignored here is still
in the file for whichever agent wrote it. `compatibility` is read the same way,
since a list of the agents a skill was made for is the obvious thing to write
in place of the specification's sentence: a value that is not a string is
ignored and reported. Neither builds what it ignores, only skips it, and
`metadata` is read an entry at a time rather than into a map, so a key written
twice is reported (the first is read) instead of failing the manifest, as a map
would.

The name rule is held to every name, a rule's route segment by segment
included. A looser one for rules was the alternative, and it would only have
been a second rule to keep in step with the first.

### What a rule declares

Most rules declare nothing: a rule is a markdown file that gives context.
Claude Code's rules can open with front matter saying which files they are
about, and [D-03](#d-03-rules-reach-every-agent-through-agentsmd) sends a rule
that does to `.claude/rules/mindflayer` only, so a rule may carry some.
`RuleManifest` holds the two keys Mindflayer reads, both optional:

| Key | | Read as |
|---|---|---|
| `paths` | optional | glob patterns: a list of them, or one on its own |
| `description` | optional | a string |

Rules were written long before any of them was read this way, and markdown is
free to open with a `---` horizontal rule. So a fenced block at the top is
front matter only when it holds a YAML mapping. Anything else is content, as
it always was: an opening fence that never closes, prose between two
horizontal rules that YAML reads as a plain string, a list, two horizontal
rules with nothing between them, and a
block of nothing but comments too, since what YAML reads past as a comment is a
heading to markdown. An empty block could have been front matter that declares
nothing, but taking its fences off would drop two lines that read today and
gain nothing. The block is parsed as a YAML value before anything is read from
it, so that what it *is* decides whether it is front matter; deserializing
straight into a struct would have to tell a sentence from a broken mapping by
the wording of its error.

A block that does not parse at all is content too, and `validate` warns about
it, since it is most likely front matter written wrong: a glob opening with `*`
is a YAML alias unless it is quoted. Refusing the rule, as a skill whose front
matter does not parse is refused, was the alternative. It would hide from every
listing a rule that still works as text, over how its first lines are written;
a skill without its front matter has no name to be found by, and a rule never
needed one.

The line is YAML's, not the author's intent, so prose between two horizontal
rules can land on either side of it. A sentence shaped like `Note: never
force-push` is a mapping with one key nothing reads, so it is front matter and
left out of the body an agent is given, without a word. One opening with `**`,
a backtick or a link does not parse, so it is content, with the warning above.
Both are rare at the top of a rule. Warning about a mapping that declares
neither `paths` nor `description` would catch the first, and was turned down:
it would warn just as loudly about every rule carrying only another agent's
keys, such as Cursor's `globs`, which pass through by design.

The keys are read leniently, as a skill's `metadata` and `compatibility` are: a
`paths` that is not a pattern or a list of them, or a `description` that is not
a string, is ignored, reported as a warning and left in the file. A list
holding anything but strings is ignored whole, since keeping its strings would
scope the rule to fewer files than its author wrote. One pattern is never split
on commas, because a brace expansion such as `src/**/*.{ts,tsx}` has one inside
it. Every other key — Cursor's `globs`, a `name` — passes through, as it does on
a skill.

A key with nothing in it is unstated, as `paths:` with no value is: a blank
`description`, and a `paths` that is a blank string or an empty list. A blank
pattern matches no file, so one in a list is left out and the rest read. This
matters for #42 and #43, where a rule that states `paths` goes to
`.claude/rules/mindflayer` only: read as stated, `paths: []` would scope a rule
to no file, which is a rule no agent is ever given, and `show` would print an
empty description or a `paths:` heading with nothing under it. Warning about
an empty key, as `compatibility: ""` is warned about on a skill, was the
alternative; but a skill's `compatibility` is shown to an agent as written,
while an empty `paths` says nothing an agent could act on.

`Artifact::contents` is the body without the front matter, for a rule as for a
skill. It is what a renderer puts in front of an agent, so the fences are never
pasted into `AGENTS.md`, and what `show` prints below the description and the
`paths` a rule declares.

### Strict when writing, lenient when reading

`Artifact::validate` gives one list of problems, and it is treated three ways:

- **Refused** when Mindflayer writes a skill into a project, if the name or
  the description breaks the rules. `mind add`, `rename` and `install` all
  ask `artifact::strict_issues`, which keeps what `ValidationIssue::is_strict`
  picks out of that list, for the skill as it is about to be written — so one
  change to the set reaches all three. `rename` asks too, although the name is
  all it writes, because what it leaves behind is a skill Mindflayer wrote;
  `mind add` checks the name and the description before building anything as
  well, only for messages of its own. Those are what agents refuse: Codex skips a skill whose name is over 64
  characters or whose description is empty, so writing one would put a skill
  in the project that never loads. So is a name that is not the folder the
  skill is filed in, since an agent finds a skill by one or the other — asked
  of the folder Mindflayer is about to write, not the one the files were read
  from. `compatibility` and `metadata` are not refused, because an agent reads
  past them. A rule `mind add` writes from a template is refused for whatever
  its front matter would leave unread (`artifact::rule_issues`), which reading
  only warns about ([Templates live inside the
  markers](#templates-live-inside-the-markers)).
- **Warned about** when gathering, which takes the skill anyway, as the
  agentskills.io client guide advises for skills written for other agents. The
  shelf is a record of what a source holds, and leaving a skill off it would
  hide the problem along with the skill.
- **Reported** by `validate`, in full, and at the workspace level for the shelf
  too, so what gathering warned about can be found again later. Only a
  problem makes an artifact invalid and the exit non-zero; what an agent reads
  past (`ValidationIssue::is_warning`, the same `compatibility` and `metadata`
  that are not refused, and whatever a rule's front matter leaves unread) is
  listed as a warning and leaves the verdict at `ok`.
  Counted as invalid, a skill installed with nested metadata would fail its
  project's `validate` from then on, and the only way to clear it would be to
  edit the copy, which disowns it. The TUI's `✗` follows the same line.

### Adding a third kind

A variant on `Kind`, its folder and layout, a variant on `Declared`, a
constructor on `Artifact`, and an arm in every match on the kind — discovery
and discovering the shelf, `create`, `lifecycle::load` and `lifecycle::rename`,
the refusal in `gather`, and what `install` holds a copy to — which the
compiler lists, since the matches are exhaustive. Reports need nothing:
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
anywhere inside a project, like every git command. A workspace can also be
named, with `-W` or by activating one, and then it wins over the walk: see
[which workspace a command uses](#which-workspace-a-command-uses).

Nothing stops one directory from being both.

### The command surface is split the same way

`mind <cmd>` acts on the project you are standing in; `mind flayer <cmd>`, and
therefore `flayer <cmd>`, acts on the workspace above it, or the one `-W` names
or an activation chooses. The split decides two things that used to be
guesses:

- **What is in scope.** `mind list` is the project's own artifacts, never its
  neighbours'. `flayer list` is the workspace's own and every registered
  project's. Before the split one
  command tried to be both and its answer changed depending on which directory
  it was run from.
- **How an artifact is labelled.** Only the workspace level qualifies one by
  what holds it — a project, `workspace` for its own, or `shelf/<source
  folder>` for what `flayer validate` finds on the shelf — because inside a
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
you are in, against its workspace: the one named or activated, or else the one
above it. `flayer list`, `show` and
`validate` read everything — the workspace's own and every project's — or one
project with `-p`; `flayer validate` reads the shelf as well, labelling what it
finds there `shelf/<source folder>`. Reads look everywhere; a change needs one
place, and without `-p` that place is the workspace.

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

Because this changed where a project's artifacts are, `MIND_FORMAT_VERSION` is
2 and a `mind.toml` written before it is read the way it was written: version 1
had no such question, so every kind lived inside `.mind`, and reading one with
today's default would point it at directories it never had and list nothing
without saying why. `DIRECTORIES_VERSION` is what draws that line.

### Why the markers are TOML written from a template

New markers are written from a string template, not serialized. Serializing
would drop the comments, and the comment in `mind.toml` explaining where skills
go is the first documentation anyone opening the file will read.

Editing an existing marker has the same constraint and a harder job, which is
why `toml_edit` is a dependency: `flayer link` rewrites the one `projects`
array and leaves every other byte alone. Reading stays serde's job. The two
halves agree because an edit re-reads the file afterwards rather than trusting
what it believes it wrote, and the write itself goes through a temporary file
and a rename, so a crash leaves either the old config or the new one. The edit
holds the workspace's lock from its read to that re-read, so a second writer
edits what the first one wrote ([Writers at the same
time](#writers-at-the-same-time)).

### Formats are versioned

Each marker carries a `version`, and each has its own: `MIND_FORMAT_VERSION`
for `mind.toml`, `FLAYER_FORMAT_VERSION` for `flayer.toml`. A marker written by
a newer Mindflayer is refused with a message naming which of the two it is and
which version this build reads, which is what lets a format change later
without an old binary silently misreading a newer project or workspace.

Two constants rather than one, because the files change for different reasons.
Shared, a change to either format would be a new version of both: the project
marker going to 3 would write every new `flayer.toml` as a 3 too, and an older
build would refuse a workspace that had not changed at all. Both are 2 today,
and neither moves when the other does.

An older marker is read the way it was written, never refused.
`DIRECTORIES_VERSION` is the one line drawn so far, and it is a project-marker
version: a workspace's number says nothing about where a project keeps its
artifacts. The ledger carries its version in SQLite's `user_version` and is
refused from the future the same way, but an older one is migrated forward
rather than read as written, after a copy of it is set aside
([Why the ledger is SQLite](#why-the-ledger-is-sqlite)).

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
old config or the new one. The temporary is named for the run that writes it,
never shared with another writer ([Writers at the same
time](#writers-at-the-same-time)). The original's permissions are copied onto the
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
  lock                       held by whoever is writing
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
the skill declares. When those disagree that is something gathering warns about
and `flayer validate` reports; renaming the folder on the way in would repair
the symptom and hide it.

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
nothing. One that reads but breaks a rule is taken, with a warning for each
problem ([Strict when writing, lenient when
reading](#strict-when-writing-lenient-when-reading)).

### Why `gix` rather than running `git`

Nothing depends on a `git` being installed, or on which `git` is first on the
PATH, and core keeps doing its own I/O rather than supervising a process. The
cost is that authentication for private repositories is ours to solve rather
than inherited whole from `git`.

Half of it is solved: a fetch **never prompts** for a credential. When a server
asks who is fetching, `gix`'s own answer runs the credential helpers git config
names and, when none of them has a credential, asks a person: an askpass
program, then the terminal. The TUI owns the terminal while a gather runs, so
that prompt is a screen that stops answering, and the server that comes next
has no terminal at all. So every connection `gix` lets a clone configure is
given a credentials callback (`gather::credentials`) that runs the same helper
cascade with the prompt and askpass turned off. A credential no helper has is
`GitError::Credentials`: the URL without its credentials, and the two ways to
provide one, a git credential helper or, for an ssh URL, ssh-agent. Over ssh,
`gix` runs the system `ssh`, which authenticates with keys of its own and asks
the helpers only for a user name, so a login ssh refuses is reported the same
way. Whether a fetch went over ssh is read from the URL `gix` connected to, not
the one typed: git config's `url.<base>.insteadOf` can turn a typed https URL
into an ssh one, or the reverse. What ssh itself asks on the terminal is
outside the callback's reach, and so is the one connection `gix` opens without
letting the clone configure it; both are [open questions](#open-questions).

The callback is set on the connection rather than asked for through git
config (`gitoxide.credentials.terminalPrompt=false`), which would also stop
the prompt: the guarantee then lives in code a test reaches, not in a string
among the user's own settings, and the credential a front end supplies needs
the callback to arrive through anyway. That other half is
[D-16](#d-16-credentials-come-from-the-front-end-never-from-a-prompt)'s.

`gix`'s error types are large and would become part of this crate's public API
if they were carried, tying its version to gix's, so `GitError` keeps what they
said and not what they were.

### A URL is kept without its credentials

A source is stored, logged, listed and named in errors by its URL, so a token
typed into one would be repeated everywhere: in `mindflayer.db`, in the
clone's `.git/config`, in every report, and soon in a snapshot, `--json`
output and the panel. One function takes it out, `url::redacted`, and
everything that keeps or shows a URL goes through it.

What it takes out is everything before the `@` of a `scheme://` URL, except
the user name of an ssh URL: that is who ssh logs in as, part of where the
repository is rather than a secret, and a later gather from the stored URL
needs it. Over http(s) a user name goes as well, because GitHub takes a token
in that place and a credential helper is what names the user. The scp-like
form (`git@host:path`) is left alone, since `gix` reads everything before its
first `:` as user and host, which leaves no room for a password. A password
with a `/` in it is looked for past the first `/`, whenever what comes before
that `/` has a `:` and no port: every parser reads `https://me:p/ss@host` as
host `me` and port `p`, so nothing else would find the password, and the
refusal for an unparseable URL would store and print it whole. It works on
text rather than on one URL, because the other place a credential turns up is
`gix`'s own error messages, which quote the URL they failed on, and the URL a
fetch connected to may carry a token the typed one did not when git config
rewrites it with `url.<base>.insteadOf`. Parsing with `gix::url` and dropping
the user was rejected: the parser also rewrites what it keeps, a default port
dropped or a scheme lowercased, and cannot find a URL inside a sentence.

A gather URL that carries a credential is **refused** before anything is
registered or fetched, rather than used for that one fetch and stored without
it: a token on the command line is already where `ps` and the shell history
show it, and refusing is what points at a git credential helper, which keeps
it off the command line altogether, as the front-end provider of
[D-16](#d-16-credentials-come-from-the-front-end-never-from-a-prompt) will.
Behind that refusal the rest holds whichever caller is writing. The ledger
redacts every URL on the way in and again on the way out, so nothing it hands
back carries a credential, scrubbed yet or not. Every `GitError` redacts the
URL and what `gix` said. A clone is scrubbed as soon as it is made
(`git::scrub`): `gix` writes the URL into the remote's entry in `.git/config`
and into every reflog as `clone: from <url>`, and those files are rewritten
through the same function. A clone a gather makes now has nothing to take out,
and finding that costs a few small reads. A `--from` typed with credentials
names the source they were taken out of. The CLI's report prints URLs
redacted, and the TUI redacts a command line where it is built for showing
(`state::shell`), so the box that says what is running and the transcript
printed on the way out both leave the token out.

What a Mindflayer from before the refusal stored is **scrubbed** when a
command that writes opens the ledger. `FlayerWorkspace::ledger_to_write` is
the one way to open it for writing: it runs `gather::scrub`, which only core
can call, and hands back what that took out for the front end to say, the way
`resolve` with `Intent::Write` is the one way to find a workspace that may be
created.
Which commands write is the front end's to know, but what opening to write
involves is core's, so no front end can write first and scrub after. `gather
git`, `install` and `uninstall` open it that way, as does `install::disown`,
through which core's lifecycle changes (`lifecycle::add`, `finish_edit`,
`rename` and `remove`) drop a record. Each hands back what the scrub took out
(its result's `scrub`) beside what it did to the record, since a scrub says
what it found only the first time and a change that dropped it would leave it
unsaid for good. `rename` opens the ledger once for each name and tells what
both scrubs found once (`Scrub::absorb`): the second finds only the clones
the first could not rewrite. So does the install screen, in both screens,
when what it marked is applied (`install::commit`) rather than as it opens,
which is a read ([A read leaves the ledger as it finds
it](#a-read-leaves-the-ledger-as-it-finds-it)). A scrub can make two sources
one row, and a mark on the row that went would install from folders the scrub
removed, or copy the files and then fail to record them. So once a scrub has
taken out a source, the screen is built again from the scrubbed ledger and
each mark is made again on the row offering the same name from the same
place, as the row it became one with does; a mark on what only the row that
went had gathered is a warning and is not carried out. Scrubbing as the screen
opened would have spared that, and is rejected: opening it is a read.

The scrub rewrites the sources whose URL carries a credential, each clone made
from one, the origin of every installation from one, which schema 3 copied
from the URL as it was stored, and the action log. A clone is rewritten before
its row, so a clone that cannot be rewritten keeps its row tainted and is
tried again next time, as a warning; what was taken out is a note, said once,
since the command did what it was asked and an exit code that failed because
of a fix would break a script for nothing it did. When the URL without
credentials is already another source's, with the same branch and folder, the
two are one place gathered twice and become one row: two rows would offer each
skill twice under one origin, and SQLite's UNIQUE would refuse the rewrite
outright when a branch is set. The row kept is the one whose shelf was filled
most recently, since its folder holds the newer files, and a row whose gather
failed before it took anything loses however recently it was tried. What the
other row gathered goes with it, and its clone and shelf folder are removed (a
folder that will not go is a warning), rather than its records moving to the
kept row: they would point into a folder the ledger no longer reserves, which
the next source with the same readable name would be given and gather over.
The ledger turns on SQLite's `secure_delete`, so a rewritten row leaves no
copy of its old text in the file's free space.

It runs every time rather than once, as a schema step would, because an older
build could keep writing to the same file after a newer one had scrubbed it,
and the schema steps are spoken for by the changes that need a new version.
Schema 3 is a file such a build refuses, so the scrub could now be part of
that step; whether it should is open (see the open questions). With
nothing to do it still reads, every time a command writes: a scan of the
sources and one of the action log, in a transaction. Neither can use an index,
so the log's grows with the log, and every entry naming an ssh URL, which
keeps its `@`, is read and found clean again each time. Recording that the log
had been scrubbed would skip that, and is rejected for the reason the scrub
runs every time: an older build writing a token again would go unnoticed. The
cost ends when the scrub can stop (see the open questions). A read is left
alone: what it shows is redacted on the way out anyway, and a read should not
write. Until a command that writes scrubs it, a place an older build gathered
both with a token and without one is read as the file has it, two sources
offering each skill under one origin. Deleting the old
clones instead of rewriting them would have been simpler and was rejected:
the clone is kept so a gather can be looked at afterwards, and a scrub has no
reason to take that away.

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
future is refused exactly as a marker file from the future is. The first
command that writes to it makes it; a read never does, and changes an existing
one only to migrate it ([A read leaves the ledger as it finds
it](#a-read-leaves-the-ledger-as-it-finds-it)).

Four things are recorded: the **sources** gathered from, the **artifacts** on
the shelf and which source each came from, the **installations** — which skill
Mindflayer put into which project, from where, every copy it wrote and a hash
of what it wrote — and an **action log**. The log keeps failures too — a log
of only what worked cannot answer the question anybody opens it with.
Timestamps are Unix seconds, which needs no date library and which SQLite
renders with `datetime(at, 'unixepoch')`.

No URL this build writes into it, or reads back out of it, carries a
credential. One an older build stored stays in the file until the next command
that writes scrubs it, and for good in the copy a schema step took before that
(see the open questions). An older build could store one again after a scrub
until schema 3, which it refuses
([A URL is kept without its credentials](#a-url-is-kept-without-its-credentials)).

A source's shelf folder is stored rather than derived: two URLs can reduce to
the same readable name, and where a source's files went is a fact about the
past that must not move when the naming rule changes.

The schema has had three versions, each a block in `Ledger::migrate` and a
constant beside it:

1. `sources`, `artifacts` and `actions`: what was gathered, from where, and
   what happened.
2. `installations`: which artifact went into which project, which is what
   lets an uninstall tell Mindflayer's copy from somebody's.
3. `installation_targets`, one row for each copy an installation wrote, with
   the agent it is for when it is one agent's
   ([D-07](#d-07-one-copy-per-agent-folder)); and on `installations`, the
   `origin` it came from, spelled as `--from` takes it, and the
   `content_hash` of what it wrote
   ([D-04](#d-04-an-edited-copy-is-detected-not-disowned)). The
   installation's own path moved into its one target, so a copy's path is
   kept in one place (#30).

An older ledger is migrated step by step, so one made two versions ago ends
where one made today begins, and two rules hold for every step:

- **The file is copied aside first.** Before the first step, the ledger is
  copied beside itself as `mindflayer.db.v<old>.bak` (`Ledger::back_up`), and
  nothing is migrated unless the copy is whole and opens at the old version.
  A step that turns out wrong then leaves the file it started from: the copy
  opens at the old version, and the build that wrote it reads it as it
  always did. It is named after the version it holds, so which build reads
  it is in its name, and a ledger brought forward by several builds in turn
  leaves one for each version it had. It is taken once the migration holds
  the write lock, not before: a copy taken after an unlocked read of the
  version could be of a ledger another process has since migrated, and would
  replace that process's copy of the old version with one of the new. It is
  SQLite's online backup rather than a copy of the file, because opening the
  file outside SQLite while a connection holds it would drop that
  connection's locks on Unix, and it reads through a second connection,
  because SQLite will not copy from one in the middle of writing. It goes
  through a temporary and a rename, so a copy cut short is never mistaken
  for a whole one, and the temporary is made with the ledger's permissions
  before anything is written to it, so a ledger kept private is not readable
  through its copy even while the copy is written. A step to come is one
  more block in `migrate`, and the copy is taken for it without its asking.
- **The steps run in one transaction**, with the stamp of the new version.
  Run a statement at a time, a step that failed, or a process that died
  halfway, left a database half migrated under its old version, which the
  next open would try to migrate again and fail on.

A step fills in what it can from what the version before recorded, spelled
as that version's code spelled it, in the step's own SQL rather than through
the code that spells it now: a later change to the spelling would otherwise
change what an earlier step wrote. Version 3 gives an installation from a
source that source's origin, and one from the workspace or a load none, since
version 2 kept nothing to say which; it makes up no hash for a copy nobody
hashed when it was written, and D-04 says how a missing one reads. Each step
comes with a fixture of what the version before it wrote
(`apps/core/tests/fixtures/ledger-v<N>.sql`, dumped from a ledger that build
made), from which a test migrates, checks every row is kept, and opens the
copy at the old version.

The hash is `hash::tree`, its only definition: SHA-256 over every folder and
file under a copy, by their `/`-separated paths in byte order, each file's
length before its bytes. The order is not the filesystem's and the separator
not the platform's, so one tree hashes alike on Linux, macOS and Windows, and
alike in every agent's folder, since the copy's own folder is not part of it.
Folders count, so two copies hash alike exactly when `copy::same` calls them
the same. A link is refused rather than followed: Mindflayer never writes one
into a copy, since `copy::tree` copies what a link points at, and following
one could read anything. It is kept once per installation, not once per
target: every target is written from one copy, so one hash is what each is
compared with, and a copy somebody edited is the one whose own hash no longer
matches it. An install records the hash of the copy as it stands once
written; one that cannot be read back is recorded with none rather than not
at all, since without a record it would be Foreign for good. That is a
`warning:` and a non-zero exit, and the log's entry says why, rather than a
plain success: a copy an install left as it was can hold a link somebody
put there, which `copy::same` follows and the hash refuses, and a record
with no hash is read as one from before hashes were kept.

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

### Held to the rules on the way in

What gathering took leniently, installing takes strictly: a skill whose name
or description breaks the rules is offered, so nothing gathered is hidden, and
refused, so nothing an agent would skip lands in a project. The check reads the
files about to be copied rather than what the ledger noted when they were
gathered, because the shelf is a folder anybody can edit and the workspace's own
are edited as a matter of course. For the same reason it compares what they
declare with the folder the copy is about to be filed in, the name the ledger
recorded: a shelf copy edited to declare another name, or a skill its source
renamed while the record of its old name stayed, would otherwise land under one
name declaring another. A refused skill is fixed upstream and gathered again,
or, if it is the workspace's own, fixed where it is.

### It only manages what it installed

The ledger records every installation against `(project, kind, name)`, with
each copy it wrote as a target of it, and that record is what separates a
file this tool put there from one somebody wrote. Today an install writes one
copy, in the folder the project's marker names; one in each agent's folder is
#34's. A skill present but unrecorded is `Standing::Foreign`, and Foreign is
inert in both directions: never overwritten, never deleted, and the caller is
told so rather than obeyed quietly.

Without that, an install screen is a thing that can delete a colleague's work
because a checkbox looked untidy. The rule costs one query and removes the
whole class.

The record names the project by its route from the workspace, taken to where
the workspace's own entry for it leads (`FlayerWorkspace::spelling_of`)
rather than to however the command spelled the project. `mind` spells its
project the way the operating system reports the directory it runs in,
through no symlink, while a workspace named with `-W` or activated keeps the
spelling it was given. The route between those two is not the entry's, so a
record written under it would be one `flayer -p` never finds: a copy nobody
may update, or, the other way round, a record that outlives its copy. A
spelling that already matches the entry as written is kept, so no record
written before is named differently now.

A project has one directory per artifact name, so two shelf entries offering
`commit-style` cannot both be installed. The screen settles it by unticking the
other rather than letting both apply and the second win — a conflict resolved
where somebody can see it happening.

### Built where no agent looks

An install never writes beside the copy it replaces. It builds the new copy in
a stage of its own under the project's `.mind/tmp` (`copy::Stage`), and renames
it into place once it is whole. Beside is where the agents look: OpenCode's
first line globs hidden folders in a skills folder too, so the
`.deploy.mindflayer-new` that staging used to put next to `deploy` was a skill
it could load half made, and one an interrupted run left there it would load
every time after. A name taken from the skill was also shared by every run
installing that skill at once, each deleting what the other was building.

- **`.mind/tmp`, made by the first install.** Inside `.mind`, a folder
  Mindflayer owns and no agent reads, and inside the project, so putting the
  copy in place is a rename on one filesystem. The system's temporary folder was
  the alternative, and it is often another filesystem, where a rename fails and
  a second copy would be the half-made state all over again. A move across
  filesystems is not attempted: a rename that fails is an error naming both
  paths. The folder holds a `.gitignore` of `*`, written with `create_new`, so
  nothing staged is committed even while `.mind` is. A rule of its own rather
  than an entry in [the exclude block](#what-keeps-an-install-out-of-git), for
  the reason [D-01](#d-01-a-projects-mind-ignores-itself) gives `.mind` one:
  a folder Mindflayer owns can carry it, so it needs no repository and goes
  with the folder. It is not the team's `.gitignore`, which is never edited.
- **A stage per run.** Named after the run's process id and a random suffix,
  and made with `create_dir`, which refuses a name that is there rather than
  sharing it: two installs at once, from two terminals or two workspaces that
  link one project, never build in one folder. The copy is built in the
  stage's `new`, the one it replaces moves aside to its `old`, and the stage
  goes when the install is done.
- **What an interrupted run left is swept by the next, and said.** Every run
  holds a shared lock on `.mind/tmp/.lock` while its stage lives. A run that
  can take the lock exclusively is alone, so every stage it finds was left by
  a run that stopped halfway: it removes them, and `Installation::swept` names
  them for the CLI to print under what the install did. When the copy then
  fails, `InstallError::swept` names them instead, printed before the error:
  they were cleared all the same. A ledger that fails after the copy stops
  everything, as it does wherever it fails, and they go unsaid with the rest.
  A run that cannot take the lock exclusively sweeps nothing, because what it
  finds may be in use. A lock rather than the process ids in the names,
  because whether a process still runs is a question each platform answers
  differently, and a lock the system drops when its process dies answers it on
  all of them. Only folders named the way a stage is are swept; anything else
  in there, somebody else put there. A stage that will not be removed is left
  for a later run, unsaid, rather than failing this one: it is litter in a
  folder nothing reads. On a filesystem without locks nothing is swept at all,
  since no run there can tell a live stage from a leftover, and what is left
  stays until somebody deletes it; the names still keep runs apart. Any error
  taking the lock counts as such a filesystem, not only an unsupported one: NFS
  without its lock daemon says "no locks available", and refusing that would
  make every install there fail.
- **What earlier versions left beside is cleared too.** A project that
  upgraded can still hold the `.deploy.mindflayer-new` or `-old` of an install
  an earlier version did not finish, and no sweep of `.mind/tmp` would find
  it. The next install of that skill removes both, whether or not it changes
  anything else, and says so as it says a swept stage. It does so without the
  lock, because no run of this version stages there, and those versions
  cleared each other's anyway. One it cannot remove fails the install, as it
  did when staging there, rather than staying where an agent reads.
- **The folder the copy goes in is made when it is missing.** Staging beside
  made it on the way. A fresh clone has no empty folders, so a project's
  `skills/` can be missing although its marker names it.

So an agent reading a skills folder finds the old copy, the new one, or, for
the moment between the two renames of an update, neither, but never part of
one and never anything beside it. Gathering still stages beside the shelf copy
(`copy::replace`), because no agent reads `.mindflayer`; see the open
questions.

### Two places offer, one command takes

What can be installed comes from the workspace's own skills, offered first,
the workspaces it loads (below), and the shelf (`install::Offer`). Every
installation records its origin, in the spelling `--from` takes; one from the
shelf also points at the source it came from, and one of the workspace's own
at none, because it came from nowhere but the workspace.

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
  workspace's own — because there is no shelf row to point at, only its
  origin, `load:<entry>`.
- **One level deep.** Only a loaded workspace's own artifacts are offered,
  never what it loads, so two workspaces loading each other is not a loop, and
  what a workspace offers is what its marker names, not a graph.
- **Into the workspace that is not the source.** Run from inside the workspace
  being loaded, the first workspace found walking up is that one, and a
  workspace cannot load itself. `resolve_except` walks on past it — and past
  every path given — to the next one, or the default in your home; loading the
  home workspace from inside it is refused. One named with `-W` or activated
  is taken as it is, since it was chosen, and loading it into itself is
  refused by the load: by `load_all`, before any of the paths given is
  written, so a source listed ahead of it is not left loaded by a command
  that failed.

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
Two spellings of one loaded directory are one source; `load_all`,
`unload_all` and `unlink_all` rewrite the marker once, for all of the paths or
none.

### What keeps an install out of git

A project in local mode
([D-02](#d-02-installs-stay-local-unless-a-project-shares-them)) keeps what is
installed into it out of git, through a block of anchored entries in
`$GIT_COMMON_DIR/info/exclude` that `exclude` keeps. Nothing calls it yet:
which paths to exclude is the installing side's to choose (#34 for skills, #43
and #45 for rules, #94 for MCP config), and this module only writes them down.

```
# mindflayer:begin
# project packages/app
/packages/app/.agents/skills/commit-style/
/packages/app/.claude/skills/commit-style/
# project packages/web
/packages/web/AGENTS.md
# mindflayer:end
```

Why that file, and why written that way:

- **Not `.gitignore`.** It is the team's file and it is committed, so a line
  in it puts copies only one machine has into every teammate's diff, which is
  the noise local mode exists to avoid. `core.excludesFile` is no better: it is
  the user's, read by every repository they have. Mindflayer edits neither,
  ever. `info/exclude` is read like a `.gitignore` and is never committed or
  pushed.
- **The common dir, not the git dir.** A linked worktree has a git dir of its
  own under the main repository's `.git/worktrees/`, but git reads
  `info/exclude` only from the common dir, so every worktree of a repository
  shares one file.
- **Anchored at the work tree root.** Entries are relative to the work tree,
  not to the project, so a project at `packages/app` gets
  `/packages/app/.agents/skills/<name>/`. Git anchors a pattern with a `/` in
  its middle on its own, but one segment matches at every level: without the
  leading `/`, a root project's `AGENTS.md` would keep every `AGENTS.md` in
  the tree out of git, a teammate's committed one in another project
  included. Every entry starts with `/`, so one rule covers both shapes. A
  directory ends in `/`, and a route's glob characters are escaped, so
  `apps/[legacy]` is that folder rather than a class of letters. The route is
  worked out from resolved paths, not lexically as elsewhere: git matches the
  tree as it is on disk and never follows a symlinked directory, so a project
  reached through `current -> packages/app-v2` is excluded under
  `packages/app-v2`.
- **One group per project.** Several projects can share a repository, so each
  owns the lines under its `# project <key>` header and an edit rewrites only
  those. The key is whatever the caller names the project by, its id once
  settings carry one (#29) and its route until then, and it is compared, never
  parsed. The same path in two groups is two claims on it: dropping one leaves
  the other excluding it. A group goes with its last entry, and the block with
  its last group.

And how it edits:

- **Only between the markers.** Every line outside comes back byte for byte,
  its line ending included. New lines take the ending the file already uses.
  A last line without a newline gets one when a block has to follow it, and
  the block's end marker goes without one instead, so the file still ends the
  way it did and dropping the block gives that line back as it was. The file
  is replaced through a temporary and a rename, as a marker edit is.
- **A block it cannot read is left alone.** A begin with no end, an end with no
  begin, a second block or a project with two groups is a failure for the
  caller to collect, and nothing is written. Guessing where a missing end
  belongs could pull somebody's lines into the block, and the next edit would
  rewrite them as Mindflayer's.
- **Outside a git work tree it writes nothing, and says so.**
  `Excluded::Outside` is not a failure, since a project need not be in git,
  but it carries a warning: whatever local mode installs there is kept out of
  nothing, and a repository made there later would offer all of it to be
  committed.
- **Found with `gix`, opened isolated.** No `git` binary, and neither the
  environment nor the user's git config takes part, so a `GIT_DIR` set in the
  shell cannot send one project's entries into another repository's file.
- **No version.** Unlike a marker, the block holds nothing but git's own
  pattern syntax and two kinds of comment, and an edit rewrites only its own
  group and keeps every other line where it is, so a block a newer Mindflayer
  wrote survives an older one's edit.

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
project's row runs `mind -C <project>` (`mind -W . -C <project>` for one the
workspace manages from outside its own tree; see [the TUI looks where it was
told](#the-tui-looks-where-it-was-told)): a task carries its holder, and the
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

A `-C` or a `-W` in those words is relative to the workspace root, which is
how someone reading the transcript takes it, so it is resolved against the
root rather than against wherever the process started. What the screen was
opened with is passed along rather than spelled: its home, and the `-W` or
activation that chose its workspace (`tui::execute`), as they were given to
`flayer` itself. A command that opens a screen of its own
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

## The local server

`apps/server` (`mindflayer-server`) is the transport under the web panel
([D-17](#d-17-the-next-front-end-is-a-local-web-panel)): a library crate that
binds a port, checks who is asking, carries JSON between the browser and a
front end, and serves the panel's files. No binary runs it yet. `flayer ui`
(#68) will, over the CLI's backend (#67). The token keeps out a process that
connects without it, and [the guards](#what-keeps-other-pages-out) keep out
other pages in the same browser: one on another port of 127.0.0.1, which the
browser counts as the same site, or one whose DNS name has been rebound to
127.0.0.1. Neither keeps out a server on another port of 127.0.0.1 that the
browser visits, because the cookie takes the token there as well (an
[open question](#open-questions)).

### Transport only

It holds no rules. It cannot tell a valid task from an invalid one, a linked
project from a path to refuse, or a skill from a rule, and it is not to learn:
each of those answers is core's, reached through the backend, so the panel
cannot disagree with the CLI and the TUI about any of them. What it knows is
HTTP — routes, the token, cache headers, a log line — and none of that would
differ between two front ends.

The panel's whole API is four endpoints, which are
[D-18](#d-18-the-panel-writes-by-running-commands) in HTTP:

- `POST /api/query` — a read, answered with JSON of the backend's own shape.
- `POST /api/commands/preview` — the command lines a task is, with nothing run.
- `POST /api/commands` — run a task; the reply is its lines and its outcome.
- `GET /api/transcript` — every task the session ran, with what each produced.

Any other path under `/api` is a JSON 404, and every refusal, a body the
backend cannot read included, is `{"error": "..."}`, so a script never has to
parse the panel's page to learn that it typed a path wrong.

### Generic over a backend

The server is generic over a `Backend` trait whose `Query`, `Task` and
`Outcome` are associated types. It deserializes into them, hands them over and
serializes what comes back, without ever naming one. So the CLI can implement
`Backend` with its own `Task` and depend on the server, while the server
depends on no mindflayer crate at all, which `crate_test` checks through
`cargo metadata`. A protocol crate holding the shared types was the
alternative, and it would put a third crate between every change to a task
and the code that runs it. The server's own suites use a fake backend whose
types know nothing of mindflayer, which is the property the CLI relies on.

`query` returns JSON rather than a fourth associated type, because each query
has a shape of its own and one type standing for all of them would be an enum
whose only job is to serialize untagged. `preview` takes the task by
reference, so a task need not be `Clone` to be both shown and run.
`watched_paths` is for live updates (#74); nothing reads it yet.

### Threads

axum runs on tokio, and nothing a backend does belongs on tokio's own
threads, which are for moving bytes: one slow read there stalls every request
waiting on the same thread.

- **Queries and previews** go to tokio's blocking pool (`spawn_blocking`), as
  many at once as arrive. They only read.
- **Runs** go to one dedicated `std::thread`, fed by a channel. A channel is
  first in, first out, so tasks run in the order they arrived, which a lock
  does not promise whoever waits on it. One thread means two writes never run
  side by side, which is the world the commands underneath were written for:
  a CLI, where one command at a time is all there is. It orders the
  panel's own writes and nobody else's: beside a terminal or a TUI writing
  to the same workspace, the two take turns through the workspace's lock
  ([Writers at the same time](#writers-at-the-same-time)). And a long run
  holds that thread rather than one of the pool's, so the panel goes on
  reading while it waits.

A run's lines are worked out on the writer thread just before it, against the
disk the run itself meets after every write before it, and the entry is in the
transcript before the reply leaves, so a client that asks for the transcript
after its reply always finds its own run there. A client that stops waiting
does not take its task back: it was queued, and it runs. An outcome that will
not serialize is still a run, so it is recorded, and replied, as
`{"error": ...}` beside the lines that ran rather than turned into a 500 that
leaves the transcript a run short. A backend that panics loses that one reply
but not the thread, so the tasks behind it still run.

The transcript is the TUI's, kept where the panel can ask for it at any time:
runs only, since a query or a preview changes nothing there is to replay, and
in memory only, for as long as the server runs.

### Who it answers

It binds 127.0.0.1 on port 0, with no option for another address: the panel
is for the person at this machine, and beyond loopback the token would travel
in clear text. Port 0 lets the system pick a free port rather than the server
failing on a taken one.

Every process on the machine can reach that port, so the address says nothing
about who is asking. A 256-bit token, issued fresh at each launch, does. The
launch link carries it once, as `/?token=...`; that request is answered with
a 303 to `/` and the token in a cookie, and the redirect is what takes the
token out of the address bar and the history. The cookie is `HttpOnly`, so the
panel's own scripts never hold it, and `SameSite=Strict`, so no other site's
page sends it, though a page on another port is the same site, which is what
[the guards](#what-keeps-other-pages-out) are for. It is not `Secure`, which a
browser that does not count 127.0.0.1 as a secure origin would drop. And it is
named for the port, `mf_<port>`, because cookies are not isolated by port:
under one name, a panel launched on a second port would overwrite the first
one's token in the browser and lock it out.

Scripts and tests send `Authorization: Bearer <token>` instead, and either is
compared in constant time. Only `/api` asks for it: the panel's files are the
same for everybody, and a browser that has not been through the link yet still
needs the page that will tell it so.

### What keeps other pages out

The token stops a process that connects to the port without it. It does not
stop another page in the browser that holds the cookie, and the panel is
opened in a browser full of them. Three kinds of page can reach a loopback
port, and a different check stops each:

- **A remote page**, any site open in another tab. It can make the browser
  send 127.0.0.1 a GET, a form or a `text/plain` POST without asking, though
  not read the answer, and the cookie, being `SameSite=Strict`, does not go
  with it. Where the browser says which page sent it, its Origin is refused
  before the token is even looked for; where it does not, as on a GET, there
  is no cookie to stand in for the token.
- **A rebound name.** A page on `attacker.example` has its DNS answer changed
  to 127.0.0.1 once it has loaded. To the browser it is still
  `attacker.example`, so its requests here count as same-origin and the
  same-origin policy no longer hides the answers from it. It holds neither the
  cookie, which belongs to `127.0.0.1`, nor the token, so the API would refuse
  it anyway; but it names `attacker.example:<port>` in Host, and for that
  every request it makes, the panel's files and the launch link included, is
  refused before anything else.
- **Another port on loopback**: a dev server, a notebook, a tool's sign-in
  callback, anything else listening on 127.0.0.1. Cookies are not isolated by
  port ([RFC 6265, section 8.5](https://www.rfc-editor.org/rfc/rfc6265#section-8.5)),
  and `SameSite` counts sites, not origins, so the browser sends `mf_<port>`
  with that page's requests here. It cannot read the answers, since nothing
  here grants it CORS, but a change does its harm whether or not anybody
  reads the reply. That is cross-site request forgery from the same site, and
  the checks on a change by cookie are for it.

The checks, in the order a request meets them (`src/guard.rs`, and the gate
in `src/auth.rs`):

1. **Host**, on every request: exactly one Host header, naming
   `127.0.0.1:<port>`, `localhost:<port>` or `[::1]:<port>` in any case, or a
   403. The raw header and never `X-Forwarded-Host`, since no proxy stands in
   front to have forwarded anything. An allow-list of loopback's own names,
   because a deny-list would have to guess every name an attacker can buy.
   No browser sends `[::1]` yet: the listener binds only 127.0.0.1, so a
   connection to ::1 is refused before any Host is sent. It is on the list
   because accepting it costs nothing, and a server that also bound ::1
   would then need no change here.
2. **Origin**, on every request that sends one: `http://` and the name in
   Host, or a 403. That one origin, not any of the three: a page loaded as
   `localhost` that asks `127.0.0.1` is another origin to the browser, and
   `null`, from a sandboxed frame or a file, is no origin of the server's.
   Its absence proves nothing, since a browser leaves it off a same-origin GET
   and a script off everything. `flayer ui --dev` (#68) is to accept exactly
   one more, the panel's dev server's (an [open question](#open-questions)).
3. **The token**, under `/api` only: a 401 without it.
4. **A change by cookie** — any method but GET and HEAD, on a request whose
   token came as the cookie — must carry `Sec-Fetch-Site: same-origin` (or a
   403), `X-Mindflayer: 1` (or a 403) and `Content-Type: application/json`
   (or a 415). By method rather than by endpoint, so an endpoint added later
   is covered without being listed; `POST /api/query` is held to it as well,
   at no cost, because the panel sends every POST alike.

Fetch Metadata answers the third kind of page directly: the browser writes
`Sec-Fetch-Site` itself, no page can change it, and it says `same-site` for a
page on another port. Browsers send it only to a potentially trustworthy URL,
which each of loopback's three names is, and the Host check admits no other.
The custom header and the JSON body say the same by other means. A page may
send another origin a form, `text/plain` or multipart unasked, but any other
content type, or a header of its own, needs a CORS preflight first, which
nothing here answers, so the request never leaves the browser. Either alone
would do in a browser that keeps to the rules; both are there for one that
gets a rule wrong.

A change with no `Sec-Fetch-Site` is taken only with a Bearer token. Chrome,
Firefox and Safari have all sent the header since 2023, so its absence means
a script, and a script that belongs here holds the token: the cookie is
`HttpOnly`, so not even the panel's own scripts can read it, and no page can
put an `Authorization` header on a request to another origin without the
preflight it never passes. Of these checks, a Bearer request meets only Host
and Origin. A browser too old for Fetch Metadata can read the panel but not
change anything through it, which is the right way round to fail.

So the panel (#69) fetches its own origin, letting the browser send the
cookie, and sends every POST with `Content-Type: application/json` and
`X-Mindflayer: 1`.

Every answer carries `Content-Security-Policy: default-src 'self';
frame-ancestors 'none'` and `X-Content-Type-Options: nosniff`. The policy
loads nothing from another origin and runs no inline script, so markup that
reached the page by some mistake, a gathered skill's say, could not bring a
script with it; `script-src` is left to `default-src` and never allows
`'unsafe-inline'`. No page can frame the panel to steer a click onto one of
its buttons. `nosniff` holds the browser to each answer's Content-Type, so a
page on another port that loads `/api/transcript` with a `<script>` tag has
it refused rather than run. And there is no CORS layer at all: no answer
grants another origin anything, and a preflight is never passed.

What none of this stops is the server behind such a page. The browser sends
`mf_<port>` to every port of 127.0.0.1, since cookies are not isolated by port
and those requests are same-site, so any server there that the browser sends
a request to while the cookie is set receives the token in its Cookie header:
a page of it the user opens, or anything a page on 127.0.0.1 loads from it.
That server can then send the token as Bearer, with the Host a script writes
and no Origin, and pass every check above. So the guards keep out a benign
server's hostile pages, not a hostile server, which on a machine one person
uses is theirs, and on a shared machine may be another user's (an
[open question](#open-questions)). Such a server can also set cookies for
127.0.0.1, overwriting `mf_<port>` and locking the panel out until its link is
opened again; a cookie it adds under another path is no worse, because the
gate takes the token from whichever `mf_<port>` holds it.

### The log

The server prints nothing of its own accord. A front end that wants a log hands
`Server::log` a function, and gets a line per request once it is answered —
`GET /api/transcript 200`: the method, the path and query, the status. Never a
header, since the cookie and the Bearer token travel in them, and never a body.
The value of a `token` parameter is replaced by `<redacted>` however its name
is encoded, decoded the way the exchange decodes it to find it; and the token
is scrubbed from anywhere else in the line, upper-cased or pasted into a path,
because there it is still the secret. axum's `tracing` feature is off, so
nothing logs on the server's behalf.

### The panel's files

The panel is embedded with rust-embed from `apps/web/dist`, spelled
`$CARGO_MANIFEST_DIR/../web/dist`, so the binary stays the one thing to
install. `allow_missing` lets the crate build, and its tests pass, on a
machine without Node where nothing built the panel; the server then answers
every page with a 503 saying `panel not built: run make web/build`. #69 brings
`apps/web` and that target.

Files under `assets/` are named by Vite for their contents, so they are sent
as immutable, to be kept for a year; everything else, `index.html` above all,
is `no-cache`, so a rebuild shows on the next load. A path outside `/api` that
names no file gets `index.html`, because the panel draws its own routes in the
browser — except under `assets/`, where a missing file is one an old page asks
for after a rebuild, and a 404 is easier to read than a page delivered as a
script. A test hands the server a panel of its own (`Panel::embedded`, any
folder rust-embed can embed), so the suites cover all of this without a build.

Only a name Vite could have written reaches the embed. A debug build's
rust-embed reads the folder from disk, where it turns `\` into `/`, lets an
absolute path or a drive letter replace the folder, and follows a symlink out
of it; without a check, a request with no token could read whatever a symlink
anywhere on disk leads to. So a path holding a backslash or a dot segment,
which no browser sends, is a 404, and one with a drive letter or an empty
segment is never looked up and gets `index.html`, as one of the panel's routes
may. A release build only looks names up in a table, and the check keeps the
two answering alike.

## Which workspace a command uses

One function in core decides, `resolve`
([D-08](#d-08-an-explicit-or-activated-workspace-wins)), and every front end
feeds it: each command, and the TUI's screens. In order:

1. the workspace `-W DIR` names, by its root or by its `.mindflayer`;
2. the one `$MINDFLAYER_WORKSPACE` activates, which `flayer activate` is to
   set (#55);
3. the first one found walking up from where the command starts: the
   directory for a `flayer` command, the project's root for a `mind` one;
4. the default one, in home.

It hands back the workspace with the `Reason` that chose it (`Explicit`,
`Activated`, `Above`, `Home`), so a front end that says why it is showing a
workspace asks the answer rather than working it out from paths. The TUI's
header says there is no workspace here or above only for `Home`: the
directory home is can also be named with `-W`, activated, or reached walking
up from below it. Which workspace a command changed is another question,
about the workspace rather than the rule, and `load` and `unload` answer it
from the path (`FlayerWorkspace::is_default`). They call the one in home the
default one however it was reached, so their line reads the same on the run
that makes it, from anywhere, as on every run after, which from below home
finds it walking up.

The first two are choices somebody made on purpose, so they win over where
somebody happens to stand. Activating a workspace is meant to override
location, as activating a Python virtualenv does; with the walk up first, an
activated workspace would lose, silently, to any workspace above the current
directory. A choice that does not open as a workspace is an error and never a
reason to carry on down the list, which would act on a workspace nobody chose
and say nothing about it. Nor is a chosen directory made into a workspace for
the occasion, a write included: `flayer init` makes workspaces, where it runs.

`-W` given at all is the whole explicit answer. Given empty, it names none and
also keeps the activated one out, as `--home=` keeps the default out; that is
what lets the test helpers pin `--workspace=`, so no test picks up what the
developer's shell activated. An empty `$MINDFLAYER_WORKSPACE` names none, as
an empty `$MINDFLAYER_HOME` does. Both are read by the front end, as home is
(`Lookup::given` in the CLI), and handed to core in `Inputs`, so core never
reads the environment. `--home` and `$MINDFLAYER_HOME` keep meaning only
where the default lives: reusing them to choose a workspace would confuse
where the default is with which workspace to use. A relative `-W` is taken
from where the command was run, as `--home` is, rather than from `-C`, which
moves where a command works and not what it was told.

### The default, made only by a write

When nothing chooses a workspace and nothing up the tree has one, a command
falls back to the one at the default home. Home sits above most of what a
person works on, so it is the one directory where a workspace can manage
projects anywhere, and making it on demand means `flayer link ~/code/x` works
before anyone has chosen a directory.

It is made by the first command that **writes** to it, and by nothing that
only reads, and the caller says which with an `Intent`. `flayer link`, `add`,
`gather git`, `load`, `install` and `uninstall` resolve with `Intent::Write`,
which makes `~/.mindflayer` when nothing up the tree has one. A read resolves
with `Intent::Read`, which falls back instead to `FlayerWorkspace::unmade`:
the workspace `init` would make there, parsed from the same template rather
than written. That is `flayer list`, `show`, `validate`, `scan`, `templates`,
`gather list`, and `load` and `install` on their own.

A read of the unmade workspace answers as a read of one made a moment before
would, because it is the same marker: `flayer list` says it manages no
projects yet, `flayer scan` lists what is under home, all of it unlinked,
`flayer gather list` says nothing is gathered. Then one `hint:` line on stderr
says the workspace is not made yet and where the first command that writes
will make it. A hint, not a `warning:`: nothing is wrong with a workspace
nobody has needed yet, so the exit is whatever the read alone makes it. The
alternative was an empty answer written for each command, which is one more
thing to keep in step with what the made workspace says, and every front end
would have to write its own.

The TUI and the minds screen open on it the same way: empty, under a header
saying it is not made yet. The first change made from either runs `flayer
init` before its own lines (`App::lines`), so the workspace is made where the
screen shows it and the transcript says so. Left to the change, the command
that writes would make it, as it does from the shell; but the screen's lines
run from the workspace root and walk up from there, and before it is made no
marker at the root stops them, so with a workspace above home the change would
land in that one. `-W` cannot name it instead: a directory with no marker is
refused, not made. For the same reason the minds screen is built from where
`flayer minds` was run, as the TUI is, and the TUI opens its install screen on
the root it shows rather than walking up from it. The install screen itself
does not open on an unmade workspace: like any workspace with no projects, it
says there are none to install into.

Nothing is written through an unmade workspace. An edit of its marker is
refused as it is for any directory that is not a workspace, and so is opening
its ledger to write to, which would otherwise leave a `.mindflayer` holding a
database and no marker. A command that writes asks for the real one.

`unload`, which only takes away, resolves as a read and wants a made
workspace, since an unmade one has loaded nothing.

### A project's workspace

A `mind` command resolves from the project's root, not from the directory it
was run in, because a project's workspace is the one above the project: core
moves the start there itself (`resolve::managing` and `resolve::consulted`).
`mind link` and `unlink` register the project with the workspace resolved;
`install` and `uninstall` need that workspace to link the project, and are
refused (`NotManaged`) when it does not (`managing`); `add`, `edit`, `rename`,
`remove` and `templates` consult it, to drop the record of what they change
and to lend templates, and only if it is made (`consulted`): a change to a
project is no reason to write a marker into somebody's home, and a workspace
made just now would hold nothing to drop or lend. One found above that
cannot be read is a `warning:` after the change, since the change is the
project's and the workspace was only to be told; one named or activated
that will not open refuses it, as it refuses every other command, because
whoever chose it meant that one to be told.

With `-W` or an activation, that is the chosen workspace wherever it sits,
which is how a project linked from beside its workspace is managed from
inside the project: walking up never reaches such a workspace. Which of a
workspace's projects `-p` names is core's too (`FlayerWorkspace::pick`, and
`holder` for the workspace's own when nothing is named).

`load` resolves with `resolve_except`, which passes over the workspaces being
loaded on the walk up and at home (see
[Loading](#loading-a-third-place-read-live)). A workspace named or activated
is never passed over: it was chosen, and a workspace loading itself is
refused where loads are.

### The TUI looks where it was told

The TUI resolves through the same function. `tui::load` and `load_project`
ask `resolve` and `consulted` with the `Lookup` the screen was opened with,
and `tui::execute` hands that lookup to every line the screen runs, as it
hands them home, rather than letting each line read the environment again: a
screen opened with `-W` or an activation runs every line against that
workspace. A `-W` among a line's own words wins, and is typed from the root
the lines run from, as a `-C` is.

A project's row runs `mind -C <entry>`, the line somebody in the project would
type, which consults the project's own workspace. Where that is not the
workspace on screen, because the project is linked from outside its tree or
from above a nearer workspace, the row runs `mind -W . -C <entry>` instead
(`Holder::Elsewhere`), so a change drops the record of the workspace on screen
and the add form offers that workspace's templates, which are the ones its
line finds. The `-W` is there only where leaving it out would consult another
workspace, so every other row still teaches the plain project command; a
`flayer <cmd> -p <entry>` line would have consulted the same workspace, at the
price of a project's row no longer teaching a project's command.

### A read leaves the ledger as it finds it

`mindflayer.db` belongs to the workspace like its marker, and a read does not
make it either. `FlayerWorkspace::ledger_for_reading` opens it only when it
exists (`Ledger::open_existing`, without SQLite's create flag) and otherwise
answers from an empty ledger in memory, which says what a ledger nothing was
ever written to would. So `flayer gather list`, the TUI's shelf count and the
install screen leave a workspace that has never gathered or installed anything
without one. The install screen is a read until something on it is applied;
applying opens the ledger to write (`ledger_to_write`, through
`install::commit`), which makes it, and never the stand-in, whose records
would be gone when it was dropped. Nor does a read scrub an existing ledger of
what an older build stored with a credential, which waits for the next
command that writes ([A URL is kept without its
credentials](#a-url-is-kept-without-its-credentials)).

A read does write to one thing: an existing ledger from an older schema is
migrated to this build's, as every open migrates it. A build reads its own
schema and no other, so declining would leave the file unreadable rather than
untouched; that step is upkeep of Mindflayer's own file, not a change anybody
asked for. Every migration copies the database aside before its first step
([Why the ledger is SQLite](#why-the-ledger-is-sqlite)), and a read migrates
through the same `Ledger::migrate`, so it leaves that copy too: the first
`flayer gather list` after an upgrade puts `mindflayer.db.v2.bak` beside a
ledger version 2 wrote.

## The agents' own folders

Checks to come read what an agent reads for every project, not only what a
project holds: Claude's user memory, Codex's config, OpenCode's configuration
folder and the skills in `~/.agents` (#44, #91, #96). Those folders come in
the way home does. `--claude-config-dir`, `--codex-home`,
`--opencode-config-dir` and `--agents-dir` are global options on both parsers,
and `homes::agent_homes` takes each from its option, then from the variable
the agent itself reads (`$CLAUDE_CONFIG_DIR`, `$CODEX_HOME`,
`$OPENCODE_CONFIG_DIR`), then from its folder in the user's home (`~/.claude`,
`~/.codex`, `~/.config/opencode`, `~/.agents`). `~/.agents` has no variable:
it belongs to no one agent, so none of their variables moves it. Core takes
the answer as an `AgentHomes` and never resolves one.

For OpenCode that order is a simplification: its documentation reads
`$OPENCODE_CONFIG_DIR` as a folder loaded on top of `~/.config/opencode`, not
one that replaces it. Nothing reads either yet, so the simpler shape stands
until #17 checks the binary; the [open questions](#open-questions) say what
changes if it is confirmed.

Those folder names are core's all the same (`AgentHomes::defaults_under`),
the way `.mindflayer` is: the CLI finds the user's home and core says which
folder in it each agent reads. Which folder an agent reads is a fact about
the agent, and spelling it in the CLI would leave a second front end to copy
it and, one day, to disagree. The variable names stay in the CLI, the only
one that reads them.

They are options rather than variables read where a check needs them, for the
reasons `--home` is one: core reads no environment, and a test cannot safely
set a variable, because every other test in the process would see it. An
option is pinned per command line, so `common::mind` and `common::flayer` give
all four empty and no test can read a developer's real `~/.claude`. A test that
spawns a binary has no parser to pin through, so it clears the variables and
points `HOME` at a temp dir instead.

Empty means none, not "unset, so use the default", because that is what makes
pinning work: an empty option has to switch the lookup off rather than fall
through to the environment running the tests. An empty variable means none
too, as an empty `$MINDFLAYER_HOME` does; whether the agents agree is an [open
question](#open-questions).

No command reads them yet, so none takes them, and `agent_homes` is first
called by the first one that does. That one also has to make the TUI pass its
own to every command it runs, as `tui::execute` passes `--home`. Otherwise a
command run from the TUI decides them again from the environment, and a test
that drives the TUI reaches the real folders.

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
manages and for its own artifacts; of two templates of one kind and name, the
closer wins.

Which workspace that is, the command says. Core takes it as a parameter
(`template::templates`, `template::find`) rather than walking up from the
project, because a walk misses a workspace that links the project from beside
it, and `flayer add -p` run there would not find the workspace's own
templates. `flayer` passes the workspace it runs in. `mind` passes what
`resolve::consulted` resolves from the project: the one `-W` names, then the
activated one, then the one above the project, then the default one in your
home if it has been made, without making anything. `-W` is how a workspace
beside the project lends it templates from inside it. The TUI's add form
offers the templates its line will find. On a workspace's screen those are
the workspace on screen's: a row whose project would consult another runs
`mind -W . -C <entry>` to reach it (see [the TUI looks where it was
told](#the-tui-looks-where-it-was-told)). Core still checks that the workspace manages the project, so one
that does not lends it nothing.

A workspace `mind` cannot read refuses `mind add --template`, since passing it
over would let a farther template of the same name, a built-in one say, stand
in for its own. For `edit`, `rename`, `remove`, an add from nothing and `mind
templates`, one found above that cannot be read is a warning after the
change, since there the workspace is only told, and `mind templates` lists
what it could read. One named or activated that will not open refuses all of
them, as it refuses every other command (see [a project's
workspace](#a-projects-workspace)).

A skill template's front matter keeps whatever it says except `name` and
`description`, which `mind add` always sets: the template's lines for those
two are dropped, continuation lines and all, before anything parses the rest —
so a template may write `name: {{name}}`, which is not YAML anybody could read,
and still work. The result is parsed back before the first file is written, so
a template with broken front matter costs nothing but the error.

A rule template is filled in throughout, front matter included, so it may
scope its rule to `{{name}}/**`. Whether its fences hold front matter is
decided as for any rule, but on what they hold once filled in, since
`{{name}}/**` is a YAML flow mapping until it is. A `description` the front
matter declares is set as a skill's is, dropped and serialized afresh: pasted
in as text, a description breaks the YAML at its first `: `, and one opening
with `#` is a comment that declares nothing. The result is read back before
anything is written and refused for whatever `validate` would warn about
(`artifact::rule_issues`) — front matter that does not parse, a `paths` or a
`description` it cannot read. Reading a rule takes those as content or passes
over them, because a rule somebody wrote still says what it says; writing one
is the moment to find out, and a template's `paths:` that became content would
scope nothing and say so nowhere.

A rule template that does not say where `{{description}}` goes gets it as a
heading on top of its body, below any front matter it declares. On top of the
file, the heading would push the fences down a line, where they stop being
front matter.

### Finding what to change

`edit`, `rename` and `remove` find an artifact by what it declares, and then by
where its name would put it on disk (`lifecycle::find`). The second is what
lets a skill whose front matter does not parse be opened and fixed: it is left
out of every listing, and it is the one most in need of opening. Only a name
made of plain folder names (`install::is_folder_name`, segment by segment) is
turned into a place, so `../x` cannot reach out of the project. Not the naming
rules: they are for what Mindflayer writes, and a folder from before they were
tightened — `a--b` — or one made by hand is exactly what has to be found to be
fixed. A template is looked up by one folder name for the same reason, since
`mind templates` lists whatever is there. More than one match is an error: changing the wrong artifact is
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
would then delete somebody's work because a box looked untidy. So
`lifecycle::remove`, `lifecycle::rename` and `lifecycle::finish_edit` drop the
record, and `lifecycle::add` drops any stale one under the name it writes, as
`lifecycle::rename` does under the new name. An installed skill that is edited
becomes its author's.

Core applies that rule inside each change. Dropping the record in the CLI
after the change, as it once did, put a rule outside core, and
[D-18](#d-18-the-panel-writes-by-running-commands) asks for the orchestration
left in the CLI to move into core before the panel runs its writes through
the CLI. Installed rules and MCP entries, when they come, would each have
needed calls of their own there, and each could have got them wrong. Each
change returns what it did to the record (`lifecycle::Record`), with what
opening the ledger to do it scrubbed out ([A URL is kept without its
credentials](#a-url-is-kept-without-its-credentials)), and the CLI only says
so; `install::disown`, which does the dropping, is private to core so that
nothing else can. That keeps a front end from applying the rule
differently, not from skipping it: `create` and `create_from`, the writers
`lifecycle::add` is built on, see to no record and stay public for core's
tests, so a front end adds through `add`. A record that cannot be dropped is
reported beside the change rather than instead of it, and the CLI prints it
as a warning: the change has been made, and saying it was not would be false.

Editing is split in two, because the editor is not core's to run: which one
comes from `$VISUAL` or `$EDITOR`, and core never reads the environment.
`lifecycle::start_edit` notes what the file holds, the CLI runs the editor,
and `lifecycle::finish_edit` decides whether anything changed and what that
means for the record. The CLI finishes the edit even when the editor fails:
one that writes the file and then exits non-zero, as vim does on `:cq`, has
changed the copy all the same, and a record left behind would let `flayer
uninstall` delete the change. What dropping it scrubbed out of the ledger goes
with the editor's error, said there or never.

The record dropped is the one held by the workspace the command resolved, the
one its templates come from, and each change is given it for the same reason:
a workspace beside the project, which a walk up from the project never
reaches, would keep a record of a copy that has changed. A change takes that
workspace rather than its ledger, because opening a ledger creates one, and a
workspace that never installed anything must not get a database because
something in a project changed; core opens it only when it is there. Another
workspace that links the same project keeps its own, an [open
question](#open-questions).

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
complete. A folder that cannot be listed is a failure reported beside what was
found, except a root that is not there at all: that holds nothing, and only
the default workspace before it is made can have one, in a home nobody has
created yet.

## Writers at the same time

The commands were written for a person at one terminal, running one at a time.
The TUI runs them while another terminal runs its own, and the server under the
panel runs them while both do. Three things keep two writers from undoing each
other, and none of them makes a reader wait.

- **A temporary of its own.** A file Mindflayer replaces — a marker, the
  `SKILL.md` a rename rewrites, git's exclude file — is written whole into a
  temporary beside it and renamed into place (`replace_file`). The temporary
  was `flayer.toml.tmp`, one name for every writer: two at once wrote into one
  file, either could rename the other's half into place, and the second rename
  found nothing to move. It is named for the run now,
  `flayer.toml.<pid>-<suffix>.tmp` as [a stage](#built-where-no-agent-looks)
  is, made with `create_new`, and taken away when the write or the rename
  fails.
- **A ledger that waits, and keeps a log.** `Ledger::open` gives its
  connection SQLite's busy timeout, as long as a writer waits for the
  workspace, so a second writer waits for the first rather than failing with
  "database is locked". rusqlite sets five seconds of its own accord, but as a
  library's default; set there, the wait is Mindflayer's. It also turns on
  write-ahead logging: a reader goes on reading what the last write left while
  the next is made, and a writer does not wait for readers. The mode is kept in
  the file, so a ledger switches once, at the first open to write, and SQLite
  keeps `mindflayer.db-wal` and `mindflayer.db-shm` beside it while it is open.
  `open_existing` sets the timeout and not the mode, because switching is a
  write, and a read makes none but a migration.
- **A lock on the workspace.** Unique names keep each file whole, not each
  entry in it: two `flayer link`s that both read `flayer.toml` before either
  writes each put back a marker with only their own entry, and the second
  rename wins. So a writer holds `.mindflayer/lock` from before it reads what
  it will change until the change is written: an edit of the marker (`link`,
  `unlink`, `load`, `unload`), an install or an uninstall, a gather — clone and
  all, since two gathers of one source clone into one folder — and opening the
  ledger to write, which is when it is made or migrated. Core takes it inside
  each of them (`FlayerWorkspace::lock`), so no front end has to remember to.

How the lock behaves, and why:

- **The system's lock, as a stage's is.** `std`'s `File::lock`, which is
  `flock` on Linux and macOS and `LockFileEx` on Windows. The system drops it
  when its process dies, however it dies, so a writer that was killed leaves
  nothing to clear, and nothing has to decide whether a process named in a
  file is still running. The file itself is never deleted, not even by the
  writer that made it: a writer waiting on the file it opened would then lock
  one nobody else can see, beside the one the next writer makes, and the two
  would write at once.
- **A bounded wait.** A writer that finds the lock held tries again, every
  few milliseconds at first and every tenth of a second at most, for
  `lock::WAIT`: polled, because `std` cannot wait for a lock with a deadline,
  and a thread blocked in `lock` could not be told to stop. Then it gives up
  with `LockError::Busy`, which names whoever holds it:

  ```
  /home/you/Projects/.mindflayer/lock: workspace busy: waited 10s for process 3234405 to finish writing to it; try again once it has
  ```

  Ten seconds is longer than anything but a gather takes, and short enough
  that a command kept waiting by a process that has stopped answering says
  so rather than looks hung. The error is collected like any other: the CLI
  prints it, and the install screen prints it for the row it was applying
  and goes on to the next.
- **Who holds it, in a file of its own.** The writer holding the lock writes
  its process id to `.mindflayer/lock.pid`, and takes it away while it still
  holds the lock, so it never names a writer that has let go. Not in `lock`
  itself, because a lock is mandatory on Windows: while one process holds a
  file locked, no other can read a byte of it, and the id is wanted by exactly
  the processes that cannot have the lock. Writing it is best effort, and a
  waiting writer that finds none says "another process".
- **Readers never take it.** A marker is replaced by a rename and the ledger
  keeps its log, so a read finds what was there before a write or what is
  there after it, never part of one, and has nothing to wait for.
  `ledger_for_reading` migrates an older ledger as `ledger` does, but without
  the lock, so a read's migration is kept apart from a writer's by SQLite's
  locking alone ([open questions](#open-questions)).
- **Not re-entrant.** Two handles on the file are two writers even in one
  process, which is what lets threads stand in for processes in the tests,
  from a link on each of two threads at once to every writer waiting for a
  lock the test holds. It also means a writer that asks for the lock while it
  holds it waits for itself, and fails as busy. Core's writers each take it
  once and call none of the others while they hold it, and a front end does
  not take it at all.
- **A filesystem without locks lets writers through unlocked**, as it lets
  installs through without a sweep: refusing would refuse every change there.
  Any error taking the lock counts, for the reason a stage gives — NFS
  without its lock daemon says "no locks available". The unique names still
  keep every file whole.

Writers on two machines are out of reach, and not attempted. On a network
filesystem `flock` is emulated with locks that belong to a process rather than
to a handle, and the write-ahead log needs every process using the ledger to
share memory on one machine. A workspace on a network share is safe used from
one machine at a time.

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

Carrying it out, #30 keeps the hash once per installation, on
`installations`, rather than once per target: every copy D-07 writes is
staged from one tree, so each is compared with the same hash, and an edited
copy is the one whose own hash no longer matches it. A target whose content
differs from its installation's in form — a rule's section of the managed
block in `AGENTS.md` beside its `.claude/rules/mindflayer/<route>.md` file
(D-03) — shares no one tree with the others, and would need a hash of its
own on `installation_targets`, added by the schema step that first records
one.

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
minimal. Unblocks #54, #55 and #56. #54 carried it out: core's `resolve`,
with `-W/--workspace` on both binaries (see [which workspace a command
uses](#which-workspace-a-command-uses)).

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
since `codex mcp add` writes only Codex's user config, never a project's, and
re-serializes every entry there, though they may return for user scope (D-22).
Unblocks #18, #86, #89, #93 and #94.

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
- **Gathering still stages beside.** `copy::replace` builds a shelf copy as a
  hidden sibling, `.<name>.mindflayer-new`, under names taken from the skill,
  so a gather stopped halfway leaves its staging on the shelf. Two gathers no
  longer meet there, since each holds the workspace's lock ([Writers at the
  same time](#writers-at-the-same-time)). No agent reads the shelf, which is
  why installs moved first. A `copy::Stage` in a scratch folder of the
  workspace's own would do for gathering what `.mind/tmp` does for installs.
- **Removing is not staged.** `uninstall` deletes a copy where it is, so one
  stopped halfway leaves part of a skill in a folder agents read. Renaming the
  copy into a stage first, and deleting it there, would make a removal as all
  or nothing as an install.
- **A batch takes turns row by row.** The install screen's batch calls core
  once for each row, and each call takes the workspace's lock for itself:
  another writer can come between two rows, and with the workspace busy every
  row waits the whole ten seconds before it is passed over. Holding the lock
  across the batch needs core to apply the batch, which is where #78 plans it.
- **A read migrating beside a writer.** A read migrates an older ledger
  without the workspace's lock, so it can meet a writer migrating the same
  file. A step is several statements, and SQLite orders them one at a time:
  one of the two fails on a table the other has just made, and the other
  finishes the step. A migration made in one transaction that reads the
  version again once it holds SQLite's write lock would make the second wait
  and then find nothing to do, which is how #30 migrates.
- **Changes to an artifact take no lock.** `add`, `edit`, `rename` and
  `remove` write into a project or the workspace's own folders and then drop a
  record ([A record must not outlive the copy it is
  about](#a-record-must-not-outlive-the-copy-it-is-about)). Opening the ledger
  to drop it takes the lock, but the change and the record are not held
  together, so an install of the same name can come between them, and the
  record dropped be that install's.
- **Two first writers making one workspace.** `init` makes a marker with
  `create_new` and then fills it, so a second writer making the same
  workspace at that moment — two first commands in a home with no
  `.mindflayer` yet — finds a marker it cannot read, or fails to create one.
  Writing the marker whole under a name of its own and linking it into place
  would let the second find it whole.
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
  per target. #30 records it, once per installation since every target is
  written from one copy, and #47 reads it, in the lifecycle changes that now
  hold the rule (`lifecycle::finish_edit`, `rename` and `remove`).
- **Old copies of the ledger.** Each migration leaves a
  `mindflayer.db.v<old>.bak` beside the ledger, and nothing removes one: a
  copy is the way back from a step that went wrong, and when it is no longer
  needed is for whoever owns the workspace to say. A command to list and
  remove them, or a doctor check (#96) that mentions them, would make them
  something seen rather than found by browsing `.mindflayer`.
- **What a copy keeps.** A copy is the ledger as it was, so a URL an older
  Mindflayer stored with its credentials in it stays in the copy once the
  scrub has rewritten the ledger itself without them, and the copy is taken
  as the file is opened, before any scrub. Rewriting the copy too, or taking
  it without them, would close that, at the cost of a way back that is no
  longer the file as it was: two of its sources the rewrite makes one would
  have to be merged in the copy, as the scrub merges them in the ledger.
- **Disowning from elsewhere.** A change drops the record of the workspace
  its command resolved, wherever that sits, but not of any other workspace
  that links the same project: nothing in the project says which workspaces
  link it, so each of them keeps its stale record. Every workspace linking a
  project would have to be findable from the project for that to change. The
  TUI no longer runs into it from inside: a project's row whose own workspace
  is not the one on screen runs `mind -W . -C <entry>`, which consults the
  one on screen (see [the TUI looks where it was
  told](#the-tui-looks-where-it-was-told)).
- **A project stored by its absolute path.** The ledger names a project by
  the route from the workspace's root to where its entry leads, whatever the
  command called the project. An entry `link` stored absolute, because no
  route from the root reached it, still gives a route that depends on how the
  root itself was spelled: `/tmp/hub` and `/private/tmp/hub` climb a
  different number of `..`s to it. `-C` could always spell a root two ways;
  `-W` and an activation make it likelier. Keying such a project by its entry
  as stored would settle it, at the price of re-keying the records already
  written under the route.
- **Writes that refuse after making the home workspace.** A command that
  writes makes the default workspace before it finds out whether there is
  anything to do in it. Outside every workspace, `mind install` and `mind
  uninstall` in a project nothing manages, `flayer install -p` and `flayer
  uninstall -p`, and `unlink` at either level leave a `~/.mindflayer` behind
  and then refuse, since a workspace just made manages nothing. `flayer edit`,
  `rename` and `remove` make it to change its own artifacts, `remove`'s dry
  run included. Each could resolve with `Intent::Read` first and make it only
  when it is about to write; whether each should is a choice per command, and
  they were left as writes when the reads stopped making it.
- **Reads from a screen on the unmade default.** The TUI's reads (`v`, `s`,
  and showing an artifact) run from the default workspace's root, as its
  changes do. Before the default is made, with a workspace above home, they
  walk up to that one and read it instead. A change makes the default first;
  a read cannot without making the home workspace to read it. Nor can `-W`
  name it ([D-08](#d-08-an-explicit-or-activated-workspace-wins)): a named
  directory with no marker is refused. A way to say "no walk, only the
  default", or a `-W` that takes the unmade default for a read, would settle
  it.
- **A transcript from a screen opened on a named workspace.** A screen
  opened with `-W` runs every line against that workspace, but its
  transcript does not spell the `-W`, as it does not spell `--home`: the
  lines read as somebody would type them at the root, and replayed in a
  shell without the same `-W` or activation they walk up instead. Writing it
  into every line would make the transcript replayable anywhere, at the
  price of lines nobody would type from inside the workspace.
- **Worktrees share one exclude file.** Every linked worktree of a repository
  reads the same `info/exclude`, and an entry is anchored at whichever work
  tree reads it. A project checked out in two worktrees under one key has one
  group for both, so uninstalling in one stops excluding the copy the other
  still has. A key that names the checkout rather than the project would keep
  them apart; nothing has to choose until #34 installs through it.
- **A project that moves.** Exclude entries are anchored at the route the
  project had when they were written. Moved within its repository, a project
  keeps a group of lines pointing where it was, which removing its entries by
  their new route never matches. Rewriting a project's whole group from what
  the ledger says it has installed would heal it.
- **A team's `.gitignore` outranks the block.** Git ranks every `.gitignore`
  in the tree above `info/exclude`, so a negation there (`!.claude/skills/**`)
  takes back what the block excludes, and a local install shows up in `git
  status` with nothing said: the entry is written all the same, and only
  `Excluded::Outside` warns. Asking gix's `excludes` whether each path is
  ignored once it is written, as the tests do, would let the installing side
  or a doctor check say so.
- **Long work in the TUI.** A gather holds the screen until it finishes, with a
  box saying what is running, and cannot be cancelled. A background thread
  would change that; it has not been needed yet.
- **Gathering rules.** Only skills are gatherable. A rule is a loose file at any
  depth, so harvesting one means deciding what its name is relative to — the
  same question `Catalog::take_files` answers inside a project, asked of a
  folder that is not one.
- **Private repositories.** Mindflayer gives `gix` no credentials of its own,
  so a private source gets through only when a git-config helper (or, for an
  ssh URL, ssh itself) has them, and otherwise the gather fails and says how to
  provide one, never at a prompt (see
  [Why `gix`](#why-gix-rather-than-running-git)). A URL is stored, logged and
  shown without credentials, and a gather URL that carries one is refused, so
  a helper is also the only place a token typed into a URL can go (see
  [A URL is kept without its
  credentials](#a-url-is-kept-without-its-credentials)). What else it should
  be given is decided in
  [D-16](#d-16-credentials-come-from-the-front-end-never-from-a-prompt): a
  provider the front end supplies, through that same callback. #110 carries it
  out, and #109 checks the helpers and ssh-agent against a real private
  repository. ssh's own questions are still open: a passphrase for a key the
  agent does not hold, a password when the server accepts password
  authentication, or whether to trust a host seen for the first time, is asked
  by ssh on the terminal, out of the callback's reach. `BatchMode=yes` in
  the ssh command would make those failures too. One connection never sees
  the callback either: when the remote has a branch named `HEAD`, which
  clashes with the `HEAD` a clone fetches to know what to check out, `gix`
  0.87 retries on a new connection without calling `configure_connection`, and
  that one runs the helpers and then prompts, as `gix` does by default. While
  the helpers are the only source, it asks them what the first connections
  already got answered; once #110 supplies a token through the callback, that
  connection will not have it. An in-memory `gitoxide.credentials.terminalPrompt=false`
  on the clone would keep that prompt off the terminal, though not an askpass
  named by `GIT_ASKPASS` or `SSH_ASKPASS`; a `gix` that configures the retry
  too would close it.
- **When the scrub can stop.** `gather::scrub` runs whenever a command
  opens the ledger to write (`FlayerWorkspace::ledger_to_write`), because an
  older build could write a token into the same file. Schema 3 (#30) changes
  that, since a build older than it refuses the file: the scrub could run
  once, as part of that migration, and stop. It has not moved there, because
  a read migrates too ([A read leaves the ledger as it
  finds it](#a-read-leaves-the-ledger-as-it-finds-it)), so a scrub folded into
  that step would run on a read as well, rewriting clones as well as the file,
  and a read would then have to say what it took out, as only a command that
  writes does now.
- **A user name on its own in an https URL.** #25 allows a bare user name,
  and its examples are ssh's (`git@host:...`, `ssh://git@host/...`). Only ssh
  keeps one here: over http(s) a user name is refused at gather and taken out
  by the scrub, because GitHub takes a token in that place and nothing in the
  text tells a token from a user name. That refuses the https URLs Bitbucket
  Cloud and Azure DevOps hand out (`https://<user>@bitbucket.org/...`), which
  then have to be gathered without the user name, with a credential helper
  naming it. The other reading would let them through and store and show
  them without the user name, which also lets a token typed there through,
  for that one fetch, from the command line. The error and the scrub's note
  say a user name was refused or taken out rather than calling it a password.
  Which reading holds is the owner's to confirm.
- **OpenCode's folder and `$XDG_CONFIG_HOME`.** Without
  `$OPENCODE_CONFIG_DIR`, OpenCode's folder is taken to be
  `~/.config/opencode`. Whether OpenCode moves it with `$XDG_CONFIG_HOME`, as
  programs that follow the XDG base directories do, is unverified. If #17 finds
  that it does, that variable becomes the step before the default for
  `--opencode-config-dir`, and tests that spawn a binary clear it too.
- **`$OPENCODE_CONFIG_DIR` beside OpenCode's folder, not instead of it.**
  `--opencode-config-dir` takes the variable in place of `~/.config/opencode`,
  as `--codex-home` takes `$CODEX_HOME` in place of `~/.codex`. OpenCode's
  documentation says otherwise: the folder the variable names is loaded after
  the global one and the project's `.opencode`, so it can override their
  settings, and the global folder is still read. If #17 confirms that on the
  real binary, a check that reads only one folder misses what OpenCode reads
  from the other whenever the variable is set, and `AgentHomes` needs both
  for OpenCode: `~/.config/opencode`, and the variable's folder on top.
- **An empty agent variable.** An empty `$CLAUDE_CONFIG_DIR`, `$CODEX_HOME` or
  `$OPENCODE_CONFIG_DIR` means no folder at all here, following
  `$MINDFLAYER_HOME`. An agent that reads an empty one as unset uses its
  default instead, and a check would then leave out a folder that agent does
  read. How each agent reads it is for #17 to record.
- **A panel built after the server.** A release build embeds what
  `apps/web/dist` held when `mindflayer-server` last compiled, and a debug
  build reads the folder at run time only if it existed then, because
  rust-embed resolves the folder's real path when it compiles. Neither sees a
  panel built later until the crate compiles again. A build script that reruns
  when the folder changes would, at the price of rerunning on every build
  while the folder is missing, since cargo counts a missing path as changed.
  #68, which puts the panel into the binaries, settles the build order.
- **The panel's transcript ends with the server.** The TUI prints its
  transcript when it closes; the server keeps the panel's in memory and drops
  it when it stops. Whether `flayer ui` prints it on the way out, as the TUI
  does, is for #68 to choose.
- **A style nonce for the panel.** The policy allows no inline style, so a
  `<style>` element that a library writes into the page at run time is
  blocked, as theming and colour-mode helpers often do; style the panel's
  scripts set through the CSSOM, as Vue's bindings do, is not governed by
  it. If #69 finds the panel needs one, the server would issue a nonce with
  each launch, add it to `style-src`, and write it into `index.html` where
  Vite's `html.cspNonce` leaves its placeholder. `script-src` never gets
  `'unsafe-inline'` either way.
- **A hostile server on loopback.** The cookie reaches every port of
  127.0.0.1, so a server there that the browser visits while the panel runs
  receives the token and can use it as Bearer
  ([What keeps other pages out](#what-keeps-other-pages-out)). On a machine
  one person uses, that server is theirs, and a hostile program of theirs
  could read the browser's profile as easily; on a shared machine it can be
  another user's, and nothing here keeps it out. Two changes would. The panel
  could hold the token in memory and send it in a header of its own, which
  goes nowhere the panel does not send it, at the price of the `HttpOnly`
  that keeps the token from the panel's scripts, and of a reload that loses
  it. Or each launch could bind an address of its own in 127.0.0.0/8, so the
  cookie's host is the panel's alone, where the system allows it: Linux and
  Windows answer the whole block, macOS only 127.0.0.1 until an alias is
  added. Either changes the launch #15 validated (127.0.0.1, and a cookie
  named per port), so it is the owner's to choose, or to accept as it is.
- **The panel's dev server.** #68 gives `flayer ui` a `--dev` flag that also
  accepts the Vite origin, and #69 has Vite proxy `/api` to `flayer ui --dev
  --port 7878`. Through that proxy a change arrives with the browser's Origin,
  the dev server's (`http://127.0.0.1:5173`, say), which the Origin check
  refuses like every origin but the server's own. The widening meant for it is
  one exact origin, handed to the server by the front end and accepted only
  under `--dev`, never a pattern. Host stays checked as it is, so the proxy
  sets `changeOrigin` and names the server in Host rather than the dev server.
  Fetch Metadata needs nothing, since the browser sees a same-origin request
  to the dev server. Nor does the gate while the dev server is opened as
  127.0.0.1, where the launch link's cookie reaches it whatever its port;
  opened as `localhost`, it has no cookie to forward.
- **A run that panics.** A backend that panics while running a task has no
  outcome to record, so the task is not in the transcript, though it may have
  changed something before it stopped. The CLI's commands collect their
  failures rather than panic, so this is a bug's path, but the transcript then
  cannot say what the bug left behind.
- **Names outside ASCII.** The specification allows "unicode lowercase
  alphanumeric characters" in a name, while listing `a-z` and `0-9`, and its
  reference validator accepts more than ASCII. Mindflayer holds names to ASCII,
  the narrower reading and the one every agent accepts, so a gathered `café` is
  warned about and not installed. If the agents settle on the wider one, the
  name rule is one function (`is_kebab_case`).
- **The shelf in other reports.** `flayer validate` reads the shelf; `flayer
  list` and `show` do not, because `gather list` is the shelf's listing. Seeing
  a gathered skill's contents before installing it means reading its folder.
