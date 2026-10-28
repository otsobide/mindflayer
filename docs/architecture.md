# Architecture

## The shape of the tree

Every unit of the product is an app under `apps/`, with its own manifest
(`Cargo.toml`, or `package.json` for the panel) and its own `Makefile`. The
root `Makefile` delegates (`make <app>/<target>`) and CI invokes those
targets, so adding a front end is adding a directory, a line in `APPS` and the
workspace `members`, and its test and build jobs in CI, never reshaping the
tree.

```
apps/core    mindflayer-core     the engine
apps/cli     mindflayer-cli      the `mind` and `flayer` binaries
apps/server  mindflayer-server   the local server behind the web panel
apps/web     mindflayer-web      the web panel, in Vue
```

The layout exists because there would be more than two, and the server and
the panel are the apps [D-17](#d-17-the-next-front-end-is-a-local-web-panel)
adds. Both sit beside the CLI rather than around it. The panel is in
`NODE_APPS` rather than `APPS` for now (see [the web panel](#the-web-panel)).

`scripts/agent-probe/` is not an app, and nothing builds or tests it. It is
the probe behind [docs/agents.md](agents.md), run by hand against the real
agents with the operator's own login, which is why CI never runs it.

### The examples are a workspace

`examples/` sits beside `apps/` and is content, not an app: a flayer
workspace (`examples/.mindflayer/flayer.toml`) holding two skills and two
rules of its own, so there is something to install while gathering is
paused. A workspace rather than files the binaries embed or `flayer init`
copies in, because [loading](#loading-a-third-place-read-live) already offers
one's own skills and rules to any other, read live from the clone: the
examples take the path a team's workspace takes, and need no command of their
own.

Its `[directories]` are spelled out, so it reads the same whatever a new
workspace defaults to
([D-05](#d-05-a-workspaces-library-lives-in-mindflayer)). They sit beside the
marker rather than in `.mindflayer/library`, because the examples are meant
to be read where the repository is browsed. No agent working on this
repository reads them there: each looks for skills and rules in a folder of
its own, such as `.claude` or `.agents`, not in one called `skills` or
`rules`.

`apps/cli/tests/examples_test.rs` uses the folder where it is rather than a
copy, since that folder is what a person loads. It holds every example to
`validate` with neither a problem nor a warning, installs `commit-style` with
the reference it carries and both rules where the README says they go, checks
that nothing it ran wrote into the folder, and finds the README's walk-through
in the output of the commands it shows.
An example whose summary changes changes the README with it.

### No em dash in what the repository says

The owner finds the em dash reads as machine-written, so none is written:
not in docs, comments, messages, test expectations or commit messages. A
comma, a colon, parentheses or a new sentence takes its place, rather than a
hyphen or a spaced en dash, which would keep the tic under another glyph.
`make lint` runs `lint/em-dash` before clippy, and it fails on one in any
tracked file. A Makefile target rather than a clippy lint, because the
character turns up in Markdown and in the panel as readily as in Rust. Only
tracked files are searched, since an untracked folder (an agent's worktree,
a scratch clone) is not the repository's text, and git failing is a failure
rather than a clean tree. Commit messages are not checked: the check reads
the tree, the history from before the rule holds some that cannot be
rewritten, and a new one is its author's to watch.

## Where the line between core and a front end falls

`mindflayer-core` knows what a mind project is, what a flayer workspace is, and
what each kind of artifact is. It reads and writes files, and the only other
I/O it does is fetching a repository through `gix` when asked to gather. It has
no idea a terminal exists.

`mindflayer-cli` parses a command line, asks core for values, and renders them.
It holds no rules about artifacts: not the name limits, not the front matter
format, not which folder a kind lives in, not what a change does to the
record of an install.

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
server](#the-local-server). Nor does the panel it serves: it asks through the
server and draws the answers. See [the web panel](#the-web-panel).

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
helpers every report is laid out with and the envelope its JSON goes out in,
in `render.rs`. `CliError` has
`error.rs` to itself, `homes.rs` decides where the default workspace's home and
each agent's own folder are, and gathers them with what `-W` and
`$MINDFLAYER_WORKSPACE` say (`Lookup`), `progress.rs` says what a gather's
steps read as and draws them on stderr, and `lib.rs` only says what is public.

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
disagrees with what was parsed: there is only one. Each payload says what its
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
artifacts; skills are therefore flat. A rule is a loose file, so folders under
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
as "the skill `naming`": a listing printing a name its own `show` rejects, or
worse, resolves to a different artifact. A rules folder grouping rules *about
writing skills* is not an exotic thing to have.

A colon is not a path separator anywhere, and on Windows a filename cannot
contain one at all, so the collision is gone rather than narrowed.

### Where a name comes from

From wherever it is declared. A skill declares one in its front matter, so that
is its name and disagreeing with its directory is a problem `validate` reports.
A rule's front matter has no name in it (a `name:` there passes through like
any key Mindflayer does not read), so its name is its route. This is why the
two are not symmetric, and the asymmetry is the honest one.

### What a listing shows for a rule

A skill has a `description`. A rule has its body: the file, less any front
matter. The body's **opening line** (the first line carrying any text, with
leading `#` stripped) is what a listing shows, and it is captured when the file
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
what keeps `validate` pure: every check it makes was decided when the file was
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

Every other key (Claude Code's `paths` and `disable-model-invocation`, say)
belongs to an agent, and passes through: not modelled, not rejected, and kept
byte for byte by everything that writes a manifest, since `rename` rewrites one
line and gathering and installing copy files. Modelling them would make this
crate the place where three agents' formats are reconciled, which is not a job
anybody asked it to take on.

`allowed-tools` is split on spaces, as the specification writes it, and on
commas, as Claude Code first wrote it and older skills still do, but never
inside parentheses, so a permission pattern such as `Bash(git add *)` stays one
tool. A YAML list is read too.

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
it. Every other key (Cursor's `globs`, a `name`) passes through, as it does on
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
`paths` a rule declares. Claude's rule files are the one place a rule goes
whole, fences and all, since Claude reads `paths` from them ([Claude's rule
files](#claudes-rule-files)). Whether a rule is about some files only is
`RuleManifest::is_scoped`'s to say, `paths` stated in the shape it is read in,
so the block and those files never disagree on it.

### Strict when writing, lenient when reading

`Artifact::validate` gives one list of problems, and it is treated three ways:

- **Refused** when Mindflayer writes a skill into a project, if the name or
  the description breaks the rules. `mind add`, `rename` and `install` all
  ask `artifact::strict_issues`, which keeps what `ValidationIssue::is_strict`
  picks out of that list, for the skill as it is about to be written, so one
  change to the set reaches all three. So does `flayer adopt`, whose library
  copy is a write, and whose project copies become what an install would
  have written ([adopting what a project already
  keeps](#adopting-what-a-project-already-keeps)). `rename` asks too, although the name is
  all it writes, because what it leaves behind is a skill Mindflayer wrote;
  `mind add` checks the name and the description before building anything as
  well, only for messages of its own. Those are what agents refuse: Codex skips a skill whose name is over 64
  characters or whose description is empty, so writing one would put a skill
  in the project that never loads. So is a name that is not the folder the
  skill is filed in, since an agent finds a skill by one or the other (asked
  of the folder Mindflayer is about to write, not the one the files were read
  from). `compatibility` and `metadata` are not refused, because an agent reads
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
  edit the copy, which leaves it edited, never updated again unless forced.
  The TUI's `✗` follows the same line.

### Adding a third kind

A variant on `Kind`, its folder and layout, a variant on `Declared`, a
constructor on `Artifact`, and an arm in every match on the kind (discovery
and discovering the shelf, `create`, `lifecycle::load` and `lifecycle::rename`,
the refusal in `gather`, what `install` holds a copy to, and where the
[target table](#agents-and-where-each-kind-reaches-them) puts it for each
agent), which the compiler lists, since the matches are exhaustive. Reports need nothing:
columns, labels and counts are all driven from `Kind::ALL` and from what the
catalog actually found.

## The two levels

A **mind project** is a directory carrying `.mind`, the way a repository
carries `.git`. Its skills live in `skills/<name>/SKILL.md` by default, beside
the code, and are committed with it, so they travel with the code they
describe. The `.mind` itself is the checkout's: its settings keep themselves
out of git unless the project is shared
([D-01](#d-01-a-projects-mind-ignores-itself)).

A **flayer workspace** carries `.mindflayer` and references the mind projects
it manages, so their skills can be handled together. It is the level above, and
it does not have to be a repository at all: the directory your repos happen to
sit in is the usual case. It also holds skills and rules of its own: the ones
that belong to all of its projects, written once.

Both are identified by a **marker file**, not by the directory alone: a
project by its `settings.toml`, or the `settings.json` or `mind.toml` an older
build wrote, a workspace by its `flayer.toml`. An empty `.mind` left behind by
a failed copy is not a project, and saying so costs an `is_file()` or two
(`MindProject::is_marked`, which everything that looks for a project asks).

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
  what holds it (a project, `workspace` for its own, or `shelf/<source
  folder>` for what `flayer validate` finds on the shelf), because inside a
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
`validate` read everything (the workspace's own and every project's) or one
project with `-p`; `flayer validate` reads the shelf as well, labelling what it
finds there `shelf/<source folder>`. `flayer status` reads every project, or
one with `-p`, and `mind status` the one you are in: not the workspace's own,
which nothing is installed into ([what each project
has](#what-each-project-has)). `mind agents`, `share` and `targets` change
the project you are in, and `flayer agents`, `share` and `targets` the one
`-p` names, which they need: the workspace's own have no agents, and reach
one by being installed.
`mind migrate` moves the project you are in from its `mind.toml` to
settings, and `flayer migrate -p` one the workspace links, or
`--all-projects` every one ([moving a project to its
settings](#moving-a-project-to-its-settings)). `mind update` brings what is
installed in the project you are in up to date, and `flayer update` what is
installed in each project `-p` names, or with `--all` in every one, needing
one or the other, as a change of more than one place says which ([bringing
what was installed up to date](#bringing-what-was-installed-up-to-date)).
`flayer install` and `uninstall` take `-p` once for each project too, or
`--all-projects` for every one, and need one or the other ([one artifact
into many projects](#one-artifact-into-many-projects)); `uninstall
--orphaned`, at either level, takes out what nothing offers any more in
place of one name ([taking out what nothing
offers](#taking-out-what-nothing-offers)). `flayer remove --everywhere`
takes one of the workspace's own out of every project it links before the
library, and has no `mind` form, nor a `-p`: a project's own was installed
nowhere ([removing one of the workspace's own
everywhere](#removing-one-of-the-workspaces-own-everywhere)).
`flayer adopt -p <project> <name>` brings a skill somebody keeps in the
project's agents' folders into the workspace's library and records the
project's copies as installed from there, and has no `mind` form: what it
writes is the workspace's library, which `mind` changes nowhere, as
`flayer add` without `-p` has none ([adopting what a project already
keeps](#adopting-what-a-project-already-keeps)).
`flayer relink <old> <new>` follows a project the workspace links to where it
moved, and has no `mind` form: the project at `old` is gone, and nothing at
`new` is linked yet to say which workspace to tell. `mind init --new-id`
gives the project in the directory it works in an id of its own, as `init`
makes one there rather than in a project above, and has no `flayer` form,
since a copy is most often not linked yet ([an entry whose project
moved](#an-entry-whose-project-moved)).
Reads look everywhere; a change needs one place, and without `-p` that place
is the workspace.

There is one implementation of each. `mind add` and `flayer add -p alpha` are
the same function given the same project, and `flayer add` is that function
given the workspace's own holder, so the levels differ in what they hold and
never in the rules. What only has meaning at one level (the shelf, `gather`,
`scan`) is reachable from the other as `mind flayer <cmd>`, which is the
command itself rather than a copy of it.

### The workspace's own artifacts

They are held the way a project holds its own: in directories the marker
names (`[directories]` in `flayer.toml`), found by the same catalog, created,
renamed and removed by the same functions. `FlayerWorkspace::own` hands the
workspace out as a `MindProject`-shaped holder for exactly that reason:
everything that touches an artifact asks its holder where it is, what it is
called and where each kind lives, and a workspace answers those as a project
does, so there is no second copy of any rule to drift.

Where a project's default is beside the code, a workspace's is its library,
inside `.mindflayer`
([D-05](#d-05-a-workspaces-library-lives-in-mindflayer)):

```toml
[directories]
skills = ".mindflayer/library/skills"
rules = ".mindflayer/library/rules"
```

A project's folders sit where its agents look, because the agents read them.
A workspace's are read by nothing but Mindflayer, which installs from them,
and beside the projects, where they used to be, they put instruction-shaped
files in the directories above projects: `~/skills` and `~/rules`, for the
workspace in a home directory. No agent scans `.mindflayer`, and a workspace
that keeps them there is one folder wherever it is made.

`flayer init` spells both lines out, as `mind init` does. A kind the marker
does not name is in the library too (`Directories::library_for`), never in a
folder at the root, which for the default workspace is the home directory;
that is also what lets a kind added later land in every workspace's library,
whatever its marker says. `own` names those kinds in the holder it hands out,
since a `MindProject` asked about a kind nobody named answers with a folder
beside the code.

A workspace made before the library keeps the folders its marker names,
`skills` and `rules` beside the projects as `flayer init` used to write them,
and is read exactly as before: the default changed, the format did not, so no
version moved. Moving them into the library is for `flayer migrate --library`
to do when asked ([below](#moving-them-into-the-library)), never for a read.

The folders can be anywhere inside the workspace, `.mindflayer` included, but
in what is reserved for gathering. A directory that is the shelf of either kind
or the cache, is inside one, or holds one, as `.mindflayer` itself does, is
refused by `flayer init` before anything is made
(`WorkspaceError::OverlapsGathered`). Gathering replaces and deletes folders
there by names of its own, so an artifact somebody wrote among them could go
with the next gather; and a rules folder holding the cache would read every
markdown file in every clone as one of the workspace's rules. The rules shelf
is reserved too, though nothing gathers rules yet, rather than matched on the
kind as `gather` is: scrubbing a source already deletes its folder from every
kind's shelf, and a kind made gatherable later has to find its shelf free. The
comparison ignores ASCII case, since on macOS and Windows `.Mindflayer/Skills`
is the shelf.

Unlike a project's, the folders are not made by `flayer init`, wherever they
are. A workspace told to keep them beside its repositories puts nothing among
other people's work until there is something to put, and the library follows
the same rule rather than a second one: the first artifact added makes its
folder.

A project holds what is in its own folders, because that is what its agents
read; the workspace's artifacts are not inherited out of sight. They reach a
project by being installed into it (the install screen and `mind install` offer
them beside the shelf) and are then managed there like anything else
Mindflayer installed.

When the workspace's root is itself a project it manages, a folder both keep a
kind in is the project's (`own_kinds`), and a catalog counts each file once
however many holders can see it: otherwise every artifact there would be listed
twice. With the library, the two share a folder only when a marker says so, as
an older workspace's does. Reports name a workspace's own artifact's holder
`workspace`, told by where the file sits rather than by the root it was found
from, since those two share a root.

### Moving them into the library

`flayer migrate --library` moves a workspace's own from the folders its marker
names into `.mindflayer/library/<kind>`, and points `[directories]` there
(`migrate::library`). Without `--yes` it plans, says what would happen and
changes nothing (`migrate::library_plan`). That dry run is a report, and exits
as one, where `remove`'s is a refusal: what it gives is a line per artifact to
read before saying yes, not one deletion that was asked for and did not
happen.

- **What moves.** Every artifact of each kind whose folder is not the
  library's, found the way discovery finds it but without reading it
  (`Catalog::locate`). A skill whose front matter is broken is no less the
  workspace's, and left behind it would drop out of every listing once the
  marker points elsewhere. It goes to the same route below the library's
  folder, by a rename: the library is inside the workspace, so on its
  filesystem but for a mount in between, and a move across two fails naming
  both paths, as an install's does, rather than turning into a copy. A folder
  a move empties goes too, up to and including the kind's, with `remove_dir`,
  which keeps a folder holding anything else: the workspace in a home leaves
  no empty `~/skills`, and a `README.md` beside the skills keeps it.
- **What a project holds stays.** A project the workspace manages, or one at
  its root whether it is linked or not, can keep a kind in a folder the
  workspace names too: a home made a project shares `~/skills` with the
  workspace in it. What such a project's discovery finds there, of any kind,
  stays where it is and is said, and the marker's line moves to the library
  all the same, where the workspace's own of that kind start empty. The root
  counts unlinked because the agents reading `~/skills` do not ask whether
  the workspace was told about the project. `own_kinds` draws that line
  elsewhere, counting what an unlinked root project shares as the
  workspace's own, so such a leftover was listed and offered by the
  workspace and stops being either; the plan marks it (`Left::counted`) and
  its line says so, since an install of it keeps a record no offer matches
  any more (an [open question](#open-questions)).
- **Refused as a whole.** Nothing moves while the plan has any reason not to,
  and every reason is said at once: something already where an artifact would
  go; a copy of the marker an earlier migration kept; a link whose target is
  relative, which a rename to another depth would aim elsewhere; a folder
  overlapping the library, another kind's folder or a project's root, where
  moving one file would carry another, or a project's, with it; a folder that
  cannot be listed; a project whose marker cannot be read and whose root
  holds a folder being moved from, which could keep anything there; a folder
  the marker names outside the workspace or overlapping the shelf or the
  cache, which `open` reads but `flayer init` refuses (`misplaced_own`, its
  check), since what is there is somebody else's, or gathering's; a folder
  that is a link, or is reached through one below the root, since what it
  holds is kept elsewhere on purpose, a dotfiles repository say, and a rename
  would empty that and leave the link aimed at nothing.
- **Planned again under the lock.** It holds the workspace's lock from its
  plan to its rewrite of the marker, and plans from the marker on disk once it
  has the lock, so neither a plan shown a minute before nor a writer in
  between decides what moves. The marker is rewritten with
  `edit_marker_held`, the edit `edit_array` makes once it has taken the lock,
  since the lock is not re-entrant.
- **All or nothing, as far as renames go.** The marker is kept first, beside
  itself as `flayer.toml.v<version>`, named after the version it holds as the
  ledger's copy is, with `create_new`, so a copy an earlier migration kept is
  never written over. Then every artifact is renamed, then the values in
  `[directories]` are rewritten through `toml_edit`, the comments and every
  other byte left alone. A step that fails puts back what had moved, the last
  first, and takes the kept copy away, so the workspace reads as it did; what
  cannot be put back is named (`MigrateError::stranded`).
- **A second run is a no-op.** A kind already in the library plans nothing,
  so a migrated workspace, or one made with its library, writes nothing, not
  even a copy of its marker. No format version moves, for the reason the new
  default needed none
  ([D-05](#d-05-a-workspaces-library-lives-in-mindflayer)): the marker spells
  its folders out.
- **The ledger is not opened.** An installation of one of the workspace's own
  records its origin as `workspace`, which names no folder, and its copies by
  where they are in the project. Nothing in it says where the workspace's own
  were, so every record reads the same once they have moved: an install
  compares the project's copy with the offer, found wherever the marker now
  says, and calls it current when they match. A workspace that loads this one
  names its root, and reads its folders through its marker.
- **Resolved as a read.** Even with `--yes` it finds its workspace with
  `Intent::Read`, so the default one is not made only to find that, being
  new, it keeps its own in its library already.
- **`--library` is required.** A flag per migration, rather than a bare
  `migrate` doing every one there is, so the next one is asked for by name
  and never run by a line typed for this one.

### Where a project keeps its artifacts

`.mind` is the marker and the configuration. The artifacts themselves sit
beside the code, in directories the marker names. A project's settings, the
`settings.toml` `mind init` writes ([a project's
settings](#a-projects-settings)), name them in a table, as a `mind.toml`, the
marker of a project an older Mindflayer made, does:

```toml
[directories]
skills = "skills"
rules = "rules"
```

The `settings.json` between the two named them in an object,
`"directories": {"skills": "skills", "rules": "rules"}`, and is read the way
it was written.

Beside the code rather than inside `.mind`, because these are files the
**agents** read, and an agent looking for skills does not know what a `.mind`
is. Every project already has a place its agents look (`.claude/skills`,
`docs/rules`), and the marker is how a project says which, so Mindflayer
follows the repository rather than the repository following Mindflayer.

Both are keyed by the kind's folder name, which is the plural spelling the
CLI already accepts, so there is no second table to keep in step. A key this
build does not recognise is carried along rather than rejected, for the reason
unknown front matter keys are.

`init` writes every kind's directory even when it is the default. The answer to
"where do this project's skills go" then lives in the file somebody opens
rather than in a function they have to find. In settings it is also the only
way to have the folder at all, since a kind they leave out has none (below), so
`mind init` hands core only the folders it was told about, and core gives each
other kind its default (`MindProject::init_asked`). Which folders a new project
gets is a rule ([D-06](#d-06-projects-keep-authoring-beside-the-code)), and a
front end that filled them in itself could disagree with this one;
`MindProject::init_settings` takes its folders as given, for what needs a
project that keeps none.

A directory has to be inside the project. An absolute path, or one that climbs
out with `..`, describes a project whose artifacts are not the project's, and
that contradicts the one thing a mind project is for. `mind init --skills
/etc/skills` is refused rather than made, before anything is written.

Nor, in settings, inside `.mind` (`InsideMind`), which is Mindflayer's: the
marker, the templates and an install's stage. A local project's keeps itself
out of git, so `mind init --skills .mind/skills` would hide what the project
wrote from its team without a word, where authoring beside the code is so it
is committed with it. A shared project is refused the same, since a change to
its share may make it local later. `init` and `change_settings` refuse it,
and a settings file somebody wrote that way is still read: only a version 1
`mind.toml` kept its kinds inside `.mind` on purpose.

Because this changed where a project's artifacts are, the project marker went
to version 2, and a `mind.toml` written before it is read the way it was
written: version 1 had no such question, so every kind lived inside `.mind`,
and reading one with today's default would point it at directories it never
had and list nothing without saying why. `DIRECTORIES_VERSION` is what draws
that line.

What a kind left out means is the marker's version's to say, and version 3
says something else again. In a `mind.toml` it is the default beside the code;
in a project's settings it is no folder at all, so a kind added later, MCP
servers say, is never given a folder at the project's root that nobody asked
for. `MindProject::directory_for` answers `None` for such a kind. A reader
finds nothing there, an absence and not a failure, as in a folder nobody has
made; what needs the folder (adding an artifact, renaming one) asks
`required_directory`, which refuses with `NoDirectory`. Installing does not
need it: a project marked by its settings is installed into the folders its
agents read, never into the one it authors in ([It only manages what it
installed](#it-only-manages-what-it-installed)), so one naming no folder for
skills takes installs like any other. One naming no agent a kind is sent to
has nowhere to put it, and `install::targets` refuses it with `NoTarget`.
`install::survey` offers such a project nothing rather than refusing it: the
install screen surveys every project the workspace links, so a refusal for
one would hide all the others, and the screen shows that project with core's
line saying why it can take nothing, rather than sending somebody to gather
for it. Naming one skill for it (`install::offered`) or installing one is
refused, since a project was named; uninstalling one goes by its record, so
what was installed while the project named an agent still comes out.
Answering with a default instead was turned down: an `Option` costs every
caller a line, and a default would have put the next kind beside every
project's code. A listing still reads such a project's agents' folders ([read
where its agents
read](#a-project-marked-by-its-settings-is-read-where-its-agents-read)):
keeping no folder of its own is no reason to hide what its agents load.

### A project's settings

From version 5 (`SETTINGS_TOML_VERSION`) a project is marked by
`.mind/settings.toml`, which holds what the hub needs to know about it and
nothing else ([D-06](#d-06-projects-keep-authoring-beside-the-code),
[D-09](#d-09-project-settings-live-in-the-project)), written from a template
whose comments say what each key does (`Settings::text`, [why the markers are
TOML written from a
template](#why-the-markers-are-toml-written-from-a-template)):

```toml
# Mindflayer mind project: what Mindflayer needs to know about the project
# this .mind is in, and nothing else. No agent reads it.
#
# `name` is what the project is shown as, and `agents` the agents that work
# in it (claude, codex, opencode): what is installed into the project is
# copied where each of them reads. `mind agents` changes them.
# ...
version = 5
id = "4f9c2a7e1b3d5f60a8c4e2b6d0f1a3c5"
name = "collapse"
agents = ["claude", "codex", "opencode"]
share = "local"
edit_tracked = false

[directories]
skills = "skills"
rules = "rules"
```

Versions 3 and 4 (`SETTINGS_VERSION`, `TARGETS_VERSION`) held the same keys
as `.mind/settings.json`, in strict JSON, the owner's first choice for
D-06. Such a file is read the way it was written, through the same reader,
and changed by nothing but its move to a `settings.toml` ([moving a project
to its settings](#moving-a-project-to-its-settings)).

The agents that work in the project, by the names `Agent` takes; whether what
is installed into it is the team's or this checkout's
([D-02](#d-02-installs-stay-local-unless-a-project-shares-them)); whether a
team file git tracks may be edited to reach an agent, off unless the file says
so; the folders it authors in; a name; and an id. A project that installs a
kind somewhere other than where its agents read names where in `targets` too,
which a new project leaves out ([a project's own
targets](#a-projects-own-targets)). They describe the files
physically in the project, and several workspaces may link it, so they live
in the project, where every workspace reads the same ones. The id is 128 bits
drawn from the operating system through `std`'s hasher keys, as a stage's name
is, written as 32 lowercase hexadecimal digits: random rather than derived
from the name or the path, which both change, so only a copy of the project
shares one.

Minimal, because `.mind` is meant to stay out of the way (vision point 8). No
agent reads it, so nothing instruction-shaped sits where an agent looks, and
a file of seven keys is one a person reads at a glance and edits by hand,
its comments telling them what each one does.
Anything that is not about the project's own files stays out of it: what was
installed is the ledger's, and which workspaces link the project is theirs.

A local project ignores itself, as a Python venv does
([D-01](#d-01-a-projects-mind-ignores-itself)). `MindProject::init_settings`
writes a `.mind/.gitignore` holding `*` (`MIND_IGNORE`), with `create_new`, so
one somebody changed is left as they left it, and before the settings, so they
never show in `git status`, not even for a moment. The rule lives inside the
folder Mindflayer owns: it needs no repository yet, goes with a copy of the
project and goes when `.mind` does, and no team file is edited for it. A shared
project gets none, and its settings are the team's to commit. A test asks gix,
in process, whether a local project's settings are ignored. A second run, as
for `init`, rewrites nothing and only makes what the marker asks for and is
missing, a deleted `.gitignore` of a local project included; a project an
older marker marks is not given a `settings.toml`, since moving it is `mind
migrate`'s ([moving a project to its
settings](#moving-a-project-to-its-settings)).

`mind init` writes them (#35, a `settings.toml` since #166): every agent
unless `--agents` names fewer, local unless `--shared`, and `skills/` and
`rules/` unless `--skills` or `--rules` says otherwise. Every agent, so a project made without a thought
for it reaches whichever agent is opened there; local, because that is what a
new project is (D-02). The TUI's offer to make a project and the minds
screen run that same `mind init`, so they write the same marker. The CLI's
test makes a fresh repository a project and asks gix, in process, what `git
status` would list: nothing, since `.mind` ignores itself and git lists no
empty folder, until the first artifact is written into `skills/`, which is
there for the team to commit. `MindProject::init`, which writes a `mind.toml`,
is no front end's any more; it stays for the tests that start from a project
an older build made, which is still read the way it was written.

Any of the three files makes a directory a project, and when more than one
is there the newest wins, `settings.toml`, then `settings.json`, then
`mind.toml` (`MindProject::open`): an older one beside it is one an upgrade
left or a copy brought back. Refusing the project would hide every artifact
in it over a stray file, so it is read, and `MindProject::warnings` names
each one ignored ([open questions](#open-questions)). Each file holds
versions of its own, and `MarkerFile::of_version` says which: a `mind.toml`
up to 2, a `settings.json` 3 or 4, a `settings.toml` 5. A version found in
another file, a `mind.toml` claiming 3 or a `settings.toml` claiming 4, is
refused as malformed: what it would mean is nobody's to say. Both settings
files are read into one tree (`config_entry::document`) and held to the keys
by one reader, a key at a time, so the same key is refused the same way in
either.

A key the format does not define is read past and kept, as unknown front
matter is: the format grows, and a project a newer build configured is still a
project. A key that changes what an older build would do with the project is
the exception, and comes with a version of its own, as `targets` came with 4
in a `settings.json`; a `settings.toml` is 5 whatever it sets, since every
build that reads one reads `targets` too. Every key it does define is held to
what it holds: a missing `id`, an agent no `Agent` names, a `share` other than
`local` and `shared`, a `targets` entry that is not a list of folders or names
none, are refused, each named, rather than guessed at. `edit_tracked`,
`directories` and `targets` may be left out, and read as off, as no folders
and as where the agents read. A folder `targets` names that no install may use
is read and said, not refused, since everything else the file says still
stands.

`MindProject::change_settings` changes them, the way `flayer.toml` is
changed: through the same `rewrite`, which reads the file again rather than
trusting what was read when the project was opened, and writes back only
what changed, and through toml_edit (`settings::edited`). Its caller changes
a copy, and only the keys whose values changed are edited, each value written
where the old one stood, keeping the spacing around it and a comment after it
on its line, `directories` and `targets` an entry at a time. The template's
comments, a comment somebody added, a key this build does not know, the
order of the keys and a layout made by hand all survive, and a key the file
left out goes in after the others only when it says something its absence
does not, so turning nothing on writes no `edit_tracked = false`. `targets`
emptied takes its `[targets]` table out, so setting and resetting them gives
the file back byte for byte, unless somebody wrote a comment above the
header, which keeps the header, empty. A changed file goes through a
temporary and a rename; an unchanged one is not written, its modification
time included. The version and the id are refused: a version moves with a
migration, which writes the file its version describes, and an id names the
project for good. So is a folder the change moves out of the project, as
`init` refuses one, or a target no install may use, while one written there
by hand is still read and stops no change to something else.

Only a `settings.toml` is changed (`MindProject::changeable`). A project a
`settings.json` marks is refused as one a `mind.toml` marks is, naming the
file and pointing at `mind migrate`: the settings are edited one way, in
the grammar this build writes, rather than a third way through
`config_entry`, which is for the agents' own files, and the JSON editor and
its rule for the version a change of `targets` brought with it went with
the grammar.

What changes them from a command line is `mind agents` and `mind share`
(#37), and `mind targets` (#40), at either level, and since a project's
agents, targets and share mode decide where what is installed into it goes
and whether git sees it, those carry the installs along too, through
`reapply`, holding the lock of the workspace that links the project ([when a
project's agents, targets or share mode
change](#when-a-projects-agents-targets-or-share-mode-change)).

### A project marked by its settings is read where its agents read

What an agent loads is what sits in its folders, whoever put it there, and a
project marked by its settings says which agents work in it. So discovery
reads it where they read (`Catalog::discover_kinds`): the folder each of its
agents is sent copies in (`MindProject::targets`, the project's target table,
the standard one overridden by its `targets`), and beside them the folder the
project authors in, from `directories`. Reading only the authored
folder, as every project was read before, would list the project's own work
and none of what its agents run: an install's copies, and the skills
somebody already keeps in `.claude/skills` or `.agents/skills` by hand. A
project a `mind.toml` marks names no agents, so it is read from the one folder
its marker names, exactly as before.

- **Each folder once.** An authored folder that is one of the targets,
  `"skills": ".claude/skills"` say, is read once, matched by its spelling and
  then on disk (`same_directory`), so a skill in it is listed once. Whether
  that skill is the project's own or an install's is the ledger's to say,
  below. So are two targets that are one folder on disk, `.claude/skills`
  linked to `.agents/skills` so that Claude and Codex share it, under the
  first spelling: read twice, each failure there would be said twice.
  `Catalog::folders` is the list read, in order, which `list` names when it
  finds nothing.
- **One artifact a name.** Every copy of one name the project holds, in a
  target, a folder one of its agents reads or the folder it authors in, is
  one artifact (`Artifact::copies`), described by its first copy: the
  project's own when it has one, then the rest by path. An installation
  writes one copy per folder ([D-07](#d-07-one-copy-per-agent-folder)), and
  a listing that showed each would show one skill twice, under a name the
  agents see once. The project's own leads because it is what a change from
  the project reaches (below), so `show` prints the file `edit` opens.
- **With the agents that see it.** `Artifact::agents` is every agent the
  project names that reads one of its copies, by `Agent::reads`, which is not
  the target table ([agents, and where each kind reaches
  them](#agents-and-where-each-kind-reaches-them)): OpenCode reads
  `.claude/skills` and is sent a copy only in `.agents/skills`, so a skill in
  `.claude/skills` alone lists with Claude and OpenCode, and one in both with
  all three. An agent the project does not name is left out, since nothing
  of its reaches the project.
- **Copies that differ are said.** Claude reads one standard folder, Codex the
  other, and OpenCode loads whichever it takes of the two (V1 at random), so
  two copies an agent reads that do not hold the same mean the agents do not
  run the same skill. They are compared by `hash::tree`, the hash an install
  records, which refuses a link rather than reading wherever it points, as a
  copy here is anybody's; one folder reached twice, through a link to it, is
  one copy without being read. Differing copies are still one artifact
  (`Artifact::copies_differ`), and a failure, `DiscoveryFailure::Diverged`: a
  `warning:` and a non-zero exit, like anything else discovery could not make
  one thing of. `validate` counts them apart from unreadable files
  (`DiscoveryFailure::is_unreadable`). A copy that cannot be hashed is said
  too (`Uncompared`), and is not taken to differ. Comparing reads every file
  of each copy compared, on every listing, where discovery otherwise reads
  only `SKILL.md` ([an open question](#open-questions)).
- **Each copy is validated.** `Artifact::validate` judges every copy on its
  own file and folder, adding what another has to what the first has, since
  an agent loads the copy in the folder it reads: one copy filed under a
  folder its name does not match fails there and nowhere else.
- **What the project authors where no agent reads is a copy no agent
  sees.** The default `skills/` is read by no agent: what is there is the
  project's own work, which reaches an agent only once installed. A skill
  there is one more copy of its name, with no agent of its own, so the
  agents an entry lists are still only those that load it, and it is
  compared with no other copy: it is the source an install delivers from,
  and a source edited since it was delivered is for `status` to report once
  it tells an outdated copy from a current one (#48), not a warning on every
  listing until it is installed again. Listing it
  apart, as a second entry of the same name, was turned down: a project
  level `list` has no column to tell the two apart, and a screen naming a
  row by its name alone would aim a change at the agents' copy and reach
  the project's own.

Who put a copy there (`Provenance`) is the ledger's, and discovery reads no
ledger: a listing that says nothing of it opens none. `Catalog::with_records`
is handed the workspace's, and marks installed each copy whose path an
installation records (`Ledger::copies`), not each whose name it does, so an
install into one folder never claims what somebody keeps under that name in
another. Unrecorded, a copy in a folder the project authors in is
`Authored`; one only in an agent's folder is `Foreign`, and an artifact is
`Installed` only when every copy is. Records are a workspace's own, so a
project the workspace does not link has none in it, and a ledger that cannot
be read leaves every copy unrecorded, which nothing changes, rather than
installed, which an uninstall may delete. `list`, `show` and `validate`
hand none over, since none says who put what where; `status` does, and the
TUI's rows with it ([what each project has](#what-each-project-has)).

Foreign is never touched. A change to a project's own artifacts looks only in
the folders it authors in (`Catalog::discover_authored`, from
`lifecycle::find`), so `edit`, `rename` and `remove` never reach a copy in an
agent's folder the project does not author in. A name the project authors is
one entry whose first copy is its own, and a change reaches that copy alone;
one found only in an agent's folder is refused as such
(`LifecycleError::NotAuthored`) rather than reported missing, since a listing
has just shown it. Installing writes a copy in each target, and in the
authored folder only when that is one of them, so what a project authors and
what it was installed are apart: a change to the first touches no record of
the second. Settings that author in a target, `"skills": ".claude/skills"`,
put an installed copy where a change reaches it, and that change sees to the
record of that copy alone: kept through an edit, carried through a rename,
dropped by a removal ([A record must not outlive the copy it is
about](#a-record-must-not-outlive-the-copy-it-is-about)).

### Moving a project to its settings

`mind migrate` (#38), or `flayer migrate -p` for a project the workspace
links and `flayer migrate --all-projects` for every one an older marker
marks, moves a project an older Mindflayer made to a `settings.toml`
(`migrate::project`): from a `mind.toml`, with what is installed into it
moved to where its agents read; from a `settings.json`, the same settings in
TOML (below). Without `--yes` it plans, says what would happen and changes
nothing (`migrate::project_plan`), a report as `flayer migrate --library`'s
dry run is.

- **Explicit, never on a read.** Every format here is read the way it was
  written, and moved forward only when somebody asks: a workspace's own by
  `flayer migrate --library`, a ledger only after it is copied aside. A
  project has three reasons more. The move changes what a team committed,
  the `mind.toml` and the installs in `skills/`, which is theirs to see in a
  diff and commit, not something to find after a `mind list`. It needs
  choices only whoever owns the project can make, which agents and whether
  it is shared, and a read would have to guess them. And a build from before
  settings, on a teammate's machine or beside this one, finds no project
  once the `mind.toml` has gone, so moving it is a step a team takes on
  purpose. A read that rewrote a marker would also break what `Intent::Read`
  promises: that a read leaves things as it found them.
- **What the settings say.** The `mind.toml`'s name, and its folders
  written out (`Directories::with_defaults`): a kind it left out had the
  default beside the code, where one settings leave out has none. An id of
  their own. Every agent unless `--agents` names fewer, as `mind init`. And
  shared unless `--local`, as [D-02](#d-02-installs-stay-local-unless-a-project-shares-them)
  has it, because such a project was committed by design; `--shared` says
  it out loud. Written from the template as `mind init` writes them, a
  `settings.toml` straight from a `mind.toml`, with `create_new`. What else
  the `mind.toml` held stays in the copy kept.
- **The marker is kept, in `.mind/tmp`.** Moved to
  `.mind/tmp/mind.toml.v<version>`, named after its version as
  `flayer.toml.v2` is. Not beside the settings, where it would be a
  leftover the project warns about and an older build reads instead; and
  `.mind/tmp` carries its own `.gitignore`, so the kept copy is out of git
  in a shared project too. One already there, kept by an earlier migration,
  refuses the move. Put back in place of `settings.toml`, it is the project
  as it was, though its installs have moved.
- **The installs move as for a change of agents.** It is one, from none to
  the ones asked for, so each recorded installation is brought to the
  targets the settings give through `reapply::retarget`, as `mind agents`
  brings it ([when a project's agents, targets or share mode
  change](#when-a-projects-agents-targets-or-share-mode-change)): a new copy
  is made from a copy that still holds what was installed, never from the
  offer, which may have changed since; an untouched copy in a folder no agent is
  sent copies to, which for such a project is the folder it authors in, is
  taken out; one changed since is left, and its record dropped, its
  author's. A project whose `mind.toml` kept its skills in `.claude/skills`
  keeps its copy there and gains one in `.agents/skills`. Somebody's where a
  copy would go is left, and said. A rule's copy is not moved: a rule
  reaches a project marked by its settings through the block of its
  `AGENTS.md` and Claude's files, which a render writes, so the copy and its
  record are left as they are for the next render of the project's rules,
  which takes the copy out ([installing rules](#installing-rules)).
- **A record from before hashes is held to its offer.** Most of such a
  project's records were written before schema 3, with no hash, and
  [D-04](#d-04-an-edited-copy-is-detected-not-disowned) reads one as current
  only while its copy matches its offer byte for byte. So the migration finds
  the offers (`install::offers`) and hands `retarget` those a record may have
  come from (`install::offered_for`, which tells every record's standing by
  the same answer): by the origin it records, or for a record from before
  origins by its shelf source's id, or else among the workspace's own and the
  loaded ones, any of which may be it. A copy matching one is compared as
  `install::is_current` compares (`copy::same_leaving`), moved, and its record
  given the hash every copy now holds (`Ledger::migrated`), so the next change
  of agents compares it as any other; one matching none is edited; with no
  offer left, it cannot be told, and is left. A change of agents does not look
  for offers, and reads such a record as one it cannot compare.
- **What the project wrote stays.** Every artifact in a folder the settings
  keep naming that no installation records, rules included, is left where
  it is and said, as what sits unrecorded in a folder its agents are now
  sent copies to is said to be somebody's: the project is listed from there
  from now on.
- **Local when asked.** `--local` writes `.mind/.gitignore` and keeps each
  copy out of git in the project's new group of the exclude block, before
  the copies land, as `mind init` and an install do. A `.gitignore` keeps
  out only what git does not track yet, and such a `.mind` was committed, so
  when gix's index says git tracks something there (`exclude::tracks`), the
  command says `git rm -r --cached .mind` stops it. Said and never run:
  Mindflayer runs no git command, and taking files out of a team's index is
  not its to do.
- **All or nothing, for the project.** Held by the lock of the workspace
  that links it, and planned again once held. Then, in an order that shows
  nothing in `git status` for a moment: a local project's exclude entries
  and `.gitignore`; the new copies, each installation's from one stage
  (`copy::Stage::place`); the copies taken out, moved into that stage rather
  than deleted; the settings; the marker; and last the ledger, every
  installation and the log in one transaction (`Ledger::migrated`), so that
  each step before it can still be undone. A step that fails puts back every
  one before it, the last first, so the `mind.toml` marks the project again
  and the ledger names what it named, and what cannot be put back is said
  (`MigrateError::left_behind`). Only once the ledger holds the move are the
  copies taken out deleted, with the stage. A process killed part way is not
  a failure, and nothing puts it back ([open questions](#open-questions)).
- **Refused before anything is written**, every reason at once: a folder
  the `mind.toml` keeps a kind in that settings cannot name, inside `.mind`
  as a version 1 project kept every kind it named no folder for, or outside
  the project; no agent; a kept marker; and a copy that would land in an
  agent's own folder or where a loaded workspace keeps its own, as an
  install refuses one. A folder a version 1 `mind.toml` does name is kept,
  as every other command reads it (`MindProject::directory_for`), so
  naming one outside `.mind`, as the refusal asks, gets such a project
  past it.
- **A second run is a no-op.** A project its `settings.toml` marks has
  nothing to migrate, and nothing is written for it, not even its ledger
  opened.
- **Each project on its own.** `--all-projects` moves every project the
  workspace links that a `mind.toml` or a `settings.json` marks, one at a
  time, each all or nothing: one refused or failed is an `error:` naming it,
  the rest go ahead, and the command exits non-zero. Without `--yes` it
  lists every one.
- **A project nothing links still moves.** Its marker is its own
  ([D-09](#d-09-project-settings-live-in-the-project)), so in a project no
  workspace `mind` consults links, `mind migrate` writes its settings and
  moves no copy, and says with a `hint:` that nothing installed was carried
  along. The hint never says that no workspace links it, which walking up
  cannot know: it names `-W`, which reaches a workspace that links it from
  beside it, and without which that one's records are lost to the move all
  the same. A project whose workspace cannot be read is refused instead,
  in a dry run too, where a change of agents goes ahead beside a
  `warning:`. That change can be run again; the move is made once, and
  what the workspace records decides which copies it carries. Made without
  them, nothing carries those copies once settings mark the project, and
  one recorded without a hash, which only the move holds to its offer, is
  dropped the next time its agents change.
- **One migration a line.** `flayer migrate` takes `--library`, `-p` or
  `--all-projects`, one at a time, for the reason `--library` is a flag of
  its own ([above](#moving-them-into-the-library)), and a line naming none is
  told which there are, rather than shown clap's usage.
- **From a `settings.json`, the same settings in TOML.** A project a
  `settings.json` marks has its agents, share mode and folders already, so
  its move writes what the file holds as a `settings.toml`, at version 5,
  and keeps the `settings.json` as `.mind/tmp/settings.json.v<version>`,
  with `create_new`, as a `mind.toml` is kept; one an earlier migration kept
  refuses it. A folder written there by hand that a change would refuse is
  carried as it is: the move changes the grammar, not what the settings
  say, so the project reads the same before and after. Asked for, never
  made on a read, as a move from a `mind.toml` is: a build from before #166
  finds no project in a `settings.toml`, so the move is a step a team takes
  on purpose, and a read leaves things as it found them. No copy moves, since
  every one is where the agents send it already, so no workspace is read
  (`migrate::carries_installs`): no lock, no ledger, and a workspace that
  cannot be read is a `warning:` beside the move, where it refuses a move
  from a `mind.toml`. `--agents`, `--local` and `--shared` are what a
  `mind.toml` is given, and the report's first line says the agents and
  share mode the file keeps. A key this build does not know stays in the
  copy kept. Nothing else writes a `settings.json` any more, and a second
  move at once finds the `settings.toml` there, which `create_new` refuses,
  so the move needs no lock of its own.
- **The TUI points at it.** Its header counts the projects an older marker
  marks, in yellow after the workspace's name, where a narrow terminal keeps
  it, naming each file once (`2 projects marked by mind.toml or
  settings.json`), and names the command: `flayer migrate --all-projects`,
  or `mind migrate` for a project on its own. `t` and `T` on a project a
  `settings.json` marks say it is changed by nothing but the move before a
  form opens, as they say of a `mind.toml`. It has no key for the move
  ([open questions](#open-questions)). `flayer status` and `mind status`
  say it too, a hint for each such project ([what each project
  has](#what-each-project-has)).

### Why the markers are TOML written from a template

New markers are written from a string template, not serialized. Serializing
would drop the comments, and the comment at the top of a marker, saying what
each key does, is the first documentation anyone opening the file will read.

A project's marker is TOML again from version 5, the owner's amendment to
[D-06](#d-06-projects-keep-authoring-beside-the-code): `.mind/settings.toml`,
like `flayer.toml`, so both markers are written and edited the same way.
Versions 3 and 4 were JSON, `.mind/settings.json`, the owner's first choice:
any tool can edit JSON, and seven keys that name what they hold seemed to
explain themselves. JSON has nowhere to put a comment, though, so what each
key did was in this document and the README rather than in the file
somebody opens to change one, and the settings needed an editor of their
own, a third way beside `flayer.toml`'s and `mind.toml`'s, which was
`config_entry`'s JSON (held to RFC 8259 so `jq` and `JSON.parse` took what it
wrote). Now `Settings::text` fills a template as `flayer.toml`'s is filled,
its comments saying what each key does and which command changes it, every
folder spelled out and a `[targets]` table only when it names a kind; and
`config_entry` is for the agents' own config files, whose formats are
theirs. A `settings.json` is still read, the way it was written, and moved
forward only by `mind migrate`.

Editing an existing marker has the same constraint and a harder job, which is
why `toml_edit` is a dependency: `flayer link` rewrites the one `projects`
array and leaves every other byte alone, and a change of a project's settings
rewrites the keys whose values changed, each where it stands, keeping the
comment on its line ([a project's settings](#a-projects-settings)). Both go
through one `rewrite`, which reads the file, hands its text to the edit and
writes back only what changed. Reading `flayer.toml` and a `mind.toml` stays
serde's job; a project's settings, in either grammar, are read a key at a
time (`config_entry::document` and `settings::parse`), so a newer version is
refused before any key it may have given a new meaning, and a key this build
does not know is read past rather than refused. The two halves agree because
an edit re-reads the file afterwards rather than trusting what it believes
it wrote, and the write itself goes through a temporary file and a rename,
so a crash leaves either the old config or the new one. An edit of
`flayer.toml` holds the workspace's lock from its read to that re-read, so a
second writer edits what the first one wrote ([Writers at the same
time](#writers-at-the-same-time)); an edit of a project's settings takes
none of its own, and the one change that makes it (`reapply::apply`) holds
the lock of the workspace that links the project around it.

### Formats are versioned

Each marker carries a `version`, and each has its own: `MIND_FORMAT_VERSION`
for a project's marker, `FLAYER_FORMAT_VERSION` for `flayer.toml`. A marker
written by a newer Mindflayer is refused with a message naming which of the
two it is and which version this build reads, which is what lets a format
change later without an old binary silently misreading a newer project or
workspace.

Two constants rather than one, because the files change for different reasons.
Shared, the project marker going to 3 would have written every new
`flayer.toml` as a 3 too, and an older build would refuse a workspace that had
not changed at all. A project's marker reads up to 5 today, a
`settings.toml`, and a workspace's up to 2, and neither moved when the other
did.

A project's newest version is the one `mind init` writes, a `settings.toml`
(#166), as it waited to write a `settings.json` (#35) until this build read
such a project (#29), listed it (#33) and installed into it (#34). 5 for a
`settings.toml` whatever it sets, though it holds the keys a version 4
`settings.json` does, because a version names the file it is written in as
well as what the file holds, as 3 did between `mind.toml` and
`settings.json` (`MarkerFile::of_version`): one found in another file is
refused rather than guessed at. An older build finds no project without the
file it looks for, so a build from before #29 says a new one is not a
project, and one from before #166 the same of a `settings.toml`, or that a
workspace's entry for it is stale, rather than misreading it. Except where
that answer leads it to make one: a build from before #29 asks only whether a
`mind.toml` is there, so its `mind init`, or its TUI's making a repository a
project, writes one beside the settings and reads the project as a `mind.toml`
one from then on, while this build reads the settings and passes the
`mind.toml` by ([open questions](#open-questions)).
`MIND_TOML_VERSION` is the newest a `mind.toml` holds, which only
`MindProject::init` still writes.

A key that an older build can read past without misreading anything moves no
version: `known`, the list of [the workspaces this machine
knows](#the-workspaces-this-machine-knows) that the default workspace keeps,
says nothing about the workspace holding it, and every edit of a marker keeps
a key it does not know.

An older marker is read the way it was written, never refused, and moved
forward only when somebody asks: a workspace's own folders by `flayer migrate
--library`, a project's `mind.toml` or `settings.json` by `mind migrate`
([moving a project to its settings](#moving-a-project-to-its-settings)).
Four lines are drawn so far, all project-marker versions, since a
workspace's number says nothing about how a project keeps its artifacts:
`DIRECTORIES_VERSION`, before which every kind lived inside `.mind`;
`SETTINGS_VERSION`, from which the marker is a settings file and a kind it
does not name has no folder; `TARGETS_VERSION`, from which the settings may
say where installs land; and `SETTINGS_TOML_VERSION`, from which the
settings are `settings.toml`. A `settings.json` was written at 4 only while
it set `targets`, so one is a 3 or a 4 by what it says rather than by which
build wrote it: one that never set them is read by every build since
settings, and one that did is refused by the builds that would read past
them ([a project's own targets](#a-projects-own-targets)). The
ledger carries its version in SQLite's `user_version` and is refused from the
future the same way, but an older one is migrated forward rather than read as
written, after a copy of it is set aside ([Why the ledger is
SQLite](#why-the-ledger-is-sqlite)).

## Failures are collected, not raised

Discovery returns what it found *and* what it could not read. One skill with
broken front matter must not hide the forty next to it, and a stale entry in a
workspace registry must not stop the other projects from being managed. Two
copies of one skill that differ in a project's agents' folders are collected
the same way: listed, and said.

The CLI prints those on stderr as warnings and exits non-zero: visible, but not
in the way of the answer. Asked for JSON, it puts them in the envelope beside
the data instead ([JSON, in one envelope](#json-in-one-envelope)).

The distinction the code draws is between a problem and an absence. A project
with no skills folder yet is not a failure, it is a project nobody has added a
skill to. A skills folder that exists and cannot be listed is a
failure.

## JSON, in one envelope

A reading command's `--json` prints what it read inside one envelope,
whatever the command (`render::json`):

```json
{"version": 2, "data": ..., "warnings": [...]}
```

`status` was the first to take it ([what each project
has](#what-each-project-has)), and every later reading command takes the same
one rather than a shape of its own: `scan --json` ([Scanning](#scanning)),
`workspaces --json` ([the workspaces this machine
knows](#the-workspaces-this-machine-knows)), and the read types #61 makes
serializable. One shape is one parser for a script,
and for the panel's backend when it reads through the CLI.

- **The warnings are inside it.** Failures are collected, not raised, in JSON
  as in text: what a table would say on stderr as `warning:` lines goes into
  `warnings`, without the prefix, and stderr is left without them, so a
  script reading stdout misses nothing. The exit status is the text's. A
  hint, advice for whoever reads the terminal, stays on stderr. An error that
  stops a command is still text on stderr, `--json` or not, until #61 gives
  errors codes.
- **Its names are a contract.** Every type in `data` derives `Serialize` in
  core, each field by its snake_case name, and `Kind`, `Agent` and `State`
  are serialized by the word a command line takes for them (`skill`,
  `opencode`, `missing`), written by hand rather than derived: a derive
  spells the variant, so `Agent::OpenCode` would print `open_code`, a name no
  command line or settings file holds, and a rename would change it. A path
  is text, whatever bytes it holds, so one unprintable path cannot fail the
  report. A name once printed stays: a field that goes or changes what it
  holds moves `ENVELOPE_VERSION`, and one added does not, since a reader
  passes over a key it does not know. A test reads each report back into
  structs of its own that refuse any field they do not name
  (`common::enveloped`), so a change shows there before it reaches a script.
  It moved once, to 2, when `status` took the five states an installation
  stands in ([what each project has](#what-each-project-has)): `installed`
  and `partial`, which a version 1 script knew, went, and a value a script
  never met came in their place, which is a field changing what it holds.
- **Pretty-printed**, two spaces a level, since a person reads it as often as
  a script does, and `jq` takes either.
- **The envelope is the CLI's**, in `render.rs` beside the table helpers: how
  a report is laid out is a front end's to choose. What goes in `data` is
  core's, which builds and orders it, so the table and the JSON cannot
  disagree about what is there.

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
somewhere that does not exist; `/tmp` is a symlink to `/private/tmp` on every
Mac, so this is not exotic. So `link` **checks its route against the
filesystem** before storing it, and falls back to an absolute path when it does
not land where it should. The check happens there and only there: its answer
decides which spelling to store and is never stored itself, so entries stay
portable rather than frozen to one machine's symlink layout.

Matching is by where an entry **points**, not by how it is spelled, so
`collapse` and `./collapse` are one entry. Arithmetic settles most of it, and
two spellings only the filesystem can equate are settled by canonicalising.

`loaded` and the default workspace's `known` are stored and matched the same
way ([the workspaces this machine knows](#the-workspaces-this-machine-knows)).
An entry dropped from any of the three leaves its place's layout to the entry
after it, so dropping the first of `["a", "b"]` leaves `["b"]` rather than
`[ "b"]`, and a list laid out a line each keeps its lines. It takes its own
comments with it and leaves everybody else's: the ones above it and the one
after it on its line go, while a comment above the entry after it, or at the
end of the line before it, stays where it was. toml_edit keeps what comes
before an entry as one string, from the comma ahead of it, so the end of the
line above and the comments above the entry share it. Each is split at its
first line break: the part before it stays with the line above, the part after
it with the entry below. Handing the next entry the dropped one's string whole
would put the dropped entry's comment above it in place of its own.

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
temporary first, and a symlinked config is followed to the file it names;
otherwise an edit would quietly widen a chmodded config or detach a shared one.
A hard link is the case this cannot preserve: a rename breaks it, and keeping
it would mean giving up the atomic write.

### An entry whose project moved

A route is what moving the project changes. Moved alone, a repository leaves
its entry stale, and every record of what was installed into it names a
route nothing is at, so its copies read as somebody's once it is linked again
where it went. Moved with the workspace, the route stays true, which is why
it is a route; moved without, nothing in the entry can follow it. What does
move with it is the `id` its settings hold ([D-09](#d-09-project-settings-live-in-the-project)),
128 random bits, so that only a copy shares it. So the ledger keeps the id it
last saw at each route ([schema 5](#why-the-ledger-is-sqlite)), and a
project found somewhere else holding an id last seen at an entry where no
project is any more is that entry's project, moved. `flayer scan` says so
(`moved: <old> -> <new>`, [scanning](#scanning)), and `flayer relink <old>
<new>` (`relink::relink`) follows it.

The route stays the key of every record, with the id beside it
(`installations.project_id`), rather than the id taking its place. A project
a `mind.toml` marks has no id and goes on being found by its route, and so
does one whose id nothing recorded yet; keyed by the id, both would need a
second way in, and every record written so far re-keying.

The ledger learns the ids from the projects themselves, never from the
entries: a step to schema 5 is told those of every project the workspace
links, and the ledger, opened to write, is told them again every time
(`FlayerWorkspace::ledger_to_write`, `Ledger::knew`), as the scrub runs every
time, so an entry written by hand, a project given a new id and one whose
route holds another project now are all followed without each command that
links remembering to. A stale entry keeps the id last seen there, which is
what finds its project, and only a relink moves it. An id two linked
directories hold is told to nobody.

A relink rewrites the entry where it stands, with the comments around it,
which a person wrote about the project rather than about where it was, and
moves every record and every file Mindflayer made there to the new route in
the same transaction as the rewrite of the marker (`Ledger::relinked`): an
entry moved without its records would leave them under a route no entry
names, and records moved without it would name a directory the workspace
does not link. It holds the workspace's lock from before it reads the ledger
for its checks. A local project's exclude lines go with it
([what keeps an install out of git](#what-keeps-an-install-out-of-git)). It
refuses, with nothing written:

- a directory holding another id than the one last seen at the entry, which
  is another project, and one holding the id the ledger last saw at another
  entry, which is where it moved from, if anywhere;
- an entry that still opens a project, which has not moved: with the same id,
  a copy; and one whose marker is there and would not open (a typo, a newer
  version), which still holds its project, whose id nothing read, so a copy
  of it would pass for it moved (`MindProject::is_vacant` tells the two);
- another linked project holding the id, a copy too;
- a project a `mind.toml` marks, which has no id (moves of those are found
  by nobody, by design);
- records at the entry with no id, as a ledger from before ids were kept
  holds them for a project not read since: nothing says they were that
  project's, and a record is what lets Mindflayer write over and delete what
  sits at the path it names.

A workspace with no ledger, or whose ledger records nothing at the entry,
has nothing to keep, and the entry alone moves.

A repository copied with its `.mind` holds the same id as the one it was
copied from, and no record can tell which of the two it was made for. So a
copy is never taken for a move: two directories holding one id, among what a
scan found and what the workspace links, or among what `status` reports, are
a failure collected with the rest that names every one (`relink::Duplicate`),
and a relink refuses one it can see. `mind init --new-id` (`relink::new_id`)
gives a copy an id of its own and keeps every other byte of its settings:
the one change `MindProject::change_settings` refuses, an id naming the
project for good, made apart from it (`MindProject::change_id`) through the
same rewrite and toml_edit. A local copy's exclude lines follow it to the new
id, and a workspace linking it knows it, and every record of it, by the new
one, so what was installed stays the project's. A `settings.json`, which
nothing but `mind migrate` changes, is pointed there.

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
answer `Catalog::find` gives inside a project (return both, decide nothing),
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

### Following a gather, and stopping it

A gather is the network, so it can take long enough that somebody wants to
know how it is going, and to change their mind. A `gather::Request` carries a
`Watch`: a callback the gather tells each `Progress` to, and a flag that
cancels it. Both are core's own types. `gix` has a progress tree and an
interrupt flag of its own, and handing those to the front ends would tie each
of them, the server that will stream progress (#112) included, to gix's
version and to the names it gives its steps.

The steps are connecting, receiving objects, resolving deltas, checking out
files, reading the folders that hold a manifest, `n` of `m`, and placing what
was read on the shelf, `n` of `m`. The counts during the fetch are gix's: the
clone hands it a progress tree and reads it from a thread of its own ten times
a second, because gix counts most of its work on shared atomics, from threads
of its own, without telling the progress anything. The counts are picked out
by the identifiers gix gives them, not the names it shows, which it is free to
reword. That thread is joined before the clone says its next step, so a front
end never hears a count after the step that follows it. The callback is a
plain `Fn` that is `Send` and `Sync`, rather than a channel, so each front end
chooses how the news travels: the CLI writes a line, the TUI sends it to the
thread that draws, and a server can stream it.

The flag is gix's interrupt, so a cancel stops the fetch or the checkout where
gix checks, and the clone it had begun is removed. gix does not check it while
it connects, only once the pack is coming, so a cancel while connecting waits
for the connection to be made or to fail: as long as a server takes to answer,
or a prompt to be answered. One made while the gather waits for [the
workspace's lock](#writers-at-the-same-time) waits too, since nothing checks
the flag until the lock is held: as long as the writer holding it takes, and a
wait that runs out ends the gather as busy rather than cancelled, unlogged all
the same. The harvest checks it once more after every folder
is read and before anything is placed, which is why it reads them all first
rather than reading and placing each in turn. That is the last moment a cancel
is honoured: past it the shelf and the ledger change, and a gather stopped
halfway through that would leave them neither as they were nor as the source
has them. A cancel that comes later is too late, and the gather finishes.
`Progress::cancellable` says which steps are which, so a front end that offers
a cancel stops offering it when the gather stops honouring it, rather than
knowing where the check is. Cancelled in time, the shelf and the ledger are as
they were: the gather is not logged, and a source it registered is forgotten
again. It is registered before the fetch because the clone needs the folder
name registering gives it; holding the registration in a transaction until the
gather ends was the other way, and is rejected, because every other write to
the ledger would wait as long as the network does. The clone is not part of the
promise: every gather re-makes it, so a cancelled one can leave a known source
with the new clone, or with none.

`flayer gather` on a terminal draws each step over the last on one line of
stderr, and wipes it before the report. Piped or captured it draws nothing,
so what it prints there is what it always printed. The binaries run every
line with that line's watch (`run_watched`, `run_flayer_cli_watched`), and
`run` and `run_flayer_cli`, which the tests call, with one nobody follows. The
TUI runs a gather on a thread of its own, and `esc` cancels it ([A gather runs
apart](#a-gather-runs-apart)).

### Why `gix` rather than running `git`

Nothing depends on a `git` being installed, or on which `git` is first on the
PATH, and core keeps doing its own I/O rather than supervising a process. The
cost is that authentication for private repositories is ours to solve rather
than inherited whole from `git`.

Half of it is solved: a fetch **never prompts** for a credential. When a server
asks who is fetching, `gix`'s own answer runs the credential helpers git config
names and, when none of them has a credential, asks a person: an askpass
program, then the terminal. The TUI owns the terminal while a gather runs, so
that prompt is drawn over, its keys raced for, and the server that comes next
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
created. Which commands write is the front end's to know, but what opening to
write involves is core's, so no front end can write first and scrub after.
`gather git`, `install` and `uninstall` open it that way, as do
`install::disown` and `install::carry`, through which core's lifecycle changes
(`lifecycle::add`, `rename` and `remove`) drop or carry a record. Each hands
back what the scrub took out (its result's `scrub`) beside what it did to the
record, since a scrub says what it found only the first time and a change that
dropped it would leave it unsaid for good. `rename` opens the ledger once for
each name and tells what both scrubs found once (`Scrub::absorb`): the second
finds only the clones the first could not rewrite. So does the install screen,
in both screens, when what it marked is applied rather than as it opens,
which is a read ([A read leaves the ledger as it finds
it](#a-read-leaves-the-ledger-as-it-finds-it)): applying runs the `install`
and `uninstall` lines its marks amount to, and the first of them opens the
ledger to write, scrubbing it, as from the shell. A scrub can make two sources
one row, and a mark on the row that went would install from folders the scrub
removed, or copy the files and then fail to record them. Each install line
names its offer by its origin (`--from`), which is shown without credentials
before a scrub as after it, so once a scrub has taken out a source the line
finds the row it became one with, through the lookup any `--from` goes
through; a line for what only the row that went had gathered is refused as
not offered, said under that line. Scrubbing as the screen opened would have
spared that, and is rejected: opening it is a read. An edit writes
nothing to the ledger, since it keeps the record as it was, so `finish_edit`
only reads it (`ledger_for_reading`) and scrubs nothing.

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
answers questions a file cannot (which of two identically named skills came
from which repository, at which revision, and what happened the last four times
a gather ran), and answering them from a flat file would mean writing a query
engine badly.

It sits in `.mindflayer/`, beside the marker it belongs to, so it travels with
the workspace and two workspaces never share a history. It carries its schema
version in SQLite's own `user_version` header field, and a database from the
future is refused exactly as a marker file from the future is. The first
command that writes to it makes it; a read never does, and changes an existing
one only to migrate it ([A read leaves the ledger as it finds
it](#a-read-leaves-the-ledger-as-it-finds-it)).

Six things are recorded: the **sources** gathered from, the **artifacts** on
the shelf and which source each came from, the **installations** (which skill
or rule Mindflayer put into which project, from where, every copy it wrote and
a hash of what it wrote), the **files Mindflayer made** in a project, which it
may take away again, the **projects** by the id each was last seen holding,
and an **action log**. The log keeps failures too: a log
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

The schema has had five versions, each a block in `Ledger::migrate` and a
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
4. A `content_hash` of each target's own, for a kind whose copies hold
   different bytes: a rule's section of the block in `AGENTS.md` and its
   file for Claude ([installing rules](#installing-rules)), each of which
   can be edited apart from the other. A skill's targets leave it empty,
   every one holding the tree the installation's hash names. And
   `created_files`, each file Mindflayer made in a project rather than
   wrote into, so the block goes with the last rule and the file with it
   only when Mindflayer made it (#45).
5. `projects`, the id the ledger last saw at each route, one id a route and
   one route an id, and on `installations`, the `project_id` each record was
   made for, so a project that moved is found again by its id ([an entry
   whose project moved](#an-entry-whose-project-moved), #57).

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
hashed when it was written, and D-04 says how a missing one reads. Version 4
fills in nothing: no build before it installed a rule, and a skill's targets
need no hash of their own. Version 5 fills in what no SQL could: the file
kept no id, so the step is handed the id of each project the workspace
links, opened for its settings, with the route its records are under
(`Ledger::open_knowing`), and its own SQL writes them in. The one exception
to what a step is filled from, and the reason for it holds: what it is handed
is what the projects hold, not how today's code spells something. A project
a `mind.toml` marks, an entry that will not open and an id two linked
directories hold are handed in for none, and their records keep their route
alone. D-04's states (#47) took no step of their own:
what tells them apart is the hash each copy was written with, which versions
3 and 4 keep, and what the place it came from offers now, which is read where
it is offered rather than kept. A record with no hash is held to its offer
by the code that reads it (`install::standings`), since a step has no offer
to fill one in from, and the first install over it stores its hash. Each step
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
one could read anything. For a skill it is kept once per installation, not
once per target: every target is written from one copy, so one hash is what
each is compared with, and a copy somebody edited is the one whose own hash no
longer matches it. A rule's targets each keep their own, `hash::bytes` of
what each holds, since its section and its file are not the same bytes. An install records the hash of the copy as it stands once
written; one that cannot be read back is recorded with none rather than not
at all, since without a record it would be Foreign for good. That is a
`warning:` and a non-zero exit, and the log's entry says why, rather than a
plain success: a record with no hash is read as one from before hashes were
kept. No install leaves a copy it can tell holds what the hash refuses. A
link where no install writes one is an edit, which an install that is not
forced refuses and a forced one writes over, even when it points at the
same bytes, which `copy::same` follows it to (`Stage::place_over`): left as
it was, the copy would be recorded with no hash and read as edited still.
What is left is a copy changed between landing and being read back.

## Installing

Gathering fills the shelf; installing is the other half, and the only thing in
Mindflayer that writes into a mind project. A skill is copied wherever the
project's agents read it ([D-07](#d-07-one-copy-per-agent-folder)): one copy
in each of its targets (`install::targets`), the folders the target table
sends its agents to (`MindProject::targets`), so a project naming all three
gets a copy in `.claude/skills` for Claude and one in `.agents/skills` that
Codex and OpenCode share, and one whose settings name `targets` a copy in
each folder they name, for all of them ([a project's own
targets](#a-projects-own-targets)). A project a `mind.toml` marks names no
agents, and gets one copy in the folder its marker names, as before. A rule is
no folder, and reaches a project's agents otherwise: through the block of its
`AGENTS.md` and Claude's files, rendered with every other rule it has
([installing rules](#installing-rules)).

Not the folder a project's settings say it authors in
([D-06](#d-06-projects-keep-authoring-beside-the-code)): what the workspace
offers goes where the agents read, what the project writes itself stays where
it writes it, and neither is mistaken for the other. Two targets that are one
folder on disk, `.claude/skills` linked to `.agents/skills`, are one target
under the first spelling, as discovery reads them once; two copies put in
place in one folder would find each other there.

### The folder is named twice, on purpose

Gathering keeps the folder name the **source** used. Installing files the copy
under the name the skill **declares**. That looks inconsistent and is not.

On the shelf, renaming would repair a symptom and hide it: a skill whose
directory disagrees with its front matter is something `validate` exists to
report, and the shelf is a record of what a source actually contained. Inside a
project the same mismatch is not a report, it is a skill that lists fine and
never loads: the directory name is how an agent finds what it was told to
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
each copy it wrote as a target of it (its path, and the one agent it is for,
or none for `.agents/skills`, which two agents share), and that record is what
separates a file this tool put there from one somebody wrote. Standing is
worked out over every target, by the path each record names rather than by
its name (`Ledger::copies`, as a listing marks installed copies): a skill
present anywhere it would be written, and unrecorded there, is
`Standing::Foreign`, whatever the other targets hold. Foreign is inert in both
directions: never overwritten, never deleted, and the caller is told so,
with the path, rather than obeyed quietly. It becomes Mindflayer's only when
somebody adopts it by name ([adopting what a project already
keeps](#adopting-what-a-project-already-keeps)), as it is. So somebody's skill in
`.claude/skills` refuses the whole install, and `.agents/skills` is not
written either: a copy beside it would hand OpenCode, which reads both, two
different skills of one name. Only the targets refuse, as D-07 has it, not
every folder an agent reads (`Agent::reads`): a project that names OpenCode
and not Claude is sent nothing in `.claude/skills` or `.opencode/skills`,
though OpenCode loads what is there, so somebody's `ship` in one of them does
not stop `.agents/skills/ship` from being written, and OpenCode is then
handed two versions of one skill, which nothing says ([an open
question](#open-questions)). A name installed into one folder does not claim
what somebody keeps under it in another, as when a project names an agent
after the install. Where every copy there is recorded, what each copy the
record names holds says how the installation stands ([an edited copy is
left alone](#an-edited-copy-is-left-alone)): one gone since is
`Standing::Missing`, and the next install puts it back. A listing says the
same of a copy in a project's agents' folders
(`Provenance::Foreign`), and no change to the project's own reaches one
([read where its agents
read](#a-project-marked-by-its-settings-is-read-where-its-agents-read)).

Uninstalling goes by the record, not by where the project would install now:
every copy it names is removed, and nothing else, so a folder of that name in
a target the record does not name stays somebody's. A route the record holds
is followed only when it is a folder of the project named after the skill
(`InstallError::Recorded` otherwise): before records had targets, a removal
was only ever the name joined onto a folder of the project's, and the ledger,
though the workspace's own, is not a reason to delete anywhere else.

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

A project has one directory per artifact name in each target, so two shelf
entries offering `commit-style` cannot both be installed. The screen settles
it by unticking the other rather than letting both apply and the second win: a
conflict resolved where somebody can see it happening.

### An edited copy is left alone

The record keeps a hash of what was written in each copy
([D-04](#d-04-an-edited-copy-is-detected-not-disowned)): once for a skill's
installation, since every copy is staged from one tree, and once a target
for a rule's section and its file, which hold different bytes. That hash,
and what the place the record says it came from offers now
(`install::offered_for`), tell five standings of an installation apart, as
`install::standings` says of each installation and `survey` of each offer of
its name:

- **Current**: every copy holds what was written, and the offer holds it
  still.
- **Outdated**: every copy holds what was written, and the offer has moved
  on. An update brings it up to date, as an install of it does ([bringing
  what was installed up to date](#bringing-what-was-installed-up-to-date)).
- **Edited**: a copy holds anything else: a hand edit, `mind edit`, a
  skill's rename, which rewrites its `name` line, a file added, or a link
  where a file was, which the hash refuses and no install writes. A rule's
  rename moves its file as it is, and is no edit: the copy holds what was
  installed, orphaned while nothing offers it by its new route.
- **Missing**: a copy the record names is not where it was put, and an
  update puts it back, as an install of it does. Every copy gone is still missing rather than absent:
  the ledger records it, and `flayer uninstall` forgets it. The install
  screen opens its box empty all the same (`Standing::present`), saying a
  copy is gone, so marking it puts every copy back, as it did when every
  copy gone read as absent: a `git clean -dX` takes every copy of a local
  project at once (and its `.mind` with them, an open question). Held, the
  box could only be marked to be taken out, so one with a copy left is
  taken out with `flayer uninstall`.
- **Orphaned**: the place it came from no longer offers it. No offer carries
  it onto the install screen, so `standings` is where it is said, and
  `status` and an update say it from there.

When more than one holds, edited is said first, then orphaned, missing,
outdated: an edit is what a change must not undo, an offer gone leaves
nothing to put back or bring up to date, and a copy put back is put back as
the offer holds it now. Outdated asks for the offer itself, read where it is
offered: a skill's copy is compared with it as an install would write it
(`copy::same_leaving`, without what no copy holds), a rule by the hash of its
file against the one its record says every copy was rendered from.

An edited copy is treated as a foreign one is. `install` and `uninstall`
refuse the whole change, nothing written or taken out in any target, and name
each edited copy (`Installed::Edited`, `Removed::Edited`), unless forced
(`install_with` and `uninstall_with` given `Edits::Forced`, which `--force`
asks for). Every copy or none: writing over the untouched copies beside an
edited one would hand the agents reading both folders two versions of one
skill. The look is made under the workspace's lock, after the one for
somebody's, and wherever the record has its copies, since an install records
its copies afresh and one in a folder it no longer writes would be left with
no record. Forcing reaches only what the ledger records: a copy nothing
records stays out of reach whatever is asked. The install screen never
forces, and holds an edited row's box as it holds a foreign one's. An
update leaves an edited installation as an install does, every copy of it,
and says so, unless forced.

A rule's copy from before a project had settings, in the folder its
`mind.toml` named, is not one of the copies told in a project marked by
them. The next render takes it out while it holds what was copied and
leaves it, the project's own, when it does not, forced or not ([moving a
project to its settings](#moving-a-project-to-its-settings)), so an edit
there refuses nothing: refused, it would be a change `--force` does not
make either.

A record with no hash, from before version 3 of the ledger or of a copy
that could not be read back, is held to its offer byte for byte: current
while its copy matches, edited otherwise, and edited when nothing offers it
any more. An untouched copy whose offer has moved on then reads the same as
an edit, and taking it for one is the safe mistake. The first install over a
copy that matches stores its hash, and from then on it is told as any other.

### Bringing what was installed up to date

`flayer update` and `mind update` (`update::plan`, `update::apply`) act on
the standings: each installation outdated or missing is written again from
the place its record says it came from, as that place offers it now
(`Surveyed::source`), and nothing else is. A current one has nothing to
change. An edited one is left, every copy of it, as an install leaves it,
and said (`Step::Leave`) unless the update is forced, when it is written
over and the copies it wrote over are said (`Step::WriteOver`). An orphaned
one has nothing to be written from, edited or not, so it is said and left
as the project holds it (`Step::Orphaned`): a note, never a failure, since
nothing could have been done for it, and `uninstall` is what takes it out.
A record from before origins were kept may stand for an offer of the
workspace's own or of any workspace it loads; the one holding what an
untouched copy holds is where it came from, and with none left to tell by
it is not guessed (`Step::Unsure`), as an install with no `--from` does not
guess between two.

- **Written as an install writes.** Each skill, and each rule copied into a
  project a `mind.toml` marks, goes through the install's own body
  (`install::install_held`), so its copies are staged and put in place
  together, its record and hash written afresh, and a local project's
  exclude entries written before a copy lands. A copy a local project lost
  comes back kept out of git as it was: one deleted by hand, or the
  `AGENTS.md` it made, which a `git clean -X` takes, since it is a file in a
  folder git tracks. Every other copy is in a folder of its own, which only
  `git clean -dX` goes into, and that takes the project's `.mind` too, so
  the project has to be marked again before an update can find it (open
  questions). Under the lock the body looks again at everything an install
  refuses, so a copy edited between the plan and the write is left all the
  same.
- **One lock, a plan made under it.** `apply` opens the ledger to write,
  takes the workspace's lock, reads each project again and makes the plan
  again under it, then writes every installation, as `reapply` and
  `migrate` hold it from plan to write. The lock is not re-entrant, so the
  install it writes with is the body `install_with` hands to once it holds
  the lock, rather than `install_with` itself. A workspace with no ledger
  file is given none: it records nothing to update.
- **Failures are said where they happened.** A project that cannot be read
  again refuses the whole update before anything is written. After that,
  each installation is written whole or not at all, and one that fails
  says so in its own `done`, beside the rest, which go ahead; the same
  update run again finishes what is left.
- **A project's rules come together.** The block in `AGENTS.md` is one file
  written whole, so a project's rules are brought up to date by one render
  (`rules::update_held`), which gives each rule as its offer holds it now.
  Updating one rule therefore brings each other outdated or missing rule
  of the project along, and the plan names each, so the lines say what was
  written; leaving them behind would mean reading each back from the block,
  as a render reads back only an edit it keeps. That render keeps a rule no
  longer offered as the project holds it, as an uninstall of another rule
  does, rather than refusing the rest for it, and writes over only the
  edits of the rules the update was forced to write over. Every other rule
  somebody edited, named or not, is kept whole (`Change::leaves`): each
  copy as the project holds it, the one not edited too, and its record as
  it was. A rule has two copies where Claude reads no block, its section
  and its file for Claude, and keeping only the edited one, as an install's
  render does, would write the other from the offer and leave Claude and
  the agents reading the block with two versions of one rule.
- **Names narrow it.** Only the installations the names given name are
  planned, by kind and name as `install` takes them, every one when none is
  given; a name that matches nothing installed in any project asked is
  said, as a typo usually is.
- **A dry run without `--yes`**, which reads the ledger without making it,
  as every dry run does, and says the same lines with the same exit status:
  an edited copy left alone is a warning either way, as an install refused
  over it is, since what was asked for is not all done.
- **Nothing is gathered.** An update copies what the shelf holds now; a
  gather is what moves the shelf, and refreshing a source is its own
  command (#100).

The CLI lays it out (`commands/update.rs`): a line for each installation
written, `updated`, `put back ... at`, or `updated ..., writing over the
change at`, then, for a project's rules, what the render said of the rules
it wrote (`Rendered::warnings_of_update`), but no edit it kept, which the
update says itself. `flayer update` takes `-p` once for each project or
`--all`, refusing neither given, and `mind update` the project it is in,
through the workspace that manages it, as `mind install` does. Both resolve
it as a read, so neither makes the default workspace in home, not even with
`--yes`: one made for the occasion links no project, and the update would be
refused all the same.

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
- **Every copy from one tree, all or none.** An installation's copies are
  built once in the stage, and each target that does not already hold that
  tree gets a copy of it (`copy::Stage::place`), so the copies are the same
  bytes even when the offer changes while they are made, a workspace's own
  skill being edited say; copied each from the offer, OpenCode, which reads
  both folders, could be handed two versions of one skill. A target that
  already holds it is left alone, down to its times. Only once every copy is
  whole is any put in place, a rename each, and one that cannot be put in
  place takes those already moved back out, the copies they replaced put
  back: each target is as it was, and nothing is recorded. The installation
  is `Added` when every copy is new, `Unchanged` when none is, and `Updated`
  otherwise, a copy put back in one folder included. One hash is recorded,
  of the copies as they stand, which every one of them holds; copies that
  cannot be hashed, or that turn out not to hold the same, are recorded with
  none and said, as one copy that cannot be hashed always was.

So an agent reading a skills folder finds the old copy, the new one, or, for
the moment between the two renames of an update, neither, but never part of
one and never anything beside it. Gathering still stages beside the shelf copy
(`copy::replace`), because no agent reads `.mindflayer`; see the open
questions.

### Two places offer, one command takes

What can be installed comes from the workspace's own skills, offered first,
the workspaces it loads (below), and the shelf (`install::Offer`); a rule from
the first two, by its route, since nothing gathers one. Every
installation records its origin, in the spelling `--from` takes; one from the
shelf also points at the source it came from, and one of the workspace's own
at none, because it came from nowhere but the workspace.

`mind install <name>` and `flayer install -p <project> <name>` install one
without the screen, through `install::offered_typed`: the one candidate of that
name, or an error naming every place that offers it when there is more than
one; `--from` then says which, because which of two lands in the project's one
folder of that name is not something to guess. It takes an origin as printed,
and looser spellings of one: a source's address without its branch, a loaded
workspace's name. The origin as printed wins (`install::offered`), so
`<url>`, one address gathered at its default branch, is that offer and not
also `<url>#main`'s, whose address it is: refused as offered twice, the
first would have no `--from` of its own, and the line the install screen runs
for its box would be refused.

### Which kind a name is

A name is typed bare or after its kind, `rule:git/commits`, with the colon
every command takes ([why a qualifier uses a
colon](#why-a-qualifier-uses-a-colon)), and core reads it (`Reference`).
Bare, it is the one kind that offers it (`install::offered_typed`), asked
only of the kinds a copy could be filed under that name
(`install::is_installable_name`), so `git/commits`, which no skill can be
called, is a rule's however it is typed. A name two kinds offer is refused
(`InstallError::OfferedAsKinds`), naming what to type for each, as `mind show`
refuses one: a skill and a rule of one name are two things. Defaulting a bare
name to a skill, what every install meant before rules could be installed, was
turned down: a route would need its qualifier for nothing, and a name both
kinds offer would quietly install the skill. When no kind offers it, the first
refusal that says more than that nothing is offered is the one given, so a
project with nowhere to put a skill is told so, as it was. `--from` is asked
of each kind as typed, and only when that kind is offered nothing, as a path
to a loaded workspace (`install::resolve_from`): asked once for every kind,
the refusal of a project with nowhere to put a skill would hide a rule the
path offers.

Taking one out reads the ledger instead (`install::uninstall_typed`): a bare
name is the one kind recorded under it, one recorded as two is refused
(`InstallError::InstalledAsKinds`), since taking both out would be twice what
was asked, and one recorded as neither changes nothing whichever kind it is
taken for, so each kind it could be says what is there, somebody's of either
before the first's absence.

A line about installing says a rule as `rule <route>`, as `add` and `remove`
say one, and a skill by its name alone, as every such line did before: a
route reads like a path, and one name can be both.

### One artifact into many projects

`flayer install <name>` takes `-p` once for each project, or
`--all-projects` for every one the workspace links, and `flayer uninstall`
takes them the same way: vision point 2's assigning, from the command line.
Core's `assign::install` and `assign::uninstall` are the batch, and each
project's part of it is the install or uninstall that project would have
alone (`install::offered_typed`, then `install::install_with`, or
`install::uninstall_typed`), so a project reached this way is held to every
rule it is held to reached alone, and a front end asking for many decides
nothing that one install would not.

- **Each project once, in the order asked.** Two spellings of one directory
  are one project (`same_directory`), as an update takes them.
- **A part for each, beside the rest.** A project offered nothing of the
  name, one whose install fails and one whose copy is somebody's each say so
  in their own part, and the others go ahead. Each project's copies and
  record are written together or not at all by its own install, so nothing
  one project's part does depends on another's.
- **The ledger opened once, the lock taken by each install.** The batch
  opens the ledger to write, scrubbing it, once, and only that refuses it as
  a whole; a batch of no project opens nothing. Each project's install takes
  the workspace's lock as it does alone, in the lines the install screen
  runs as anywhere, rather than the batch holding it across them: nothing
  spans two projects, so holding it once would keep every other writer
  waiting for all of them, and would need a body of each install, rules'
  included, that runs under a lock it does not take, as an update has
  (`install::install_held`), to guard nothing a project's own install does
  not.
- **The front end names the projects.** `-p` picks each as everywhere
  (`FlayerWorkspace::pick`), and one the workspace does not link refuses
  the line before anything is installed anywhere: a typo in one name is no
  reason to change the rest without it. `--all-projects` is every entry
  that opens, with a warning for each that does not, as `flayer update
  --all` takes them, and resolves the workspace as update does, as a read:
  a default workspace nobody has made links no project, and making it for
  a command that then does nothing would leave a marker in the home for
  nothing. The flag is `flayer migrate`'s `--all-projects` rather than
  update's `--all`, which beside an artifact's name would read as every
  artifact.

The CLI says each project's part as an install into it alone says it
(`commands/install.rs`), in the order asked, so each line names its project
already, but for what rendering the project's rules warns of: that names a
rule or a file and not the project, which alone goes without saying, so
among several it is said after `<project>: `. One refused ends in
`error: <project>: ...`, and the command exits non-zero when any part did
not go through, while the others' lines stand, since what they did was done.
Two things are said once rather than once a project: what the loaded
workspaces could not offer, after every part, and a refusal every part
shares, such as a name nothing offers, which is the command's error. An
uninstall from several passes over a project that has nothing of the name,
which is as it was asked to be and what most projects answer when every one
is asked; when none of them has it, that is the typo it is for one project,
and refused as one. A batch of one project, named or the only one linked, is
said exactly as one install was before.

### Taking out what nothing offers

An orphaned installation, one whose origin no longer offers it, stays as the
project holds it: no update can write it, and no install screen offers it
to untick. `uninstall --orphaned` takes every one out of the projects asked,
`flayer`'s by `-p` or `--all-projects` as an uninstall by name takes them and
`mind`'s the one it runs in, through core's `assign::uninstall_orphaned`,
which is the batch above with `install::orphaned` choosing each project's
names rather than the command line.

- **What nothing offers, edited or not.** `install::orphaned` takes each
  installation whose record `offered_for` matches to nothing, which is the
  orphaned standing and the edited one that is orphaned beneath it, an edit
  being said first; an update leaves both alike (`Step::Orphaned`). Each is
  uninstalled as by name (`install::uninstall_with`), so the edited one is
  refused unless forced, every copy of it, and said with `--force` as the
  way out, as an uninstall by name says it.
- **Not one from a loaded workspace that is away.** A loaded workspace that
  cannot be opened offers nothing, so what was installed from it reads as
  orphaned, as `status` says, while it is unmounted or moved. Taking it out
  on that would take out what that workspace offers still once it is back,
  so a record naming its origin is passed over while it cannot be opened,
  and one from before origins were kept while any loaded workspace cannot
  be, since it may be that one's. What the loaded workspaces could not
  offer is said after the rest, as an install says it, so what was passed
  over has its reason beside it.
- **Nor one whose artifact cannot be read.** A skill whose front matter no
  longer parses, or a folder that cannot be listed, offers nothing, so what
  was installed from it reads as orphaned too, as `status` says; but it is
  still there, somebody's to mend, and failures are collected, never a
  reason to take out what they are about. So a record is passed over while
  the place it came from, the workspace's own (`install::own_failures`) or
  a loaded workspace's (`LoadFailure::Artifact`), has a failure about the
  artifact's place or a folder it sits in (`install::orphaned`, by
  `lifecycle::place`, since what cannot be read cannot say its name). The
  workspace's own failures are said first, before the loaded workspaces',
  as the own is offered first.
- **None is no failure.** A project with nothing orphaned is as it was
  asked to be, so it says nothing, and every project asked having none is
  said in one line, exit zero; a project that cannot be surveyed, or an
  uninstall that fails, is an error of its project while the rest stand.
- **No ledger is made to say so.** The ledger is opened to write only when
  there is one, as an update opens it: with none, nothing was installed to
  be orphaned.

### What no copy holds

A skill's own `.claude-plugin` folder, at its top, goes into no copy
(`install::LeftOut`), and the install says so with a `warning:`: Claude loads
a skill folder holding one as a plugin, with the hooks and MCP servers it
declares ([docs/agents.md](agents.md), docs-only, so it is left out whatever
Claude does with it). A skill is instructions an agent reads when it chooses
to; a plugin's hooks run commands on their own and its servers are started
for it, which installing a skill must not turn on behind anybody's back.
Gathering a plugin's skills brings that folder along, and the shelf keeps it,
as it keeps whatever a source held.

Left out of every install, whichever agents the project names, rather than
only out of Claude's copy: one copy differing from the other is what D-07
rules out, and a folder a project's `targets` name may be Claude's too. Only
at the top, where Claude looks: further down, a skill that teaches plugin
writing ships one as an example, and it is the skill's like any other file.
Left out rather than copied and deleted, so nothing in it is read, and a link
in it pointing outside the source refuses nothing. A copy is compared with
its offer without it (`copy::same_leaving`), so the next install of an
unchanged skill is `Unchanged`, and the install screen still knows which
offer the project holds.

### Never into an agent's own folder

A target is relative to its project, so in a project whose root is the
user's home `.claude/skills` is Claude's skills for every project, and
`.agents/skills` the one Codex and OpenCode read everywhere. An install there
would be the user-scope install
[D-22](#d-22-user-scope-installs-come-later) leaves for later, behind a
`--user` of its own, and nothing stops `mind init` in a home. So an install
whose copy would land inside one of the agents' own folders is refused
(`InstallError::UserScope`), naming the folder and who reads it, before
anything is looked at or written.

The table cannot tell, and core has no home to compare with: the front end
hands `install` the agents' folders (`AgentHomes`, [the agents' own
folders](#the-agents-own-folders)), resolved as far as each exists, so a home
reached through a link is still the folder it is. Inside rather than equal,
so a project kept in one of them, under `~/.claude` say, is refused too: it
is an odd place for a project, and nothing there is any one project's.
Refusing a project at the home outright was the other way, and was turned
down: whether a folder reaches every project depends on where the agents
read, which `AgentHomes` already says, not on where the user's home is, and
a home moved with `$CLAUDE_CONFIG_DIR` moves what is refused with it.

### Adopting what a project already keeps

Many repositories keep skills written by hand in `.claude/skills` or
`.agents/skills`, committed with the code. Each lists as foreign, and
foreign is inert both ways, so nothing Mindflayer does reaches one: no
update, and no copy for an agent that reads the other folder. `flayer adopt
-p <project> <name>` (`adopt::plan`, `adopt::apply`, #59) is the one way a
copy nobody installed becomes recorded: asked for by name, never done by a
scan or an install, it copies the skill into the workspace's library and
records the project's copies as an installation from there, with the hash
they hold. The project's bytes are never written: what is there is what is
recorded, and from then on it is managed as anything installed is, current
while nobody changes it, edited once somebody does, outdated once the
library copy moves on ([an edited copy is left
alone](#an-edited-copy-is-left-alone)). What a project authors stays its
own ([D-06](#d-06-projects-keep-authoring-beside-the-code)); this is about
what sits where its agents read.

- **Held to what an install could have written.** Every later look compares
  the copies with the recorded hash and with the library copy, so both are
  made from the bytes as they are: the hash is `hash::tree` of the copies,
  the library copy is made from the first with `copy::tree` and refused
  unless it hashes the same, and the record names the workspace's own as its
  origin. What no install writes is refused rather than recorded: a link,
  where the copy is or inside it, which the hash refuses and an install
  never leaves; something at the copy's place that is no folder; and a
  `.claude-plugin` at its top, which an install leaves out of every copy
  ([what no copy holds](#what-no-copy-holds)). Recorded with it, the copy
  would read as outdated at once, since an install's copy holds no such
  folder (`copy::same_leaving`), and the next update would take it out,
  turning off the plugin's hooks and servers behind somebody's back;
  recorded without it, the copy would hold what the record does not. So it
  is said, and moving it out of the skill is somebody's to do first.
- **Strict, as installing is.** The library copy is a write, so the skill is
  held to what an install refuses one over (`artifact::strict_issues`),
  filed under its name: a name that breaks the rules or does not match its
  folder, a description empty or too long.
- **Every copy one skill.** Copies in two targets that do not hold the same
  are refused, both named: one hash is recorded, and which of the two is the
  skill is not something to guess.
- **Only where an install writes, every target recorded.** It looks in the
  targets an install of it uses (`install::targets`), and records each,
  those with no copy too, so one with none reads as missing, and the next
  update puts a copy there from the library, as for any installation that
  lost one. Adopting writes nothing in a project, so it is the update, run
  when somebody chooses, that gives Codex and OpenCode a skill a repository
  kept only in `.claude/skills`. Recording only the copies found was turned
  down: current at once, but the agents of the empty folder would go
  without it and nothing would say so, until the next install recorded the
  copies afresh. So was refusing: most repositories keep one folder, and a
  new project names all three agents. A copy only in a folder no install
  writes, `.opencode/skills` say, is not adopted, since no install or update
  would reach it there and the next would make it somebody's again.
- **Somebody's, nothing else.** Not one the ledger records already, from
  wherever it came, which is Mindflayer's to update or uninstall; not one in
  the folder the project authors in, which a `mind.toml`'s target always is
  and settings may make one, since that is the project's own work and a
  change from the project reaches it; not one inside an agent's own folder
  ([never into an agent's own folder](#never-into-an-agents-own-folder)),
  nor one where a loaded workspace keeps its own. A rule is refused too: one
  reaches a project rendered from the library as a section of its
  `AGENTS.md`, and taking rules out of a hand-written one is a change of its
  own.
- **One name in the library.** A name the library holds already, by its
  folder or by what one of its skills declares, is refused with both paths:
  the workspace would offer two skills of one name. The copy's folder is
  made with `create_dir`, which refuses one made in the meantime, and the
  folder of the workspace's own skills with it when it is missing, as the
  first one added makes it. Built in place rather than beside: no agent
  reads the library.
- **Out of git only what git does not track.** In a local project each copy
  git tracks nothing in is kept out of git before anything else is written,
  as an install's copies are, so a block that cannot be edited refuses the
  whole adoption ([what keeps an install out of
  git](#what-keeps-an-install-out-of-git)). One git tracks, as a committed
  `.claude/skills` is, gets no line, then or after an update: a line would
  keep none of it out and hide every file added to it later. It stays the
  team's in git, and an update writes over it there, for the change to show
  in `git status`. Local mode edits a tracked team file only with
  `edit_tracked` ([D-02](#d-02-installs-stay-local-unless-a-project-shares-them));
  that is about a file Mindflayer edits inside its marks, an `AGENTS.md` or a
  config file, and a skill folder somebody asked by name to be managed is
  consent for that folder, which the dry run and a `note:` say. A shared
  project's exclude file, and that of one a `mind.toml` marks, is never
  opened.
- **One lock, planned again under it, together or not at all.** `apply`
  opens the ledger to write first, since its scrub can take the lock
  itself, then takes the workspace's lock, reads the project again and plans again,
  so a plan shown a minute before decides nothing. The exclude lines, the
  library copy and the record follow in that order, and a step that fails
  takes back what came before it: the library copy goes, and the exclude
  lines this added, not one the group held already. The action log is
  written last, as after an install.
- **A dry run without `--yes`**, which reads the ledger without making it and
  writes nothing, as every dry run does, and exits zero: what it gives is a
  line for the library copy, one for the copies recorded, one for each
  target with no copy, and in a local project one for each copy kept out of
  git, to read before saying yes. Resolved as a read with `--yes` too, as an
  update is: a workspace made for the occasion links no project to adopt
  from.
- **Where `gh skill` got it, shown.** A `SKILL.md` that `gh skill`
  installed says where it came from under `metadata` keys starting
  `github-` (`adopt::PROVENANCE_PREFIX`). The plan hands those back as
  written, and the CLI shows them under its first line; nothing reads them
  further, and no other tool's lockfile is read or written. Taking them for
  a source to gather from would be a gather, which is its own command.

The TUI offers it on a row `status` calls foreign, with `A`, which asks and
then runs `flayer adopt -p <project> --yes skill:<name>`.

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
  being worked on. An installation from it records no source id, like the
  workspace's own, because there is no shelf row to point at, only its
  origin, `load:<entry>`.
- **One level deep.** Only a loaded workspace's own artifacts are offered,
  never what it loads, so two workspaces loading each other is not a loop, and
  what a workspace offers is what its marker names, not a graph.
- **Into the workspace that is not the source.** Run from inside the workspace
  being loaded, the first workspace found walking up is that one, and a
  workspace cannot load itself. `resolve_except` walks on past it (and past
  every path given) to the next one, or the default in your home; loading the
  home workspace from inside it is refused. One named with `-W` or activated
  is taken as it is, since it was chosen, and loading it into itself is
  refused by the load: by `load_all`, before any of the paths given is
  written, so a source listed ahead of it is not left loaded by a command
  that failed.

A loaded entry's origin is `load:<entry>`: disjoint by construction from
`workspace` and from any shelf source's address, even a repository gathered
from the very directory that is also loaded. What `gather list` prints is what
`--from` takes; `install::resolve_from` also turns the path given to `load`,
typed from anywhere, into that origin.

A loaded workspace is read live and is usually somebody else's, which puts
installing's trust in the name a `SKILL.md` declares under strain. So:

- **A name must be one folder name** (`install::is_folder_name`). It is joined
  onto the project's folder, and replacing starts with deleting: `..` would
  delete the project, an absolute path anything at all. Such an artifact is
  not offered from anywhere, and `install`/`uninstall` refuse it outright. A
  rule's route is held to the rules every name is instead
  (`install::is_installable_name`), so it never climbs out of the folder it
  is filed in.
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

What a load offers is every kind, and what it cannot offer (a workspace that
has gone, an artifact that cannot be read, a name that is refused) is
collected by `install::offered_by_loads` and said as `warning:` lines wherever
offers are shown or used, so what `flayer load`, `gather list` and the TUI
count is what can be installed. The install screen's line for a row it could
not apply says why, and the lines after it run all the same.
Two spellings of one loaded directory are one source; `load_all`,
`unload_all` and `unlink_all` rewrite the marker once, for all of the paths or
none.

### What keeps an install out of git

A project in local mode
([D-02](#d-02-installs-stay-local-unless-a-project-shares-them)) keeps what is
installed into it out of git, through a block of anchored entries in
`$GIT_COMMON_DIR/info/exclude` that `exclude` keeps. Which paths to exclude
is the installing side's to choose, and `exclude` only writes them down: an
install of a skill names one entry for each copy git tracks nothing in, a
render of Claude's rule
files one for each file ([Claude's rule files](#claudes-rule-files)), a
render of a project's rules one for an `AGENTS.md` it made ([installing
rules](#installing-rules)), and #94 will for MCP config.

```
# mindflayer:begin
# project 4f9c2a7e1b3d5f60a8c4e2b6d0f1a3c5
/packages/app/.agents/skills/commit-style/
/packages/app/.claude/skills/commit-style/
# project 0123456789abcdef0123456789abcdef
/packages/web/AGENTS.md
# mindflayer:end
```

How an install uses it:

- **Before anything is written.** The entries go in before the copies are
  put in place, as a local project's `.gitignore` goes in before its
  settings, so no copy shows in `git status`, not even for a moment. A block
  that cannot be edited (`InstallError::Exclude`) refuses the install with
  nothing written: copies one machine has would otherwise sit in every
  teammate's way. Copies that then fail to land take back out the entries of
  every target left empty, since a line naming an empty place would keep
  whatever somebody writes there later out of git without a word.
- **Never a copy git tracks.** An ignore rule leaves alone what the index
  holds, so a line for a copy git tracks keeps none of its files out, and
  hides from `git status` every file added to it later, where a teammate
  would never see it to commit. So an install writes a line only for each
  copy git tracks nothing in (`install::untracked`), asked of the index as
  `exclude::tracks` asks it: a copy that lands where nothing was, as most
  do, is untracked, while one a team committed, as an [adopted
  one](#adopting-what-a-project-already-keeps) often is, or one a project
  that was shared kept when it turned local, stays in git, and what an
  update writes over it shows as the change it is. Mindflayer runs no git
  command and leaves the index as it is. An index that cannot be read takes
  nothing for tracked, and the exclude edit that follows reads the same
  repository.
- **Local projects only.** A shared project's copies are the team's to
  commit, and a project a `mind.toml` marks was committed by design, so for
  those the exclude file is not opened at all, by an install or an
  uninstall. A project whose share mode changes has its group rewritten
  then: forgotten whole when it turns shared, and given an entry for each
  copy the ledger records in place when it turns local ([when a project's
  agents, targets or share mode
  change](#when-a-projects-agents-targets-or-share-mode-change)).
- **Keyed by the project's id**, from its settings, which stays when the
  project is renamed or moved within its repository. A line names a path
  below the project's own directory, so the group's lines are carried
  whole when either changes (`exclude::carry`): to where a relinked project
  is now, anchored again there, in the same repository or the one it moved
  into, read from the folder nearest the old place that is still there
  ([an entry whose project moved](#an-entry-whose-project-moved)), and back
  to the old place, reached the same way, when the entry cannot follow; and
  to a copy's new id once `mind init --new-id` gives it one, before its
  settings say so, and back when they cannot. A copy elsewhere in the same tree keeps its own lines under the old
  id, since those name its copies. A repository moved whole has its lines
  right already, its exclude file with it.
- **Taken out after the copies.** An uninstall removes the copies, then the
  record, then the entries, so no copy shows in `git status` on its way out.
  A block that cannot be edited then is a `warning:` beside what was done
  (`Removed::Removed`'s `still_excluded`), not a reason to undo it: the
  copies are gone, and only a line naming their empty place is left. A
  change from the project that removes an installed copy, in a folder it
  authors in that is also a target, takes that copy's entry out the same
  way, and one that renames it moves the entry to where it went ([A record
  must not outlive the copy it is
  about](#a-record-must-not-outlive-the-copy-it-is-about)).
- **Outside git, a note.** A project outside a git work tree gets its copies
  and a `note:` saying nothing keeps them out of git
  (`Excluded::warning`), not a `warning:`: nothing went wrong, and a project
  need not be in git, but a repository made there later would offer them all
  to be committed.

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
  those. The key is whatever the caller names the project by, which for an
  install is the project's id (#29), and it is compared, never parsed. The
  same path in two groups is two claims on it: dropping one leaves the other
  excluding it. A group goes with its last entry, and the block with its last
  group.

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
  but it carries a line to say (`Excluded::warning`): whatever local mode
  installs there is kept out of nothing, and a repository made there later
  would offer all of it to be committed.
- **Found with `gix`, opened isolated.** No `git` binary, and neither the
  environment nor the user's git config takes part, so a `GIT_DIR` set in the
  shell cannot send one project's entries into another repository's file.
- **No version.** Unlike a marker, the block holds nothing but git's own
  pattern syntax and two kinds of comment, and an edit rewrites only its own
  group and keeps every other line where it is, so a block a newer Mindflayer
  wrote survives an older one's edit.

### When a project's agents, targets or share mode change

`mind agents [add|remove] <agent>...` and `mind share local|shared
[--edit-tracked]` (#37), `mind targets set <kind> <dir>...` and `mind targets
reset <kind>` (#40), and `flayer agents`, `flayer share` and `flayer targets`
with `-p` for a project the workspace manages, change a project's settings,
and with them where what is installed into it belongs: a copy in each folder
an agent it names is sent one ([D-07](#d-07-one-copy-per-agent-folder)), or
in each its `targets` name ([a project's own
targets](#a-projects-own-targets)), kept out of git while it is local
([D-02](#d-02-installs-stay-local-unless-a-project-shares-them)). Core's
`reapply` decides all of it, `reapply::plan` writing nothing and
`reapply::apply` doing it, so the TUI's toggles and form and the command line
say the same. A change of targets is one more `Change`, the whole list of a
kind's folders or none, brought to the target set as the agents are, so
everything below holds of it too: `targets set` moves Mindflayer's untouched
copies into the folders it names and out of the ones it stops using, leaves
somebody's at a new target where it is and says so, and lists what sits
unrecorded in a folder it drops; `targets reset` is the same the other way.

- **A dry run without `--yes`**, as `remove` and `flayer migrate --library`
  are: a line for each thing that would change, the settings first and then
  each copy, so what an agent change does to the installs is read before it
  is done. Made, the same lines say what was done.
- **Brought to the target set, not to the difference.** Every installation
  the ledger records is compared with where the settings, as changed, send
  copies, rather than with what the change itself moved. The same change run
  again does nothing once it is done, and finishes what one that stopped
  halfway, or a hand edit of the settings, left undone. Planning from the
  difference would leave both to somebody's memory.
- **A new copy is made from a copy, not from the offer.** A folder an agent
  the project now names is sent copies to is given a copy of one the record
  names that still holds what was installed, by the hash every copy was
  recorded with ([D-04](#d-04-an-edited-copy-is-detected-not-disowned)),
  built in a stage and put in place as an install's copies are
  (`copy::Stage::place`), and the installation records one more target, its
  hash, origin and time as they were (`Ledger::retargeted`). Installing
  again from where it came from was turned down: that is an update, which
  can bring a newer revision into one folder while the other keeps the old,
  the very thing D-07 rules out, and the offer may have gone since. An
  installation none of whose copies still holds what was installed is given
  none, and that is a warning (`Why::Unsourced`).
- **Only an untouched copy is taken out.** A recorded copy in a folder no
  agent the project names is sent copies to any more is removed when its
  hash is the one recorded and forgotten when it has gone already.
  Otherwise, changed since it was installed or not to be compared (a record
  with no hash, a link), it is left where it is and dropped from the record
  with its exclude entry: its author's now. That is not what an edit does to
  a copy where an agent reads, which stays recorded and reads as edited
  ([an edited copy is left alone](#an-edited-copy-is-left-alone)): this one
  is in a folder no agent the project names is sent copies to, which no
  install writes again. Deleting it on a guess was turned down, and so was
  keeping it recorded, which would name a copy in a folder no listing of the
  project reads, call the installation missing or edited for good, and leave
  it to the next install, which records its copies afresh, to drop all the
  same. Which
  folder a copy is in is told by the folder on disk, as `install::targets`
  makes two spellings of one folder one target, not by the route recorded:
  a copy recorded in `.agents/skills` stays when codex goes and Claude reads
  it through a `.claude/skills` linked there. It is recorded from then on by
  the spelling it is sent copies to (`Retarget::respelled`), its exclude
  entry with it, since that is the one discovery reads it by and an install
  looks for its record by: left under the old one, as it once was,
  Mindflayer's own copy read as somebody's, and `targets reset` with the
  agents still linked to `.skills` made the next install refuse it.
- **Somebody's is left, and said.** A folder where a new copy would go that
  holds something unrecorded is not written over, so that agent reads it
  rather than what was installed: a warning, as an install refused over
  somebody's skill is. Refusing the whole change over it was turned down: an
  install is refused because it would write a second skill of that name where
  an agent reads both folders, and the change writes no copy beside
  somebody's, while which agents a project names is its own to choose. What
  sits unrecorded in a folder the change stops sending copies to is left too,
  and listed, since no listing of the project reads that folder once no agent
  it names is sent copies there. The folder the project authors in is not one
  of those, being read whatever its agents. A change that sends a kind only
  to folders all holding somebody's skill of a name leaves that installation
  no copy, and the ledger forgets it, as an uninstall would: the settings
  said where its copies go, and somebody's is there already.
- **Turning shared forgets the whole group; turning local fills it.** A
  project turning shared has its `.mind/.gitignore` removed, only while it
  holds what Mindflayer writes (`MIND_IGNORE`), so one changed by hand stays
  and is said, and its group of the exclude block forgotten whole
  (`exclude::forget`), every line whichever workspace wrote it, since each
  names a copy that is now the team's to commit. One turning local is given
  the `.gitignore` and an entry for each copy the ledger records in place
  that git does not track and the group does not hold yet, both before its
  settings say so, as
  `mind init` writes the `.gitignore` before the settings. `edit_tracked` is
  written with the share mode every time, off unless `--edit-tracked` is
  given, so one command line says the whole of how the project shares.
- **Refused before anything is written**: a project a `mind.toml` or a
  `settings.json` marks, which has no `settings.toml` to change until `mind
  migrate` moves it to one ([moving a project to its
  settings](#moving-a-project-to-its-settings)), and is pointed there
  (`MindProject::changeable`); a change that leaves it naming no agent, which
  would take every copy out, an uninstall's to do by name, or that installs a
  kind in no folder, for the same reason; a target no install may use ([a
  project's own targets](#a-projects-own-targets)); and a copy that would land
  in an agent's own folder, or in one a loaded workspace keeps as its own, as
  an install refuses one.
- **Held by the workspace's lock.** `apply` takes the lock of the workspace
  that links the project from before it reads the settings again until
  every copy, record and exclude entry is written, so an install in that
  workspace comes wholly before it or wholly after it.
  `MindProject::change_settings` takes none of its own, as
  `config_entry::ConfigFile` takes none: what keeps its edit apart is its
  caller's, and here the change is a workspace's as much as the project's.
  A lock in `.mind`, the other answer the open question offered, would keep
  a second workspace linking the project away from the settings but not
  from its own installs into it, which go by that workspace's lock; that
  case stays open ([open questions](#open-questions)).
- **A project nothing links still changes.** Its settings are its own
  ([D-09](#d-09-project-settings-live-in-the-project)), so in a project no
  workspace `mind` consults links, `mind agents` and `mind share` change
  them, move no copy, and say with a `hint:` that nothing installed was
  carried along, as `mind status` says why nothing reads as installed.
- **In an order that shows nothing in `git status` for a moment**: the
  exclude entries of the copies about to land and the `.gitignore` of a
  project turning local, then the settings, then the copies and their
  records, and last what keeps out of git what has gone, or what a project
  turning shared means to commit. A refusal before the settings are written
  leaves everything as it was; what fails after is collected and said, and
  the same change run again finishes it.

### The screen, and why it is three files

`flayer install` is a two-column screen: projects on the left, and on the
right what can be installed, as seen from the project under the cursor, or,
turned round, the other way about. It is split so that only one of its three
parts needs a terminal:

- `install/state.rs`: what a key does, and the command lines what was
  marked amounts to. Marking a box is a statement of intent and touches
  nothing; it is a plain state machine a test drives by pressing the keys a
  person would.
- `install/ui.rs`: what the screen looks like. Rendered into an in-memory
  terminal by the tests, so the exact text a user sees is asserted on, the same
  way every other command's output is.
- `install.rs`: the loop that reads a real keyboard, and running the lines
  what was marked amounts to. The loop is the only part with no test, and it
  holds nothing but the loop for that reason.

What is marked is applied as the command lines it amounts to
(`Screen::task`), kind by kind: a
`flayer install -p <project>... --from <origin> <kind:name>` for each offer
ticked, with a `-p` for each project ticked to take it, and a
`flayer uninstall -p <project>... <kind:name>` for each artifact unticked,
which takes it out whichever offer its copy came from. They are the TUI's `Install` and `Uninstall` tasks, which the TUI
runs as it runs any task, and which `flayer install` runs the same way
(`install::apply`): each line parsed and run from the workspace root by
`tui::execute`, with the screen's `Lookup`, so its agent homes and the
workspace it was opened on reach every line; and what each said is printed
afterwards as the TUI's transcript prints it (`state::transcript`), the line
and then what it said, with a non-zero exit when any did not go through. So
the report shows exactly what ran, and a session with the screen in it can be
run again without it. Every line is run whatever came of the ones before
(`Task::chained`), since each is boxes marked one by one, where a task's
lines otherwise stop at the first that fails. Inside a line, a project
refused is said as its own and the others' boxes are carried out, as the
command does for several `-p`; but a `-p` that picks nothing by the time
the line runs, a project unlinked from another terminal since the screen
opened or one whose marker no longer reads, refuses its whole line, before
anything is done for the other projects on it, as it does typed ([one
artifact into many projects](#one-artifact-into-many-projects)). The
screen's picture of the workspace has gone stale under it, and every line
naming that project says so. A line a project and an artifact would have
kept the others going, and is rejected: one artifact's ticks are one line,
which is what the transcript is read for, and a workspace changed under
the screen is no reason to change the rest without asking again.

- **`--from` on every install line**, though a name offered from one place
  needs none: the box is one offer, so its line installs that one or nothing
  when run again after another place starts offering the name, and whether a
  name needs `--from` is core's to say (`install::offered`), not the
  screen's to work out again.
- **`-p` as core says** (`FlayerWorkspace::naming`): a project's entry, or
  `./<entry>` when another project declares the entry as its name, which
  `pick` would otherwise refuse as naming two.
- **What a line warns of is the command's.** A line says what the command
  says typed by hand: a project's `AGENTS.md` git tracks, say, is said by
  each rule's line that renders the project's rules, where the screen, when
  it carried out its marks itself, said it once. That was the alternative,
  rejected: a second front end to installing, row by row through core with a
  report of its own, which said what the commands say a second way and left
  the transcript a `flayer install` nobody could run again.
- **What the lines said goes to stdout**, warnings and errors included, each
  under its line, as the TUI prints a session: on stderr, a warning would be
  cut off from the line it is about. `flayer install` puts on stderr only
  what its screen says itself (`install::finish`): a project that would not
  open, and what the loaded workspaces could not offer, which every install
  line says as it does typed by hand, so the screen says it only when no
  install line ran, rather than once more than the lines did.

The right column holds every kind's offers, filtered to one kind at a time:
`tab` shows the next (`Screen::kind`), skills first, and a project's marks of
every kind are applied together. A filter rather than a column a kind, so the
screen keeps two columns however many kinds there are; and rather than every
kind in one list, where a skill and a rule of one name would sit side by side
as two boxes for what reads as one thing. A row is one kind and name, so
ticking one offer unticks another of the same kind and name, and leaves a
rule of a skill's name alone. Which offer of a rule the project holds is the
one its record came from (`install::is_current`), since nothing in a project
holds a rule's bytes alone to compare. A project offered nothing of the kind
shown says what core says it cannot take (`install::receives`), or where that
kind comes from: a rule from the workspace's own or a load, never the shelf.
A row whose box cannot move says why: somebody's, which nothing of
Mindflayer's touches, and an install edited since, which the screen never
forces ([an edited copy is left alone](#an-edited-copy-is-left-alone)); every
offer of the name stands as its one installation, so none of them can be
marked over it either.

`v` turns the screen round by artifact: every offer of the kind shown on the
left (`Screen::artifacts`, each kind, name and origin once,
`Row::same_offer`), and every project on the right, ticked where it holds
the one under the cursor. Not a second set of marks: each box is still a
project's row, found by its offer (`Target::row_of`), so ticking one goes
through the same `toggle`, unticking another place's offer of the name as it
does the right way round, and what is applied is the one batch, however it
was marked. Turning keeps the box under the cursor, and the keyboard on the
same side. A project with nowhere to put the kind is offered none of it, so
turned round it has no box and says why, as it says so in place of its
offers the right way round. The key is `v` rather than `tab`, which the view
would have taken before rules could be installed: `tab` shows the next kind,
which means the same either way round. Either way round, one artifact's
ticks are one `flayer install` line and one `flayer uninstall` line, a `-p`
for each project, since each takes several ([one artifact into many
projects](#one-artifact-into-many-projects)).

The line of keys is cut short at the terminal's edge rather than wrapped, so
a long one never pushes what is waiting, or why a key did nothing, off the
bottom, and the keys are two spaces apart, as the TUI's are, so the longest
fits 80 columns.

`try_init` rather than `init`, here and in the TUI: these are the commands that
need a terminal, and being run without one deserves a sentence rather than a
panic.

## What each project has

`flayer status` answers vision point 1, which skills and rules each project
has, and from where (`status::Status`). It is built from discovery and the
ledger together, since neither answers alone. Discovery says what sits in the
folders a project's agents read and the one it authors in, and which agents
read each copy; it reads no ledger, so it cannot tell an install from
somebody's own. The ledger says which copies Mindflayer put where, and from
where (`Ledger::records`); it cannot see that one has gone since, nor a copy it
never wrote.

Each artifact a project holds is one entry, as discovery lists it: every copy
of one name together ([read where its agents
read](#a-project-marked-by-its-settings-is-read-where-its-agents-read)). Its
state is one of seven (`status::State`), the first five an installation's
[standing](#an-edited-copy-is-left-alone), told by the same core function an
install refuses by and an update acts on (`install::standings`), so the
report, a refusal and an update never disagree:

- **current**: the ledger records every copy, every copy it records is found
  where it was put holding what was written there, and the place it came
  from offers that still.
- **outdated**: the same, but the place it came from offers something else
  by now. An update brings it up to date ([bringing what was installed up
  to date](#bringing-what-was-installed-up-to-date)).
- **edited**: the ledger records a copy that holds something other than was
  written there, by its hash. A rule is edited in its section of the block
  or in its file for Claude. Still Mindflayer's install, left alone by every
  install, update and uninstall unless forced.
- **missing**: the ledger records a copy discovery does not find where it
  was put: deleted since, by hand or by a `git clean`, which the ignored
  copies of a local project are exposed to, or left so it no longer reads,
  which discovery says beside the report. An update puts every copy back,
  since it writes each target.
- **orphaned**: the place it came from no longer offers it, so nothing can
  bring it up to date or put a copy of it back; `uninstall` takes it out.
- **foreign**: unrecorded, in an agent's folder outside any the project
  authors in: somebody's, never Mindflayer's to change unless somebody
  adopts it ([adopting what a project already
  keeps](#adopting-what-a-project-already-keeps)).
- **authored**: unrecorded, in a folder the project authors in: its own work.

They took over from `installed`, now current, outdated or orphaned, and
`partial`, now missing, which is a name a script read changing what it
holds, so the JSON envelope moved to version 2 ([JSON, in one
envelope](#json-in-one-envelope)). Keeping `installed` and adding the
standing beside it, a field that would have moved no version, was turned
down: two words for one entry, which the table and the TUI would each have
had to choose between.

The state is discovery's answer (`Artifact::provenance`) with the standing
of the install laid over it, in the order a standing is said in (edited,
orphaned, missing, outdated): one of the five only when every copy is
recorded, and missing when the record names one that is not found, even
beside a copy the project authors of that name. That copy sits where the
install wrote nothing, or it would be the install's, so the next update puts
the gone one back all the same, and calling the entry authored would leave
an agent reading none of it unsaid. Not beside a foreign copy, which is said
first: the next install of the name is refused over it. Nor beside an edited
one, said next, for the same reason unless the update is forced, nor an
orphaned one, which nothing can put back. The record is
matched to the entry by the path of a copy it wrote, as discovery marks a copy
installed, never by name, so an install of a name into one folder neither
claims nor lends its origin to what somebody keeps under that name in another.
A recorded copy is missing when discovery did not find it, rather than when
nothing is at its path: a folder whose `SKILL.md` was deleted, or no longer
parses, loads in no agent, where asking the disk would have called it
current and reaching nobody. Its standing calls it edited, since it no longer
holds what was written, and the report says edited over missing, as it does
for any edit. A copy still holding what was written where discovery no
longer looks is the other way round: missing in the report, and current to
an update ([an open question](#open-questions)).

An install nothing of which is found is still in the report, missing, or
orphaned when nothing offers it any more, and reaching no agent, so that
what was installed is said once its every copy has gone. Unless the project holds something else by that name: that is then the
name's one entry, said as what is there, since a row is one name, as in every
listing, and the name is somebody's now ([an open
question](#open-questions)).

Beside its state, each entry names the agents among the project's that read
one of its copies (`Artifact::agents`), which for an install with a copy
missing are fewer than it was installed for, and where what was installed
came from, the record's origin, as `--from` takes it. Each project names its
own agents too, from its settings, so a front end can lay its rows out by
agent; a project a `mind.toml` marks names none, and reaches none as far as
a report can say.

The rules a project marked by its settings received are no copies discovery
finds, but sections of the block in its `AGENTS.md` and Claude's files, so
each is listed from its record and the block, an installed view
([D-23](#d-23-what-a-project-received-is-an-installed-view)): catalog
discovery parses no block, since a section is never authored in place but
rendered from a rule kept elsewhere. A rule is missing while a target its
record names is not found, its route among the block's sections or its file
for Claude where it was written, as a skill's copies are, and otherwise
stands as core stands it; one recorded with none has nothing missing. It is
edited, over missing, while its section or its file holds something other
than was rendered there, and orphaned while nothing offers it. It reaches
Codex and
OpenCode through the block, and Claude through its own file, or through the
block where the render gave it none: a render gives Claude every rule as a
file while Claude reads no block, so a rule in the block with no file of its
own is one Claude reads there (a file that was somebody's when the render ran
is the one case this tells wrong, and the render said it). A block that
cannot be read holds none of them, and the next render says why. A rule
copied into a project a `mind.toml` marks is discovered as its copy, as a
skill is. A name the project authors a rule of too keeps the one entry
discovery gives it, as a skill's does.

Each rule a project marked by its settings received that some of its agents
read nowhere is named in `missed`, with those agents, and is a warning beside
the report too: one about some files, which Claude alone reads, or one Codex
and OpenCode read nowhere because its block could not be written. Of each
rule rather than of its kind, as `unreached` says a skill's: a skill reaches
the agents that read the folders its kind goes to, the same for every skill,
while a rule reaches the ones what it states, and where its block may go, let
it. Only one installed whole, current, outdated or orphaned, from the rows
as the report lists them: one partly gone is missing already, which says
so.

Each project also names, in `unreached`, every kind it installs only in
folders some of its agents do not read, with those agents: what a project's
`targets` can leave ([a project's own targets](#a-projects-own-targets)),
and nothing under the standard folders, which each agent reads. It is a
warning beside the report, with a non-zero exit, as anything that hides an
install from an agent is: a missing install says one copy is gone, and this
says every copy is where an agent does not look. A folder the settings name
in `targets` that no install uses is a failure beside the report too
(`StatusFailure::Target`), since the report reads the installs as going
everywhere else they name. Both are in the TUI's rows ([where a project
installs skills](#where-a-project-installs-skills)).

- **Every project the workspace links, in its marker's order**, each
  directory once however many entries point at it, as every screen lists
  them, and each project's entries by kind and then name, sorted in core. Not
  the workspace's own, which are no project's and which nothing is installed
  into.
- **A stale entry is listed, not left out**, with why it would not open, and
  said as a failure beside the rest, as `flayer list` says one: it is the
  entry somebody has to find to unlink it.
- **An older marker is a hint.** Each project names the file its marker is
  read from (`ProjectStatus::marker`, `marker` in the JSON), and one an
  older Mindflayer wrote, a `mind.toml` or a `settings.json`, is a `hint:`
  after the report naming `mind migrate`, as the TUI's header points at the
  move ([moving a project to its
  settings](#moving-a-project-to-its-settings)). Nothing else in a report
  tells a `settings.json` from a `settings.toml`, which name the same
  agents, though only the second can be changed. A hint, not a warning: the
  project is read whole and the report is true of it, so the exit status is
  the report's. On stderr under `--json` too, as `mind status`'s hint about
  its workspace is, since the data says it in `marker`.
- **Read, not made.** The ledger is opened with `ledger_for_reading`, so a
  workspace that never installed anything is left without one, and a default
  workspace nothing has made is reported as one with nothing linked, as
  every read resolves it. Core opens it, in `ProjectStatus::read`, rather
  than each front end, which let the CLI refuse a report over a ledger that
  would not open while the TUI drew its rows without it. A ledger that
  cannot be opened or read leaves every copy unrecorded, as `with_records`
  does, and is said once in a report, and on each project's rows in the TUI,
  never a reason to hide the rest.
- **`mind status` asks the workspace `mind` consults** (`resolve::consulted`):
  the one named, activated or above, if it has been made. One that does not
  link the project, or none at all, leaves it without records, and it is
  reported all the same, as `mind list` reports a project nothing links, with
  a hint saying why nothing in it reads as installed. Refusing, as `mind
  install` refuses without a workspace that manages the project, was turned
  down: a read has nothing to protect by refusing. So one found that cannot
  be read is a warning beside the report, as it is beside a change, and the
  warning says why nothing reads as installed in place of the hint. One `-W`
  or the activation names that is no workspace still refuses, as it refuses
  every command: whoever chose it meant that one.

The CLI lays the report out as `list` lays out its own: a project column only
when its rows come from more than one project, or a stale entry's row, which
is nothing but its name, needs one; a kind column only when more than one
kind is in play; the agents comma-separated as `--agents` takes them, so the
column holds no space for a script splitting on spaces to cut, and `-` for
none, since reaching no agent is something to say. `--json` prints the
report as core built it ([JSON, in one envelope](#json-in-one-envelope)).

The TUI's rows show the same report. Each project is read once, through
`ProjectStatus::read`, which hands back the catalog it read beside the
report, so a row's summary, its problems and its state come from one
discovery rather than two. A row shows its state, then a badge in each of the
project's agents' places, blank where that agent does not read it, so rows
read down by agent. The two letters (`CC`, `CX`, `OC`) are the screen's, its
own initials for each agent, since Claude Code and Codex share the first. A
row of the workspace's own says neither, and an install nothing of which is
found has a row like the rest, from which `i` puts it back.

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
same screen on the project it was run in: inside the workspace that manages
it, where its neighbours and its installs are, or on the project alone when
none does. Alone, the keys that belong to a workspace say so instead of
running, and `l` is `mind link`, after which the screen is the workspace's.
A project's rows say how each artifact stands, as `flayer status` does ([what
each project has](#what-each-project-has)).

### Every change is a command line

A key that asks for work produces a `Task`, and `Task::commands` turns it into
the words a person would type: `flayer link beta`, `mind -C beta add skill
deploy 'Ship it'`. Those words are parsed by `Cli` and `FlayerCli`, the parsers
the binaries use, and run through `run_with` and `run_flayer_with`, what the
binaries' `run_watched` and `run_flayer_cli_watched` call once they have read
where to look for a workspace (`run` and `run_flayer_cli` are the same with a
watch nobody follows). The TUI has no code path of its own for linking, adding
or gathering, and it cannot grow one, because there is nowhere in it to put
one.

That is what turns "every feature has a command and a place in the TUI" from a
promise into a property. A feature is written once, as a command, and the TUI
gains it by producing that command's words. A test parses every task's lines
(free text shaped to break a naive command line included), so a task that could
not be typed cannot be written.

Two things follow, and both are deliberate:

- **A form shows what it is about to run, as it is typed**, as the command
  line. The TUI teaches the CLI rather than standing in front of it.
- **Closing the TUI prints a transcript**: every command that changed
  something, with what each said. The session can be read afterwards and
  repeated in a script. Commands that only read (show, validate, the shelf)
  are left out, because they changed nothing.

A `-C` or a `-W` in those words is relative to the workspace root, which is
how someone reading the transcript takes it, so it is resolved against the
root rather than against wherever the process started. What the screen was
opened with is passed along rather than spelled: its home, and the `-W` or
activation that chose its workspace (`tui::execute`), as they were given to
`flayer` itself. A command that opens a screen of its own
(`flayer`, `flayer tui`, `flayer minds`, `flayer install` with nothing named,
`mind`, `mind tui`) is refused if it ever reaches that path, rather than drawn
inside this one.

The exceptions are named, not hidden: opening the install screen builds the
screen `flayer install` opens, as a read, rather than running a line, and
re-reading the disk is not a command, because it changes nothing. Applying
the install screen is no exception: its marks are the `install` and
`uninstall` lines they amount to (`Task::ApplyInstall`, [the
screen](#the-screen-and-why-it-is-three-files)), run as every other line is,
so the transcript says what it ran in words that run again.

### Split three ways, like the install screen

- `tui/state.rs`: what a key does. It moves cursors, fills forms and returns
  tasks; nothing in it touches a file, so a test drives it with the keys a
  person presses.
- `tui/ui.rs`: what the screen looks like, rendered into an in-memory
  terminal by the tests.
- `tui/minds.rs`: the minds screen's rows and ticks, and the task they amount
  to; like `state.rs`, it touches nothing.
- `tui.rs`: the loop, reading the disk into what the screen shows, and running
  tasks. `step` is everything the loop does between a key and the next frame,
  and `settle` what it does before each frame while a task runs apart; both
  are public so a test can do exactly that without a terminal.

The loop draws a box saying what is running before it runs it, because a
frozen screen reads as a hung one. Keys pressed while it ran are dropped: they
were aimed at a screen that has since changed.

`mind edit` is the one command that needs the terminal itself, because an
editor draws a screen of its own. For it the loop steps aside (raw mode off,
back to the normal screen), runs the command as it runs any other, and builds
its own screen again from scratch when the editor closes. The task says so
(`takes_terminal`), so the loop does not have to know which commands are
editors.

### A gather runs apart

A gather is the network, and the one task that runs apart
(`Task::runs_apart`). `step` starts its lines on a thread of their own and
returns at once, with a [watch](#following-a-gather-and-stopping-it) whose
steps come back over a channel. Before each frame the loop calls `settle`,
which takes in the last step and, once the task is over, shows what came of it
exactly as `step` does for a task that ran in place. Everything else is the
local disk, over before anybody could read a count, and stays on the loop's
thread rather than paying for one of its own.

The box shows the command, the last step and that `esc` cancels. The screen is
the task's until it is over, so every other key is ignored, as one pressed
while a task ran in place is dropped. `esc` cancels the watch: the gather stops
before it places anything, and the form it came from says so, as for any gather
that did not get as far as a report. Once the step is placing, which is no
longer `cancellable`, the box says a cancel is too late instead, and the gather
finishes with its report. Leaving cancels it too, and `wait` holds the screen
until it has stopped, so what it did reaches the transcript and nothing is half
done when the process ends. A gather still connecting takes a cancel only once
it is connected, so `esc` there, or leaving, waits as long as the network does;
and since the loop goes on reading the terminal, a question ssh asks there is
raced for its answer, as the [open questions](#open-questions) say under
Private repositories. The lines are still the task's command lines, as
`App::lines` gives every task (`flayer init` first on the [default
workspace](#the-default-workspace) before it is made), parsed and run by
`execute`: the thread changes where they run, not what they are.

They go through a `Runner` on the way, which is the real parsers and commands
unless `App::runner` holds a stand-in. A gather from a repository on the local
disk is over before a frame could show it, so the test of the progress and the
cancel stands in a gather that goes on only when the test says so, and drives
it through `step`, `settle` and the keys as the loop would. The stand-in is
handed the words all the same, so even then there is nothing the screen runs
that is not a command line.

### What it asks before it writes

The link form offers what `flayer scan` finds under the workspace, or under
the folders `flayer minds` was given for the session it opened
([Scanning](#scanning)). A git repository that is not a mind project yet can
be picked, but making it one writes into it, so `mind init` is asked about,
not assumed, and a typed path that turns out to be a plain directory is asked
about the same way. Unlinking asks too, and says that nothing on disk is
deleted. Deleting one of the workspace's own asks with a second yes for
removing it everywhere ([removing one of the workspace's own
everywhere](#removing-one-of-the-workspaces-own-everywhere)), the line each
answer runs shown under the question. A command that fails from a form leaves the form open with the reason
in it, so a typo is fixed where it was made rather than typed out again.

### The minds screen

`m`, or `flayer minds`, lists every project the workspace links, ticked, and
every one `flayer scan` finds under it that it does not, unticked, all in one
list by path, because the question it answers is "which of these", not "which
kind". `flayer minds DIR` finds those under the folders it names instead
([Scanning](#scanning)). Ticking only marks. Applying turns the marks into
lines like any other task: `mind -C <path> init` for each repository that is
not a project yet, then one `flayer link` and one `flayer unlink`, each with
every path at once. So the screen needed `link` and `unlink` to take several
paths, and they take them **all or nothing**: every path is checked before any
entry is written, so a refused one leaves the registry as it was rather than
half changed by a command that reports failure.

### The agents and share toggles

`t` on a project's row opens a box for each agent, its share mode and
`edit_tracked`, ticked as its settings say, and applying them runs the lines
they amount to: `agents --yes <agent>...`, then `share --yes <mode>`, each
only when what it sets changes, with what they said shown in full, since an
agent change can copy and remove a skill in every project folder an agent
reads. The agents a project names keep their order, and one ticked is named
after them, so a toggle does not reorder the settings. A line cannot name no
agent at all, so unticking every one becomes `agents remove` of every one
named, which the command refuses for its own reason, in the form: the rule
that a project names an agent is core's, and the screen does not say it a
second time.

### Where a project installs skills

`T` on a project's row opens a form holding the folders its settings install
skills in ([a project's own targets](#a-projects-own-targets)), empty when
they go where its agents read, and applying it runs `targets set --yes skills
<dir>...`, or `targets reset --yes skills` once the folders are cleared, with
what it said shown in full, since it moves every copy. The folders are typed
on one line, a space between each, as a shell splits a line without quotes;
a folder whose name holds a space is named on the command line instead. A
form rather than toggles, since the folders are anybody's to name. Which of
them an install may use is core's to say, so the form takes what is typed
and shows the command's refusal in place, as the toggles do.

A project whose installs some of its agents do not see is marked on its row,
and its pane gets a strip saying so in `status`'s words
(`ProjectStatus::unreached`, and each folder no install may use), beside the
strip of what could not be read. Both strips are as tall as their lines once
wrapped, so the end of a sentence, which says what to do, is not cut off.

### Bringing a project up to date

`u` runs `update --yes` for the project under the cursor (`mind -C
<project> update --yes`), for the one artifact under it from the artifacts
column (`... update --yes <kind:name>`), and on the workspace's own row for
every project it links (`flayer update --all --yes`, with the artifact's
name when one is under the cursor), and shows what it said in full, as the
toggles do. It asks nothing first, unlike unlinking: an update writes over
nothing somebody changed, since the screen never forces, and what it leaves
is in the lines it shows. A project that cannot be opened is said in place
of running anything. `u` was unlinking's key until #48 gave it to the
update, the key that runs most often, so unlinking moved to `U`, which a
stray press reaches less easily and which still asks first.

The install screen marks a row whose offer has moved on since it was
installed `(outdated; flayer update refreshes it)`, held as any install's
row is: the screen marks what goes in and out, and the update is what
brings one up to date.

### Following a stale row

`M` on a stale project, one the scan under the screen found moved
([an entry whose project moved](#an-entry-whose-project-moved)), asks and
then runs `flayer relink <old> <new>`, the entry as the marker spells it and
the directory as the scan spelled it from the root; the stale row's pane says
where it went. Only where the scan found it: which directory holds the
project is the scan's to say, by the id it holds, and the screen guessing
from a name could hand what was installed to the wrong one. Where it found
nothing, under the workspace or the folders `flayer minds` was given, the
screen says the line that names the directory by hand. The scan's match and
the stale row are told apart by the directory each names, not by spelling,
since the entry is spelled as the marker has it and the scan's route as
`flayer link` would take it. A key of its own rather than a second answer
to `U`'s question: unlinking drops the entry and leaves what was installed
recorded under a route nothing is at, and the pane says both keys, so the
choice is made seeing where the project went.

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
HTTP (routes, the token, cache headers, a log line), and none of that would
differ between two front ends.

The panel's whole API is four endpoints, which are
[D-18](#d-18-the-panel-writes-by-running-commands) in HTTP:

- `POST /api/query`: a read, answered with JSON of the backend's own shape.
- `POST /api/commands/preview`: the command lines a task is, with nothing run.
- `POST /api/commands`: run a task; the reply is its lines and its outcome.
- `GET /api/transcript`: every task the session ran, with what each produced.

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
4. **A change by cookie** (any method but GET and HEAD, on a request whose
   token came as the cookie) must carry `Sec-Fetch-Site: same-origin` (or a
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
`Server::log` a function, and gets a line per request once it is answered,
like `GET /api/transcript 200`: the method, the path and query, the status.
Never a header, since the cookie and the Bearer token travel in them, and never
a body.
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
every page with a 503 saying `panel not built: run make web/build`, the
target that writes the folder.

Files under `assets/` are named by Vite for their contents, so they are sent
as immutable, to be kept for a year; everything else, `index.html` above all,
is `no-cache`, so a rebuild shows on the next load. A path outside `/api` that
names no file gets `index.html`, because the panel draws its own routes in the
browser, except under `assets/`, where a missing file is one an old page asks
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

## The web panel

`apps/web` is the panel [D-19](#d-19-the-panel-is-built-on-nuxt-ui) chose:
Vue 3.5 and Nuxt UI 4 on Vite 8, in TypeScript 6 checked by vue-tsc, laid out
the way Nuxt UI's dashboard-vue template lays out a dashboard, with a page
per file under `src/pages`. It holds no rules, any more than the server
does: what a valid task is, or what a project has, is core's to say, and the
panel asks. So far it is a frame, a sidebar and a header around an empty
overview; the views come with the snapshot (#72 onward).

### One way out: the transport

Every request the panel makes goes through a `Transport`
(`src/api/transport.ts`): `query` for a read, `send` for a task to preview or
run, and `subscribe` for the events #74 will stream. Views get it from
`useTransport`. The server's fourth endpoint, the transcript, has no call
yet, because nothing shows it: the transcript drawer (#76) adds one, so that
it too reaches the server through the transport, and a desktop shell can
answer it. The HTTP transport (`src/api/http.ts`) is the only file allowed
`fetch`, `EventSource`, `WebSocket` or `XMLHttpRequest`: an ESLint rule
refuses them in every other file, and a test runs the rule.

So authentication lives in one place. The transport uses relative URLs, so
the panel talks to whichever server sent it, on whatever port it took. The
token travels in the cookie the launch link set, which is `HttpOnly`, so the
panel's own scripts never hold it. And every request it posts is JSON and
carries `X-Mindflayer: 1`, a header no form or link can add, as the server
asks of a change by cookie ([What keeps other pages
out](#what-keeps-other-pages-out)). A desktop shell, the reason D-17 asks
for the interface, would answer the same three calls without HTTP, and no
view would change.

What the server answers lives in Vue Query's cache, which knows when an
answer is stale and asks again; a refusal is not asked again, since the
server would only refuse it again. Pinia keeps what only the browser
decides, such as whether the sidebar is collapsed. A store holding a copy of
something the server said would be a second answer that cannot tell when it
has gone wrong.

What the browser decides and a launch should not forget, the colour mode and
the sidebar's size and collapse, is kept in cookies (`src/preferences.ts`)
rather than localStorage. localStorage is kept per origin, and each launch
takes a port of its own, so every one would open on an empty store; a cookie
is kept per host, whatever the port. Neither choice can be handed a storage,
since Nuxt UI reads the colour mode through a bare `useDark()` and the
sidebar through a bare `useStorage`, so the cookies are vueuse's default
storage (its `getDefaultStorage` handler). The browser sends them to
whatever listens on 127.0.0.1, so they hold nothing about anybody's work.

### Nothing the CSP refuses

The server's CSP is `default-src 'self'` ([What keeps other pages
out](#what-keeps-other-pages-out)): nothing from another host, no inline
script or style, no `data:` URLs. The panel is built to need nothing more,
though three of its libraries assume otherwise:

- Geist and Geist Mono come from Fontsource, as files of the build.
- Icons are Lucide's, bundled: Nuxt UI's own and every one a component
  names. Iconify's online build fetches from its API any icon it was not
  handed, so `@iconify/vue` resolves to `src/overrides/iconify.ts`, over
  its offline build, which has no code that could ask. An icon left out of
  the bundle draws as nothing, which the shell test checks for.
- Nuxt UI's colours plugin writes the palette's variables into a `<style>`
  element at run time. The plugin resolves to a stub that does nothing, and
  `main.css` declares the same variables, from the mapping in `src/theme.ts`:
  zinc for the neutrals, and violet as the one accent.
- vueuse adds a `<style>` for every colour mode switch, to stop transitions
  while the classes change, and runs a switch as the page starts.
  `src/color-mode.ts` takes over the `updateHTMLAttrs` handler vueuse lets
  a program replace, and stops them with a class `main.css` holds instead.
  It also makes dark the default: `index.html` carries the class before any
  script runs, and dark is stored when this browser has chosen nothing.
- Vite inlines nothing as a `data:` URL (`assetsInlineLimit: 0`).

A nonce per launch would have let the style elements in, at the price of the
server rewriting the page each time and the panel handing the nonce to each
library that writes one. `tests/dist.test.ts` builds the panel as `make
web/build` does, in a process of its own, and fails on a URL naming another
host anywhere in the page, the scripts or the styles (namespace names, and
the links Vue prints beside a production error, aside), a reference not to
the panel's own files, an inline script or style in the page, a palette
variable missing, or Iconify's API or the injected palette back in the
scripts. The shell test fails on any `<style>` element added while the panel
starts and switches colour mode.

### Out of `make test` until CI has Node

`make web/<target>` reaches the panel's `Makefile` as it reaches any app's,
but `web` is in `NODE_APPS`, not `APPS`, so `make test` and `make build`
leave it out until CI installs Node and pnpm (#71). In the aggregates, they
would fail on every runner for want of pnpm, test-cross's macOS and Windows
included, and say nothing about the code. The server builds without the
panel meanwhile, through `allow_missing`.

Node is 24, the LTS line (`.node-version`), and pnpm is pinned through
`packageManager`; `PNPM` names another way to run it, such as `npx`, where
corepack is not set up. Every target that runs a tool first installs from
the lockfile, frozen, when `node_modules` is older than it, `package.json` or
`pnpm-workspace.yaml`. pnpm holds back any version published less than a
day ago (`pnpm-workspace.yaml`), and runs no dependency's install script:
the two that ask, esbuild's and vue-demi's, have nothing to do here. A test
reads the licence of every package installed, development tools included,
and fails on one a GPL-3.0-only program could not take in, or one that
declares none and is not named with what its source says.

Nuxt UI's plugins declare its components and composables for vue-tsc in
`src/components.d.ts` and `src/auto-imports.d.ts`, committed so a fresh
clone type-checks. They name each file by its path in pnpm's store, whose
folder for `@nuxt/ui` carries a hash of its peers, so a bump of one leaves
every path naming nothing, and each type `any` without a word from vue-tsc.
A test fails on such a path, and `make web/build` writes the files again; no
test run writes them, or it would mend a stale one before the test saw it.

`make web/dev` serves the panel from Vite on 127.0.0.1:5173 and proxies
`/api`, and the launch link's `/?token=`, to `flayer ui --dev --port 7878`
(#68). Through the proxy the token is traded for its cookie where the panel
runs. `changeOrigin`, so the Host the server checks is its own address; the
Origin stays the dev server's, which `--dev` accepts.

## Which workspace a command uses

One function in core decides, `resolve`
([D-08](#d-08-an-explicit-or-activated-workspace-wins)), and every front end
feeds it: each command, and the TUI's screens. In order:

1. the workspace `-W DIR` names, by its root or by its `.mindflayer`;
2. the one `$MINDFLAYER_WORKSPACE` activates, which `flayer activate` sets
   (see [activating a workspace](#activating-a-workspace));
3. the first one found walking up from where the command starts: the
   directory for a `flayer` command, the project's root for a `mind` one;
4. the default one, in home.

It hands back the workspace with the `Reason` that chose it (`Explicit`,
`Activated`, `Above`, `Home`), so a front end that says why it is showing a
workspace asks the answer rather than working it out from paths. `flayer
which` puts it into words beside the root, and the TUI's header names it for
every workspace it shows (`named with -W`, `activated`, `found above`,
`default workspace`). Only for `Home` does the TUI also say there is no
workspace here or above: the directory home is can also be named with `-W`,
activated, or reached walking up from below it. Which workspace a command
changed is another question, about the workspace rather than the rule, and
`load` and `unload` answer it from the path (`FlayerWorkspace::is_default`).
They call the one in home the default one however it was reached, so their
line reads the same on the run that makes it, from anywhere, as on every run
after, which from below home finds it walking up.

The first two are choices somebody made on purpose, so they win over where
somebody happens to stand. Activating a workspace is meant to override
location, as activating a Python virtualenv does; with the walk up first, an
activated workspace would lose, silently, to any workspace above the current
directory. A choice that does not open as a workspace is an error and never a
reason to carry on down the list, which would act on a workspace nobody chose
and say nothing about it. Nor is a chosen directory made into a workspace for
the occasion, a write included: `flayer init` makes workspaces, where it runs
or in the directory it is given.

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

### Activating a workspace

`flayer activate [DIR]` prints shell code that sets `$MINDFLAYER_WORKSPACE`
for the rest of the shell, puts `(flayer:<name>) ` in front of the prompt,
and defines `flayer-deactivate`, which puts both back; it is run as `eval
"$(flayer activate DIR)"`. Printed for the shell to run rather than written
to a file to be sourced: activation lasts as long as the shell, as a
virtualenv's does, and D-08 rejected an activation file as hidden state in
repositories meant to stay minimal. Activating writes nothing at all, so it
does not remember the workspace among [the ones this machine
knows](#the-workspaces-this-machine-knows) either: `flayer init` does. It
opens `DIR` as `-W` opens a workspace (`resolve::named`), by its root or its
`.mindflayer`, and without `DIR` it activates what a read resolves from
where it runs, so `-W` and an earlier activation count, and the default one
only once it is made. What cannot be activated is an error with nothing on
stdout, so `eval` runs nothing. `flayer init PATH` makes the workspace to
activate, wherever it is to go. A relative `DIR` or `PATH` is taken from
where the command works, `-C` included, as `link`'s and `load`'s paths are:
each is one of the command's own words, unlike `-W`, which is taken from
where the command was run, as said above.

The code is core's (`activate::script`): what it sets is what `resolve` is
fed from, and how a path is quoted for a shell is a safety rule, so no front
end that offers to activate a workspace can write it differently. So is the
variable's name (`activate::WORKSPACE_VARIABLE`), by which the CLI reads it,
so the code that sets it and the front end that reads it cannot spell it two
ways; the agents' variables, which nothing here writes, stay the CLI's. The
CLI only decides the shell, from `--shell` or else from `$SHELL`, which it
reads because core never reads the environment. `$SHELL` names the login shell,
which is the running one more often than not; asking the system which
process started this one would be right more often, for much more
machinery, and `--shell` covers the rest.

bash and zsh share one code, since everything it uses means the same in
both. The prefix is put in front of `PS1` and taken out again by its text,
wherever it is by then, rather than by restoring a copy saved on the way in,
as a Python virtualenv does: restoring would also take out a virtualenv's
prefix added since. The virtualenv's own copy, `_OLD_VIRTUAL_PS1`, gets the
prefix and loses it the same way, so the virtualenv's `deactivate` neither
drops the prefix while a workspace is active nor brings it back once none
is. Between them, either deactivate undoes only its own activate, in either
order, which `tests/shells_test.rs` checks beside a real virtualenv, in bash
and in zsh. Each of those runs its shell with no startup file and an
environment holding only a `PATH`, a temporary `HOME` and what the test sets,
so no developer's shell or activation gets in. A shell that is not installed
is skipped, which a passing test does not show; under `$CI` that fails
instead, and the `test (cli)` job installs the zsh its runner lacks. The
function is never called `deactivate`: defining that would take the
virtualenv's away. Activating again runs an existing `flayer-deactivate`
first, so one workspace is swapped for the other, and what the variable held
before the first is what the last puts back.

fish and PowerShell keep the prompt in a function, which the code wraps once
per shell. The wrapper prints the prefix only while a workspace is active,
and deactivating leaves it where it is. Unwrapping, as a virtualenv does,
would break the prompt of whatever wrapped it since: a virtualenv activated
afterwards keeps a copy of the wrapper, which calls the function unwrapping
takes away. Those two are asserted as text, in core, until something runs
them.

The name in the prompt is the marker's, and a marker comes with whatever
directory holds it, a cloned one included. A prompt is expanded every time
it is drawn: bash, and zsh with `PROMPT_SUBST`, run a `$(…)` or a backquote
in it, `\` and `%` are escapes, and `!` is a history number. Each of those,
and every control character, is shown as `_` (`activate::prompt`). Escaping
them instead would take one escaper per shell and per option (`promptvars`,
`PROMPT_SUBST`, `PROMPT_BANG`), none of which the code can see when it is
written. The path is only ever one single-quoted word, in each shell's own
quoting, and one that is not UTF-8 is refused rather than spelled wrong.

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

Made, it puts one thing in the home directory, `.mindflayer`. Its own
artifacts go in its library ([The workspace's own
artifacts](#the-workspaces-own-artifacts)), so no `~/skills` or `~/rules` sits
above every project under home, and the workspace writes no `AGENTS.md` or
`CLAUDE.md` there. What an install into a project at home writes there is
that project's, as for any project
([D-03](#d-03-rules-reach-every-agent-through-agentsmd)).

A read of the unmade workspace answers as a read of one made a moment before
would, because it is the same marker: `flayer list` says it manages no
projects yet, `flayer show` looks for its own in the library the made one
keeps them in, `flayer scan` lists what is under home, all of it unlinked,
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

### The workspaces this machine knows

The default workspace keeps a list of the others on the machine, a `known`
array in its `flayer.toml` beside `projects` and `loaded`, so switching
between several flayers, in the CLI and the TUI now and in the panel later
(#84), has somewhere to read them from. `flayer workspaces` lists it, `flayer
workspaces forget PATH` takes one out, and `w` in the TUI runs `flayer
workspaces` and shows what it prints, as `s` shows `flayer gather list`. What
is known, in which order, and which one is in use are core's (`known::known`,
fed by `resolve`), so the panel's switcher will read the answer the CLI
prints.

In the default workspace's marker rather than in a file of its own or in the
ledger. Home is the one place every flayer on a machine can find; its marker
is already edited one array at a time (`edit_array`), under its lock, with
every comment kept; and the list is configuration a person may read and
prune by hand, like the registry. A file of its own would be one more format
to version and lock, and the ledger would hide the list from that person, and
would have to be made to hold it. The key is not in the template, since only
the default has a use for it and every other marker would carry a line about
nothing: the edit that first makes it gives it a comment saying what it is
(`explained`), and an edit that makes a key and puts nothing in it, as a
refused `forget` would, takes it out again, so the file is left as it was
found.

`flayer init` remembers: each workspace it makes, and one already there,
which it otherwise leaves as it was, so a workspace made before the default,
or by a build from before the list, is one idempotent command from being on
it. conda's `environments.txt` gains an environment the same way, when it is
made and not when it is activated. #56 asked `flayer activate` to remember
too, and it does not. Activating writes nothing ([activating a
workspace](#activating-a-workspace)), which #55 was accepted on and the
README promises; writing there would take the default workspace's lock at
every new shell whose startup file runs `eval "$(flayer activate DIR)"`, wait
behind a gather holding it for up to `lock::WAIT`, and turn a default a newer
build wrote, or a home nobody can write to, into a warning at every prompt.
And nothing is lost by it: the workspace in use is listed whether it is
remembered or not, so an activated one is never missing from the list
somebody is looking at. Whether activating should remember after all is an
[open question](#open-questions).

Never made for the occasion. A default nothing has made yet remembers
nothing and lists nothing, and `flayer init` does not make one to remember
in, the rule every read follows; with `--home ""` there is no default, and
only the workspace in use is listed. The default is never in its own list:
it comes first, for being the default, whatever `known` says. An entry is
stored as `link` stores a project ([how entries are
stored](#how-entries-are-stored)), a route from home when one reaches and an
absolute path otherwise, and matched by where it points. So remembering one
already known by another spelling changes nothing and says the spelling the
file holds, and `forget` takes out every spelling, of a workspace that has
gone too, since that is the one most worth forgetting. One nothing remembers
is refused, as `unlink` refuses what is not linked.

The list is the default, then each entry in the order `known` holds them,
each directory once, then the one in use when it is none of those, said to
be not remembered and given the line that would remember it. The one in use
is marked `*`, as `git branch` marks the branch checked out, and each says
how many projects it links and how many of those are stale. A remembered
workspace that has gone is a row that cannot be opened and a `warning:` that
fails the exit, as a stale project is in every report: shown, never raised.
The stale entries of a workspace that opens are counted rather than warned
of, since they are that workspace's to report. `--json` prints the same list
in the envelope ([JSON, in one envelope](#json-in-one-envelope)), each field
by its name: `name`, `path`, `default`, `remembered`, `in_use`, `projects`,
`stale_projects` and `stale`.

`known` moves no `FLAYER_FORMAT_VERSION`. A build from before it reads past
the key, which says nothing about the workspace holding it, so there is
nothing for it to misread, and keeps it when it edits the marker, since every
edit goes through `toml_edit` ([formats are
versioned](#formats-are-versioned)).

### A project's workspace

A `mind` command resolves from the project's root, not from the directory it
was run in, because a project's workspace is the one above the project: core
moves the start there itself (`resolve::managing` and `resolve::consulted`).
`mind link` and `unlink` register the project with the workspace resolved;
`install` and `uninstall` need that workspace to link the project, and are
refused (`NotManaged`) when it does not (`managing`); `add`, `edit`, `rename`,
`remove` and `templates` consult it, to see to the record of what they change
and to lend templates, and only if it is made (`consulted`): a change to a
project is no reason to write a marker into somebody's home, and a workspace
made just now would hold nothing to see to or lend. One found above that
cannot be read is a `warning:` after the change, since the change is the
project's and the workspace was only to be told; one named or activated that
will not open refuses it, as it refuses every other command, because whoever
chose it meant that one to be told.

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
and `tui::execute` hands that lookup, home and agents' folders and all, to
every line the screen runs, rather than letting each line read the
environment again: a screen opened with `-W` or an activation runs every line
against that workspace. A `-W` among a line's own words wins, and is typed
from the root the lines run from, as a `-C` is.

A project's row runs `mind -C <entry>`, the line somebody in the project would
type, which consults the project's own workspace. Where that is not the
workspace on screen, because the project is linked from outside its tree or
from above a nearer workspace, the row runs `mind -W . -C <entry>` instead
(`Holder::Elsewhere`), so a change sees to the record of the workspace on screen
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
applying runs lines that open the ledger to write (`ledger_to_write`,
through `assign`), which makes it, and never the stand-in, whose records
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

## Agents, and where each kind reaches them

Mindflayer puts what it manages where Claude Code, Codex and OpenCode read
it, and which folder each one reads is recorded in
[docs/agents.md](agents.md): each fact verified on the binary by
`scripts/agent-probe`, or marked docs-only with a fallback. Core turns those
facts into two things in `agent.rs`, so no front end ever spells an agent's
folder.

`Agent` is `Claude`, `Codex` or `OpenCode`, named on a command line and in a
project's settings by the agent's binary: `claude`, `codex`, `opencode`. It is
taken by that exact name, as a shell is, and anything else is
refused with every name there is (`UnknownAgent`), so a settings file never
holds two spellings of one agent. It is a closed enum for the reason `Kind` is:
what an agent reads, and where, is a fact about that agent, so a new one is a
new arm in every match. An agent named at runtime would meet a table with no
entry for it and quietly reach nothing.

`TargetTable` says where each kind goes in a project for each agent, every
location relative to the project's root. `TargetTable::standard` is
[D-07](#d-07-one-copy-per-agent-folder)'s layout, and the facts under it are
the ones docs/agents.md lists for #28:

| Kind | Claude | Codex | OpenCode |
|---|---|---|---|
| skill | `.claude/skills` | `.agents/skills` | `.agents/skills` |
| rule | none yet | none yet | none yet |

- **Claude needs a copy of its own.** It reads `.claude/skills` and not
  `.agents/skills`, verified on Claude Code 2.1.309.
- **Codex reads `.agents/skills`**, docs-only. Its `.codex/skills` is read only
  in trusted projects and `$CODEX_HOME/skills` is deprecated, so neither is
  ever targeted.
- **OpenCode reads both**, docs-only, and is given `.agents/skills` rather than
  leaning on Claude's copy. V1 can switch its `.claude/skills` off
  (`OPENCODE_DISABLE_CLAUDE_CODE`), and a project that targets OpenCode alone
  should not need a Claude folder for it.
- **Nothing at user scope.** `~/.claude/skills`, `~/.codex/skills` and
  `~/.agents/skills` are [D-22](#d-22-user-scope-installs-come-later)'s, later.
  A test pins the table: every location it gives stays inside the project,
  and none is `.codex/skills`.

Where each agent reads is another thing, and core keeps it apart:
`Agent::reads(kind)` is every folder in a project the agent reads a kind
from, whoever put what is there, from the same facts. It can be more than the
table sends: OpenCode reads `.claude/skills` and `.opencode/skills` as well as
`.agents/skills`, and is sent a copy only in the last. A listing asks it which
agents see a skill where it sits ([read where its agents
read](#a-project-marked-by-its-settings-is-read-where-its-agents-read)); an
install asks the table where to write. One table for both was turned down: it
would either send OpenCode a second copy it does not need, or have a listing
say OpenCode misses what sits in Claude's folder. A test holds the two
together: every folder the standard table sends an agent is one it reads.
`.codex/skills`, read only in a trusted project, is not counted, and no folder
of rules is said to be read, though Claude reads `.claude/rules`: what
Mindflayer writes there is listed from the ledger as its rule's target
([installing rules](#installing-rules)), not found by discovery.

Rules have no entry, by design rather than by omission.
[D-03](#d-03-rules-reach-every-agent-through-agentsmd) gives a rule a section
of a block in the project's `AGENTS.md`, with Claude's fallback in
`.claude/rules/mindflayer` chosen by `claude_md::reach` ([how Claude reaches
the block](#how-claude-reaches-the-block)). The block is no folder
a copy goes into, one file every agent shares, so it is rendered outside the
table ([how a rule reaches the agents](#how-a-rule-reaches-the-agents)), and
so are Claude's files, which follow how Claude reaches the block rather than
an agent alone ([installing rules](#installing-rules)). MCP servers, entries
in each agent's own config file, come in M6.

`TargetTable::targets(kind, agents)` gives one target (`AgentTarget`) per
location, each with the agents it is for: all three agents take a skill in two
places, Codex and OpenCode share one copy, and an empty list takes none. An
agent named twice is served once, and an agent the kind does not reach adds
nothing. The targets are sorted by location and their agents in report order,
in core, so two front ends list them alike. Installing a copy into each
starts from this list (`install::targets`).

The table is a value a caller is handed, not a function it reaches for,
because the standard layout is only the default: the owner's amendment to
D-07 lets a project's `.mind/settings.toml` put a kind somewhere else, one
`.skills` folder for every agent for instance ([a project's own
targets](#a-projects-own-targets)). A caller that takes the table follows
the project's without changing, and two agents the override sends to one
folder get one target, as Codex and OpenCode do in the standard layout.
The project hands it on, in core: discovery, and an install, take the
targets from the project (`MindProject::targets`), which gives the standard
table overridden by what its settings name, and no front end calls
`TargetTable::standard`, which would pick a table the project's settings
could contradict. Rejected: a free `targets`
over the standard layout, which every caller would hard-wire and the override
would have to find again; and a folder per agent alone, which cannot say that
a skill and a rule reach Claude in different places.

A target's agents are the ones its copy is for, not proof that they read it:
the standard layout is built from what each agent reads, but a folder an
override names reaches an agent only once that agent is pointed at it, which
a project's status warns of (`ProjectStatus::unreached`). The table's shape
is private, a list of locations per kind and agent: the standard layout
needs one, and `TargetTable::overriding` sends a kind to the list of folders
an override names, for every agent alike, without a caller noticing.

It is pure: nothing is looked for on disk, and locations are joined a folder
at a time, so each has one kind of separator on Windows too. Whether a folder
exists, holds foreign content or is tracked is the install's to find out.

### A project's own targets

A project's settings may name, per kind, the folders what is installed into
it lands in, in place of the ones its agents read: a `[targets]` table
holding `skills = [".skills"]` sends every skill installed into the project
to `.skills`, one copy for every agent it names (the owner's amendment to
[D-07](#d-07-one-copy-per-agent-folder)). `MindProject::targets` overrides
the standard table for each kind the settings name with the folders they
give (`TargetTable::overriding`), so discovery reads there, an install writes
there and records that one target, and a change of agents carries copies to
and from there, none of them knowing an override was read.

- **The standard layout is the default**, and stays so: it is the one place
  each agent finds a skill without being told, by the facts in
  docs/agents.md, so a project made without a thought for it reaches every
  agent it names. An override is for somebody who points the agents
  elsewhere on purpose, and says so in the file.
- **For every agent alike**, rather than a folder per agent: one shared
  folder is what the amendment asks for, and a table per agent written by
  hand would be the standard one again, with every fact in it to keep in
  step with the agents by somebody who is not reading docs/agents.md. Several
  folders are each every agent's, and a folder named twice is one target.
- **Each folder is held to what an install may use** (`misplaced_target`):
  inside the project, as a folder it authors in is, so a copy is that
  project's and no other's; not in `.mind`, which no agent reads and a local
  project keeps out of git; not in a `.mindflayer`, which no agent reads
  either, and whose shelf and cache gathering replaces by names of its own,
  which a workspace at the project's root keeps there; not in a `.git`,
  which git owns, where a skill named `logs` would be written among its own
  files, in every clone of a shared project; and only for a kind
  installed as a folder an artifact, so a rule, which reaches agents through
  `AGENTS.md` ([D-03](#d-03-rules-reach-every-agent-through-agentsmd)), has
  none: its block and Claude's files are rendered outside the table. What
  an install refuses of any target, an agent's own folder and one a loaded
  workspace keeps as its own, it refuses of these too, since only it knows
  the homes and the loads.
- **Strict when writing, lenient when reading.** A change that names a folder
  no install may use is refused before anything is written, and so is a kind
  named with no folder at all, which would install it nowhere: an
  uninstall's to do, by name. A folder written into the file by hand is read,
  passed over and said (`MindProject::misplaced_targets`, `MisplacedTarget`):
  a warning wherever the project is reported or installed into, and no copy
  there, rather than a reason to refuse everything else the settings say. A
  kind every folder of which is passed over goes nowhere, rather than back to
  the standard folders the settings said not to use, and an install into it
  is refused (`InstallError::NoUsableTarget`).
- **Read by no agent until one is pointed there.** Whether an agent reads a
  folder is told by where it reads (`Agent::reads`), matched on disk, so a
  `.claude/skills` linked to `.skills` is Claude reading it.
  `ProjectStatus::unreached` names, for each kind, the agents that read none
  of its targets; `status` warns of them, with a non-zero exit, since what is
  installed there reaches those agents no more than if it had gone, and
  `targets set` notes it when it is set (`Plan::unreached`). An agent pointed
  there by a setting of its own is beyond what docs/agents.md says it reads,
  and is still warned of ([an open question](#open-questions)).
- **Changed through `reapply`**, as the agents are: `mind targets set <kind>
  <dir>...` and `mind targets reset <kind>`, and `flayer targets -p`, a dry
  run without `--yes` ([when a project's agents, targets or share mode
  change](#when-a-projects-agents-targets-or-share-mode-change)).
- **With a version a build from before them refuses** (`TARGETS_VERSION`).
  A key the format does not define is otherwise read past, and a build from
  before this one would read past `targets` too and install into the
  standard folders without a word, recording copies the newer build does not
  read. A `settings.json` that set them was written at version 4, which such
  a build refuses, and one that did not stayed at 3, which every build since
  settings reads: the version such a file was written with was the lowest
  that read it as it meant, and a change of `targets` brought it along,
  back down once no kind was named. Bumping every file to 4 was turned down:
  a project that never set an override would have been refused by every
  older build for nothing. A `settings.toml` is 5 whatever it sets, since
  every build that reads one reads `targets`, and one from before it finds no
  project there at all, so setting or resetting them moves no version. A
  version 3 `settings.json` given `targets` by hand is read with them all the
  same: somebody wrote them to be used, and only an older build could miss
  them ([an open question](#open-questions)). One whose `targets` do not
  read as them is read past, as before them, so that no project an older
  build opened stops opening.

### Adding an agent

Record what it reads first, in docs/agents.md, verified or docs-only with a
fallback. Then a variant on `Agent` and its name, and its arm for each kind in
the table's `standard`, and in `read`, behind `Agent::reads`, which both match
every pair with no wildcard: neither a new agent nor a new kind compiles until
somebody has said where it goes and where it reads. The
compiler lists every match on `Agent`, since each is exhaustive, but not what
is not a match:

- **`Agent::ALL`**, an array. A variant left out of it still compiles, parses
  as no agent, and gets no entry in the standard table, which is built from
  it, whatever its arm in `standard` says.
- **Its home, if it keeps a folder of its own for every project.** A field on
  `AgentHomes` and its folder in `AgentHomes::defaults_under`, its option in
  `AgentHomeOptions` and its variable in `homes::agent_homes`, and both in the
  tests' `common::PINNED` and `common::CLEARED`, or a test could read the
  developer's real folder. AGENTS.md's "So do the agent homes" and
  [the agents' own folders](#the-agents-own-folders) list them too.

## The agents' own folders

Which folders each agent reads, for skills and for rules, at project and user
scope, is recorded in [docs/agents.md](agents.md), each fact either verified
on the binary by `scripts/agent-probe` or marked docs-only with a fallback.
A change to an agent path cites it; this section is about how Mindflayer
finds the agents' own folders, not what the agents do in them.

Checks to come read what an agent reads for every project, not only what a
project holds: Claude's user memory, which `claude_md` leaves out of what
keeps Claude from reading `AGENTS.md` (#44), Codex's config, OpenCode's
configuration folder and the skills in `~/.agents` (#91, #96). Those folders
come in the way home does. `--claude-config-dir`, `--codex-home`,
`--opencode-config-dir` and `--agents-dir` are global options on both parsers,
and `homes::agent_homes` takes each from its option, then from the variable
the agent itself reads (`$CLAUDE_CONFIG_DIR`, `$CODEX_HOME`,
`$OPENCODE_CONFIG_DIR`), then from its folder in the user's home (`~/.claude`,
`~/.codex`, `~/.config/opencode`, `~/.agents`). `~/.agents` has no variable:
it belongs to no one agent, so none of their variables moves it. Core takes
the answer as an `AgentHomes` and never resolves one.

For OpenCode that order is a simplification: its documentation reads
`$OPENCODE_CONFIG_DIR` as a folder loaded on top of `~/.config/opencode`, not
one that replaces it. Nothing reads either yet, and #17 ran only Claude Code,
so that reading is still docs-only and the simpler shape stands until a check
on the binary confirms it; the [open questions](#open-questions) say what
changes if one does.

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

Installing is the first to take them, to refuse a copy inside one ([never
into an agent's own folder](#never-into-an-agents-own-folder)), and
rendering a project's rules takes them to refuse a block of them there ([how
a rule reaches the agents](#how-a-rule-reaches-the-agents)). They are
decided once per command line, with the workspace to use, into the same
`Lookup` (`Lookup::given`), which the TUI keeps and hands to every line it
runs (`tui::execute`), its install screen's among them: a line the TUI runs
carries none of the four options, so deciding them again would read the
environment the TUI was opened in, and a test that drives the TUI would reach
the real folders. Nothing reads what is in them yet.

## How a rule reaches the agents

A rule reaches a project's agents as a section of a block in the project's
root `AGENTS.md` ([D-03](#d-03-rules-reach-every-agent-through-agentsmd)):
the one file of a project that Claude Code, Codex and OpenCode all read, and
Codex has no way to include another ([docs/agents.md](agents.md)).
`agents_md::render` writes it, and `claude_rules::render` the rules Claude
alone is given ([Claude's rule files](#claudes-rule-files)). Which rules a
project is given, when, and the record of what was written are
`install::rules`' ([installing rules](#installing-rules)); whether Claude
reads the file at all, or needs an `@AGENTS.md` import in its `CLAUDE.md`,
is `claude_md`'s ([how Claude reaches the
block](#how-claude-reaches-the-block)).

```markdown
# Working here

Run `make test` before every push.

<!-- mindflayer:begin -->
<!-- mindflayer:rule commit-style -->
Write commit messages as `<area>: <imperative summary>`.

<!-- mindflayer:rule git/no-force-push -->
Never force-push a branch somebody else may have pulled.
<!-- mindflayer:end -->
```

- **HTML comments for markers.** Claude strips a block-level comment before
  the file reaches the model and keeps the text between two of them,
  verified on 2.1.309 with the file read natively and imported. Codex and
  OpenCode read the file as it is, where a comment is a line of markup that
  says nothing. A heading of Mindflayer's own was turned down: every agent
  would read it as one more instruction.
- **One section a rule, in listing order.** A section is the rule's marker,
  naming its route, then its body: `Artifact::contents`, the file less its
  front matter, without the blank lines it opens and closes with, so the
  block is laid out the same however each file spaces its fences or ends.
  One blank line separates two sections, which keeps two rules two
  paragraphs once Claude has stripped the markers. They come in the order
  `list` gives them (`catalog::report_order`), by name. A rule that states
  `paths` has no section: it goes to `.claude/rules/mindflayer` alone
  ([Claude's rule files](#claudes-rule-files)), where Claude loads it only
  once one of those files is read, while in the block every agent would read
  it for every file. A `paths` in a shape nothing reads says nothing, as
  everywhere else, so that rule has its section. Both renderers ask
  `RuleManifest::is_scoped`, so they never disagree on which rules those
  are.
- **Strict when writing.** A route is held to the rules every name is
  (`name_issues`), which also keeps its marker one that reads back as
  itself: lowercase words joined by single hyphens and slashes hold no space
  and no `--`. A body line that would read back as a marker is refused, and
  so are two rules of one name, a rule that cannot be read and anything that
  is not a rule. Each refuses the whole render, with nothing written: a
  section left out would take its rule away from every agent until the next
  render.
- **Every byte outside the block is kept.** The file is read as lines that
  keep their endings (`lines`, shared with git's exclude file), and only what
  lies between the markers is replaced, whole, a line somebody wrote there
  included. A new block goes after what the file holds, on lines of its own,
  with the ending the file already uses, so a CRLF file stays CRLF; a last
  line without a newline gets one, and the end marker goes without. An empty
  line sets it apart from what is above, so that once Claude strips the
  markers the file's last paragraph and the first rule's are still two, the
  layout the probe verified ([docs/agents.md](agents.md)). That line is the
  block's: an empty line just above the begin marker is taken out with it,
  one somebody wrote there by hand included, since nothing tells the two
  apart. So dropping the block gives the file back exactly as it was before
  the block was written, and a render that changes nothing writes nothing.
- **Refused, never repaired.** A block left open, an end marker closing
  nothing, a second block, a rule's marker outside the block, one route
  twice, and a line written like a marker but none of them (indented, spaced
  otherwise, naming a mark there is not) are each refused with the line they
  are on, and nothing is written (`agents_md::Malformed`), as in the exclude
  file. Read as text, a hand-indented begin marker would have the next render
  write a second block below the first. The file is read a line at a time,
  not as markdown, so a marker quoted in somebody's code fence counts:
  telling fences apart would need a markdown parser, for a mistake that is
  refused and never acted on.
- **Made, changed, taken away.** A file that is not there is made with
  `create_new`, and only then is the render said to have made it
  (`Written::Created`); the ledger keeps that (`created_files`) and the
  next render is handed it back as `created`, so that once no rule is left
  the block goes, and the file with it when nothing but blank lines is
  left. Mindflayer takes away only a file it
  made, and a link somebody put in its place is emptied, never removed. A
  file that is there is replaced through `replace_bytes`, a temporary and a
  rename, which follows a link to the file it names: an `AGENTS.md` linked
  to the `CLAUDE.md` beside it, as some teams keep the two, is written
  through, and Claude then reads the block as that `CLAUDE.md`, with no
  import beside it (`Reach::Same`, [how Claude reaches the
  block](#how-claude-reaches-the-block)). A link leading out of the project is refused
  (`AgentsMdError::LinkedOut`), since a repository can hold one to the
  user's own `~/.codex/AGENTS.md`, and replacing it with a file of the
  project's own would undo what somebody linked on purpose. It takes no
  lock, as `ConfigFile` takes none: its caller holds the workspace's.
- **Never above other projects.** Claude reads every `AGENTS.md` from where
  it starts up toward `/`, and OpenCode V2 up to the home directory, so rules
  rendered into a folder above a project the workspace links would reach
  that project too, beside its own: a monorepo's root, above its packages.
  A block is refused in such a folder, a stale entry's included, and in the
  workspace's own root unless a project it links is there. The workspace
  puts nothing in its own directory (`FlayerWorkspace::own` is a holder at
  that root, not a project), while a project at the root is a project like
  any other, and its `AGENTS.md` its own. A render that leaves no block goes
  ahead in either: a block written before a project was linked below it, or
  before the one at the root was unlinked, is reaching those agents already,
  and refusing to take it out would keep the leak the refusal is there to
  stop.
- **Never into an agent's own folder.** A block is refused, the same way,
  in a project at or inside one of the agents' own folders
  (`AgentsMdError::UserScope`), which the front end hands over as it does
  for an install ([never into an agent's own
  folder](#never-into-an-agents-own-folder)): a project at `$CODEX_HOME`
  would write the `AGENTS.md` Codex reads in every project, one at
  `~/.config/opencode` OpenCode's, and one at `~/.claude` the
  `.claude/AGENTS.md` Claude finds above every project in the user's home.
- **What Codex would miss is said.** Codex reads one file a folder,
  `AGENTS.override.md` before `AGENTS.md`, from the git root down to where it
  starts, and cuts the whole off at 32 KiB (`project_doc_max_bytes`); with no
  git root it reads only the folder it starts in (docs-only, each with its
  fallback in docs/agents.md). So a render that leaves a block warns
  (`agents_md::Warning`) of an override beside the file, of a project in no
  git work tree, and of the files from the git root down coming within an
  eighth of the budget or past it, each worked out for Codex started at the
  project's root. Warnings, not refusals: the block still reaches Claude and
  OpenCode, and what to change is in files that are not Mindflayer's. A
  repository or a file that cannot be looked at is said the same way, rather
  than turning a render already written into a failure.

### Claude's rule files

Only Claude reads a project's `.claude/rules`, at any depth, and only Claude
honours a rule's `paths`, loading such a rule once it reads a file they
match (verified on 2.1.309, [docs/agents.md](agents.md)). So a rule about
some files only reaches its project as a file there, which
`claude_rules::render` writes, and has no section in the block. The same
renderer gives Claude the rules about every file when it reads no block.

```text
.claude/rules/mindflayer/rust/errors.md          a rule that states paths
.claude/rules/mindflayer/git/no-force-push.md    one that does not, in fallback mode
```

- **Two modes, chosen by the caller.** `Mode::Scoped` writes only the rules
  that state `paths`, for a project whose Claude reads the block, natively or
  through the import in its `CLAUDE.md`. `Mode::Fallback` writes every rule,
  for one whose Claude reads no block. Which applies is `Reach::mode`, from
  how Claude reaches the block ([how Claude reaches the
  block](#how-claude-reaches-the-block)), never from whether a block was
  written alone: in fallback mode the block may still be written for Codex
  and OpenCode, and Claude, reading none of it, reads each rule once and
  misses none. A file left from the other mode goes
  once its rule is no longer taken, unless somebody changed it since: that
  one stays, and is said, like any edited file.
- **The rule as written.** A file holds the rule's bytes, front matter
  included, where the block holds `Artifact::contents`: Claude takes `paths`
  from it and strips the rest before the rule reaches the model (docs-only),
  and a file that is the rule byte for byte is one somebody can open and
  recognise. A `paths` in a shape nothing reads makes a rule about every
  file here as in the block, and its front matter is still kept as written.
- **A folder of Mindflayer's own.** `mindflayer` keeps the files apart from
  rules somebody keeps in `.claude/rules` by hand, so no route names one of
  theirs. Below it a file is filed by its route, folders and all, as in the
  library; a route is held to the name rules (`name_issues`), so it never
  climbs out, and neither does a recorded one.
- **What is Mindflayer's is what the caller recorded.** The renderer is
  handed what the ledger records of each file ([installing
  rules](#installing-rules)), as an earlier render returned it
  (`Rendered::recorded`): each file's route, and the hash of what it held
  (`hash::bytes`, the SHA-256 of the bytes, beside `hash::tree` for a
  folder). A file is Mindflayer's while it holds exactly that: replaced when
  its rule changes, taken away when its rule is no longer asked for.
  Anything else at a rule's place is somebody's, never written over or taken
  away. A file nothing recorded is `Written::Foreign`, and its rule is not
  written, even when it holds the rule byte for byte, since taking it for
  Mindflayer's would remove it with the rule. One changed since it was
  written is `Written::Edited`, left as it is with its record kept, as
  [D-04](#d-04-an-edited-copy-is-detected-not-disowned) has an edited copy;
  edited back into the rule as it is now, it is current again. Only a forced
  install or uninstall of its own rule replaces or takes it away, handing
  the hash of what it holds now as the one recorded. A link at a rule's
  place is somebody's too, since Mindflayer writes none. Rejected: telling
  them by route alone, which would write over a teammate's change committed
  into a shared project, as the block once replaced a hand-edited section;
  by the folder, which would take somebody's file there for Mindflayer's;
  and by a mark in the file, which would no longer be the rule as written,
  and which a copy made by hand carries along.
- **Made, replaced, taken away.** A file that is not there is made with
  `create_new`, so one somebody made after the look is never written over,
  and only then is it said to be made (`Written::Created`). One that is
  Mindflayer's is replaced through a temporary beside it and a rename, never
  through a link (`replace_bytes_unresolved`): the temporary's name ends in
  `.tmp`, which Claude's `**/*.md` does not load, so it is no rule staged
  where Claude looks. One that already holds the rule is not written at all.
  Every file is written before any is taken away, so a renamed rule is never
  missing from the folder between its old file and its new. Taking away the
  last file takes the folders it leaves empty, up to and including
  `mindflayer`, and not `.claude/rules` or `.claude`, which may be
  somebody's even when empty; a recorded file found gone (`Written::Gone`)
  leaves its folders to go the same way.
- **Refused whole, or failed file by file.** A rule that is not a rule, one
  that cannot be read or is named no name, two rules of one name and a
  recorded route that is no name refuse the whole render before anything is
  written, as the block's render refuses them. Two of one name are refused
  among every rule handed in, whichever the mode takes: in scoped mode the
  block compares only the rules about every file, so a pair of which one
  states `paths` is this render's to refuse. A file that cannot be looked
  at, written or taken away is only its rule's failure
  (`claude_rules::Warning::Failed`): each file stands on its own, and the
  caller has to hear of every file that was written, to record it.
- **Kept out of git in a local project.** One exclude entry a file, in the
  project's group keyed by its id ([what keeps an install out of
  git](#what-keeps-an-install-out-of-git)), all written before any file
  lands; a block that cannot be edited refuses the render with nothing
  written. The entries of files taken away, found gone, or that failed to
  land come out last, and a block that cannot be edited then is said
  (`Rendered::still_excluded`), not raised. Somebody's file at a rule's
  place gets no entry, and keeps whatever entry it had. An entry names the
  route through `.claude`, not the folder on disk, and git follows no link,
  so a file reached through a link inside the project is not kept out
  ([open question](#open-questions)).
- **Never where the block is refused.** A render asked for a rule is refused
  wherever `agents_md::misplaced` refuses the block: above a project the
  workspace links, at the workspace's root where it links none, and inside
  an agent's own folder. For a project at the user's home that folder is
  `~/.claude` itself, whose `rules` Claude reads in every project. A render
  that only takes files away goes ahead, so rules left there can come out.
  Whether Claude reads a `.claude/rules` in a folder above where it starts is
  not recorded, so above a project is refused as for `AGENTS.md`, which at
  worst is too cautious (an [open question](#open-questions)).
- **Never out through a link.** A link on the way to a rule's file that
  leads out of the project is refused, writing or taking away
  (`ClaudeRulesError::LinkedOut`): `.claude` linked to the user's own
  `~/.claude` would give every project these rules, and a removal through
  it would take a file out of somebody else's folder. A link that stays
  inside the project is followed, though in a local project the file's
  exclude entry then misses it (above).
- **What misses a rule is said.** A rule that states `paths` reaches Claude
  alone, in either mode, and `Warning::ClaudeOnly` names the project's
  agents that read no `.claude/rules`: Codex and OpenCode, unless the
  project names fewer, and both for a project a `mind.toml` marks, which
  names none (docs-only for both agents; were either to read the folder,
  the warning would be too cautious, never wrong about Claude). Which agents
  read the folder is a match with an arm an agent and no wildcard, so a new
  agent says whether it does before it compiles. `Agent::reads` still names
  no folder of rules, so discovery reads none of these files, and a listing
  of the project takes none for somebody's: status lists each as a target
  of its rule, from the ledger ([installing rules](#installing-rules)).
- **No lock of its own.** As `agents_md::render`, it takes none: its caller
  holds the workspace's.

### How Claude reaches the block

Claude Code reads `AGENTS.md` natively only when no `CLAUDE.md`,
`.claude/CLAUDE.md` or `CLAUDE.local.md` is in the folder it starts in or
any folder above it (verified on 2.1.309, [docs/agents.md](agents.md)). So
the block reaches Codex and OpenCode wherever it is, and Claude only as
`claude_md` decides: natively, through an `@AGENTS.md` import Mindflayer
adds to the project's own `CLAUDE.md`, or not at all, when Claude's rule
files carry every rule instead ([Claude's rule
files](#claudes-rule-files)).

```markdown
<!-- mindflayer:begin -->
@AGENTS.md
<!-- mindflayer:end -->

# Working here
```

- **One predicate.** `claude_md::reads_agents_md` looks for the three names
  in the project's root and every folder above it, past the git root, since
  a `CLAUDE.md` in the folder somebody keeps their repositories in stops the
  read too (verified). It stops below `/`, as Claude's own check does by the
  directory counts in its debug log: counting `/` would choose the fallback
  where Claude reads the block natively, and give Claude every rule about
  every file twice (D-03 records the choice). The user memory file,
  `CLAUDE.md` in the Claude folder the front end hands over in
  `AgentHomes`, never counts, also where the walk from a project under the
  home finds it as the home's `.claude/CLAUDE.md` (verified); it is told by
  the file it is, both paths resolved, so a spelling through a link is still
  that file. The walk starts from the project's root resolved, as Claude,
  started there, has its directory from the system. Anything at one of the
  names counts, a link followed, a link to nothing not.
- **Five reaches.** `claude_md::reach` takes the predicate, the project's
  settings and whether the block was written, and says how Claude gets the
  block. `Reach::Native` when nothing is in the way. `Reach::Same` when one
  of Claude's own names in the project is the `AGENTS.md`, one file through
  a link, as some teams keep the two: Claude reads the block as that file,
  and an import there would bring the file into itself, between markers the
  block already holds. `Reach::Imported` when one of the project's own
  files already imports `AGENTS.md` with a line somebody wrote, as Claude's
  docs show a `CLAUDE.md` sharing it. `Reach::Import` when the project's
  own `CLAUDE.md` or `.claude/CLAUDE.md` may take Mindflayer's import, or
  already holds it. `Reach::Fallback`, with its
  reason, otherwise: no block written, only a `CLAUDE.local.md`, a
  `CLAUDE.md` only above the project, the project's own one tracked by git
  in a local project without `edit_tracked`, one reached through a link, or
  one of which git cannot say whether it tracks it. `Same` is the case #42
  left to this predicate: the block is written through such a link, and an
  import in the same file would have each render take the other's block
  for its own.
- **Never read twice.** `Reach::mode` is the mode Claude's rule files are
  rendered in: `Mode::Scoped` for `Native`, `Same`, `Imported` and `Import`,
  where Claude reads the block once, natively, as its own file, or through
  an import, which loads a file once and turns the native read off by being
  in a `CLAUDE.md` (verified); `Mode::Fallback` for every reason Claude
  reads no block, where its rules folder holds every rule and the block none
  of them for Claude. No block written is a fallback too, whatever the
  predicate says: Claude cannot read a block that is not there, and the
  rules folder then holds the rules about every file that a refused or
  skipped block would have. So Claude reads a rule in exactly one place.
  An import somebody wrote counts as Mindflayer's does, in a file Mindflayer
  may not edit too: a team's tracked `CLAUDE.md` holding `@AGENTS.md`, in a
  local project, would otherwise give the fallback, and Claude would read
  every rule about every file in the block through that line and again in
  its rules folder. So does one in a `CLAUDE.local.md`, when no file of the
  project's own can take Mindflayer's, since Claude reads that file as it
  reads `CLAUDE.md`. Such an import is a line of its own, `@AGENTS.md`, or
  `@../AGENTS.md` in `.claude/CLAUDE.md`, the line Mindflayer would write,
  outside a fenced code block, whose imports Claude skips (docs-only), and
  outside an HTML comment over several lines, which Claude strips. Looking
  no further is on purpose: a line taken for an import Claude does not
  follow would keep those rules from Claude altogether, while one missed
  has Claude read them twice at worst. An `@AGENTS.md` in the middle of a
  sentence, which Claude follows too, is one missed.
- **Which file takes the import.** `CLAUDE.md` first, the file the probe
  verified the import in, as `@AGENTS.md`; then `.claude/CLAUDE.md`, as
  `@../AGENTS.md`, since Claude resolves an import from the file holding it
  (docs-only, with its fallback in docs/agents.md). Never
  `CLAUDE.local.md`, a person's own file, kept out of the team's view. A
  file may take it in a shared project, in one a `mind.toml` marks, which
  was committed by design, and in a local one while git does not track the
  file (`exclude::tracks_file`, from the index with gix) or its settings say
  `edit_tracked` (D-02). A file already holding the import keeps it, so it
  never moves from one file to the other, and counts as how Claude reaches
  the block even once it may no longer be edited, a tracked `CLAUDE.md`
  committed with the import in it say, since Claude reads it there anyway:
  taking the fallback beside it would have Claude read every rule twice.
  An import somebody wrote in either file comes before Mindflayer's, which
  is then wanted nowhere and taken out where it may be, so nothing is
  written where a team already shares `AGENTS.md` with Claude. One in a
  `CLAUDE.local.md` comes after every file of the project's own that may
  take Mindflayer's: it reaches only its owner, and a teammate cloning a
  shared project needs the import in the file they share.
- **A block at the top.** The import sits between the markers the block of
  `AGENTS.md` has, read by the same search (`agents_md::locate`) through
  the same lines (`lines`), at the top of the file, an empty line below it,
  the layout the probe verified. The empty line is the block's, taken out
  with it, so the file comes back as it was. Everything outside the block
  is kept byte for byte, CRLF and a missing final newline included, and a
  byte order mark stays first, where the block would otherwise push it into
  the middle of the file. A malformed block is refused with nothing
  written, as in `AGENTS.md`, and so is a rule's marker in the block: it
  marks a copy of `AGENTS.md`, and an import beside its rules would give
  them twice.
- **What is never touched.** `claude_md::render` edits only the project's
  own `CLAUDE.md` and `.claude/CLAUDE.md`, each only while it is there. It
  never makes one, as it never makes a `CLAUDE.local.md`: a `CLAUDE.md`
  Mindflayer made would itself turn Claude's native read off. It never edits
  a file above the project, which is reported as the fallback's reason
  (`Fallback::Above`) instead: Claude reads it in every project below, and
  it is nobody's to edit for one of them. Nor a file reached through a
  link, whose import would mean whatever the link leads to; nor the user
  memory file in a project at the home; nor the `AGENTS.md` itself.
- **Added once, gone with the last rule.** The import goes into the file
  the reach names, and comes out of the project's other `CLAUDE.md`, so it
  is in one file at most, however many rules there are; once no block is
  written it comes out of both, and each file is as it was before.
  Replaced through a temporary and a rename, never through a link
  (`replace_bytes_unresolved`), and a file whose text a change leaves as it
  was is not written. Every file is read before any is written, so a
  malformed block refuses the whole render. An import no longer wanted in a
  file that may no longer be edited stays there, and is said
  (`claude_md::Warning`), never taken out behind the setting that keeps
  Mindflayer out of the file ([an open question](#open-questions)).
- **No lock of its own.** As the two renderers, it takes none: its caller
  holds the workspace's. Which rules a project is given and when to render
  are `install::rules`', as for the block.

### Installing rules

A rule is installed, and taken out, through the same `install::install`
and `install::uninstall` a skill is (`install::rules`), offered from the
workspace's own rules and from a load's, by the route it is filed under:
held to the rules every name is, as the block holds it (`name_issues`),
rather than to one folder name, and two places offering one route are
refused as for a skill (`OfferedTwice`), `--from` saying which. A rule is
never gathered (#111), so the shelf offers none. The command line, the
install screen and what a load is said to offer take rules as they take
skills ([which kind a name is](#which-kind-a-name-is)).

- **One render for every rule.** The block is one file, written whole, so
  a project marked by its settings is given its rules together: installing
  one, or taking one out, renders every rule the ledger records for the
  project, each from the offer it was installed from, as it is there now,
  into the block ([how a rule reaches the
  agents](#how-a-rule-reaches-the-agents)), then the import or none in its
  `CLAUDE.md` ([how Claude reaches the block](#how-claude-reaches-the-block)),
  then Claude's rule files in the mode that reach says ([Claude's rule
  files](#claudes-rule-files)). `install::rules::render` does the same with
  nothing installed or taken out, for a front end to bring the three in
  line with a project as it is now. Rendering only the rule asked for was
  turned down: the block has no part of its own to write, and leaving the
  other sections as they stand would need each read back from the file,
  which is what an edit there changes. A section somebody edited is the one
  read back and kept (below).
- **One installation a rule, a hash a target.** Its targets are its
  section, recorded by the file it is in, `AGENTS.md`, and its file for
  Claude, each with the hash of what it holds (schema 4, [why the ledger is
  SQLite](#why-the-ledger-is-sqlite)): the section's is `Section::hash`, of
  its lines whatever ending the file uses, and the file's `hash::bytes`. A
  rule given neither, one about some files in a project that does not name
  Claude, is recorded with none, and said to reach no agent
  (`Warning::Unreached`). The installation's own hash is of the rule as it
  was rendered, what its standing compares the offer with: outdated once
  the two differ ([an edited copy is left
  alone](#an-edited-copy-is-left-alone)).
- **`AGENTS.md` goes only if Mindflayer made it.** Whether the render made
  the file is noted in the ledger (`created_files`) and handed back to the
  next render, so the last rule out takes away a file Mindflayer made and
  leaves one somebody wrote, as it was before the block. A file Mindflayer
  made that somebody then wrote in is theirs once no block is left in it,
  and so is one git tracks: somebody committed it.
- **Local mode, and a file the team owns.** In a local project
  ([D-02](#d-02-installs-stay-local-unless-a-project-shares-them)) an
  `AGENTS.md` Mindflayer makes is kept out of git before it is made, an
  entry in the project's group of the exclude block, taken out once the
  file is no longer Mindflayer's; one somebody had is theirs to commit, and
  gets none. One git tracks is written only in a shared project or with
  `edit_tracked`. Otherwise no block is written there: Claude has every
  rule from its rule files, since the reach of no block is the fallback,
  and the agents that read rules only in the block, Codex and OpenCode
  among those the project names, are said to read none of them
  (`Warning::Withheld`), while the project has a rule the block would have
  held: with none left, or only rules about some files, which reach Claude
  alone anyway, nothing is missed. A block written there before, by a
  project shared then, is left as it is, and said (`Warning::Left`), as an
  import left in a tracked `CLAUDE.md` is. A rule whose section it holds is
  not uninstalled meanwhile (`InstallError::Withheld`): its section would
  stay, with no record, and the next render that may edit the file would
  take this workspace's own section for somebody's. Sharing the project,
  setting `edit_tracked`, or taking the section out by hand lets it go.
- **Somebody's section is never written over.** A section in the block
  that the ledger does not record was written by another workspace that
  links the project, or by a hand that copied a marker, and rendering the
  block whole would take it out. So it makes its rule `Standing::Foreign`,
  as a file at a rule's place in Claude's folder that nothing records does,
  and refuses every render of the project's rules until it is gone
  (`InstallError::NotRecorded`). Taking it for this workspace's was turned
  down: two workspaces, each with its own ledger, would take turns taking
  each other's rules out.
- **An edited section is kept.** A section this workspace rendered whose
  hash is not the one recorded was edited since, and a render keeps it as
  the block holds it (`AgentsMd::section`), with its record, so it reads as
  edited still, and says so (`Warning::Kept`), as Claude's renderer leaves an
  edited file. What the rule was rendered from stays recorded too, so once
  the edit is undone it reads as outdated rather than current. Installing or
  uninstalling that rule is refused, as for any edited copy, unless forced:
  then its section is rendered from its offer, or taken out, and its file
  for Claude is written over or taken away. A section that cannot be read
  back, a line in it that is not UTF-8, refuses the render
  (`InstallError::Unread`) rather than go with it. Replacing the whole block,
  as every render did before #47, lost the edit the next time any rule of
  the project was installed.
- **A rule no longer offered refuses the render.** A rule whose origin
  offers it no more (deleted from the library, a load gone) cannot be
  rendered, and leaving its section out would take it from every agent,
  which is what the block refuses a rule it cannot read for. So the render
  is refused (`NoLongerOffered`) until it is put back, or uninstalled,
  which renders the rest without it. An uninstall of another rule goes
  ahead past it, keeping it as the project holds it: its section as the
  block holds it (`AgentsMd::section`), an edit included, its file for
  Claude where it is, and its record as it was. Taking one rule out is no
  reason to take another, and refusing would leave two rules no longer
  offered each keeping the other in, with nothing else that drops a rule's
  record. An update goes past it the same way (`rules::update_held`),
  saying it is orphaned rather than refusing to bring the rest up to date
  ([bringing what was installed up to
  date](#bringing-what-was-installed-up-to-date)).
- **Refused before anything is written.** What can refuse is asked first:
  a block that cannot be read or holds somebody's section, a place no rule
  is written (`agents_md::misplaced`), an `AGENTS.md` linked out of the
  project, a `CLAUDE.md` whose block cannot be read (`claude_md::reach`), a
  rule linked out of the workspace offering it (`InstallError::Outside`).
  The block is written before the import and the files; one of those that
  fails after it records what was written, then is raised, and the next
  render finishes it.
- **Recorded before it is written.** Before the first write the ledger
  holds the rule being installed, and, for each rule's file for Claude that
  nothing is at, the hash of what the render would put there (and whether
  the render will make `AGENTS.md`). A render killed part way then leaves
  files the next one knows for Mindflayer's, rather than taking each for
  somebody's and leaving it where Claude reads it. A record of a file that
  was not made says nothing wrong: the next render makes it, or forgets it.
  A file replaced and not yet recorded reads as edited, and is left, until
  a render finds it holding its rule as written again, which makes it
  current. An uninstall keeps its record until the render is done, so one
  killed part way is run again.
- **A project a `mind.toml` marks gets a copy.** It names no agents and
  has no block, so a rule is copied into the folder its marker names, the
  file as written, filed by its route with its folders made, and recorded
  as a skill's copy is, its target with the same hash as the installation.
  An unrecorded file there is somebody's, and refuses the install, as for a
  skill; an uninstall removes the file its record names, and the folders
  its route was filed in that this leaves empty, as removing a rule does.
  Moved to its settings, the project keeps that copy and its record until
  the next render of its rules, an uninstall's included, which takes the
  copy out of the folder the project authors in while it holds what was
  copied, as a change of agents takes out an untouched copy, and leaves one
  changed since where it is, its author's, its record gone and said
  (`Warning::Edited`). One that cannot be taken out stays recorded for the
  render after (`Warning::Unremoved`).
- **Held by the workspace's lock**, from before the records are read until
  the last is written, as an install of a skill holds it; none of the three
  renderers takes it.
- **A change says what it found, once.** What the three renderers and the
  render as a whole have to warn of is gathered in core
  (`Rendered::warnings_of`), so no front end says one renderer's and forgets
  another's. Not what holds of another rule than the one installed or taken
  out whatever the render, that it reaches Claude alone or no agent: that
  was said when the rule went in, `status` says it whenever asked, and saying
  it on every render made installing one rule warn, and fail, for another.

## One entry of a config file somebody else owns

An MCP server reaches an agent as one entry in that agent's own config file
([D-10](#d-10-an-mcp-server-is-one-owned-entry-per-agent)): `.mcp.json`,
`opencode.json`, `.codex/config.toml`. Those files are the team's, the user's
and the agents' own CLIs', so `config_entry` changes the one entry it is asked
to and keeps every other byte. It knows no agent: which file, which key path
and what an entry holds are the caller's, and the agents' shapes are #90 to
#92's. Nothing calls it yet.

An entry is found by its key path, the keys of the objects that lead to it
(`["mcpServers", "github"]`), and holds a `config_entry::Value`. Its own type
rather than `serde_json::Value` or toml_edit's, because one entry is rendered
into both formats and neither can stand for the other: JSON has a null that
TOML lacks, and TOML keeps integers and floats apart where JSON does not. A
value a format cannot write is refused there, not bent into something it was
not. Arrays are not walked into: an entry Mindflayer owns is a member of an
object, which is what lets it be told apart by name from everybody else's.

### Edited in a tree that holds every byte

JSON and JSONC go through jsonc-parser's CST (its `cst` feature), TOML through
`toml_edit`, the crate the markers are edited with. Both hold the whole text,
comments and whitespace included, and change one node of it. Parsing into a
value and serializing it again, as `serde_json` would, writes the whole file in
the library's layout and drops every comment. The agents' CLIs are no way
round it either: D-10 turned them down, since `codex mcp add` writes only the
user's config and re-serializes every entry in it.

The format is the caller's to say, never guessed from a file's name: OpenCode
reads `opencode.json` as JSONC. JSON is held strict, because Claude reads
`.mcp.json` with a strict parser, so a file with a comment or a trailing comma
is refused rather than edited, and one without stays without. JSONC allows
comments and trailing commas and nothing more: jsonc-parser's defaults take
every JSON5 extension (single quotes, bare keys, hexadecimal numbers), which
the agents' parsers refuse. The text is parsed once without the CST to hold it
to that, because in 0.34 the CST's own parse lets a comment through
`allow_comments: false`. Two things no option of its covers are refused in
JSON by a scan of the text once it has parsed: a control character written as
itself in a string, and whitespace other than space, tab, LF and CR between
tokens, both of which `JSON.parse` refuses.

Where a library keeps less than every byte, the module makes up the
difference, and the tests pin what is left:

- toml_edit files the comments above an entry with the entry, so removing it
  would take them, where jsonc-parser leaves them. A TOML entry with comments
  above it is first replaced by a one-line stand-in that keeps them, marked by
  a comment nothing else in the file holds, and that line is cut from what
  toml_edit writes. Dotted keys file them with the last key of their first
  line, which a replace carries over to the new first line.
- toml_edit writes every line ending as LF. A file whose lines all end in CRLF
  has them put back, and one that mixes the two is refused, since which line
  had which would be lost. jsonc-parser keeps the file's own. toml_edit also
  ends the last line and drops a byte-order mark: a file whose last line had
  no ending is left without one, and a mark is put back.
- A string with a line break is written on one line, the break escaped, where
  toml_edit would open a multi-line string: in a CRLF file its breaks would
  then be the file's line endings to the file and part of the value to its
  reader. A float is written as the fingerprint writes it, in both formats,
  where the libraries write every digit of `1e300`.
- A TOML object is laid out as the objects already beside it are: a `[table]`
  of its own after theirs, inline, or dotted keys. Anything else is a key after
  the keys beside it, indented as the one before it.
- jsonc-parser's own `append` lays the object an entry goes into out again
  first: it drops the blank lines just inside the braces and breaks an object
  written on one line onto several. So a JSON insert takes only its place from
  the CST, after the last member and the comment on that member's line, and is
  spliced into the text there, laid out as jsonc-parser lays out what it is
  given: on a line of its own, indented as the last member, in an object
  written on several lines, and on the same line in one written on one. A
  replace and a remove go through the CST, which changes nothing outside the
  member.

### What an edit is held to

What an edit wrote is read back before it is handed over: it must parse, the
entry must hold what was asked (nothing, for a remove), and everything else
must hold what it held before, compared as values (`config_entry::kept`). A
library that changed more than the entry is then a refusal, not a file somebody
has to repair. The objects on the entry's way that it leaves empty are left out
of that comparison, because the formats differ there: jsonc-parser keeps
`"mcpServers": {}` when its last entry goes, and toml_edit drops a
`[mcp_servers]` that was never written, only implied by `[mcp_servers.github]`.

A key written twice on the way to an entry, or as the entry, is refused rather
than one copy picked, since which one an agent reads is its parser's. A replace
with what the entry already means hands the text back untouched, so installing
again over an entry somebody reformatted leaves their layout alone.

### The fingerprint

An entry can carry no mark of its own: OpenCode refuses keys it does not know,
`codex --strict-config` fails on them, and `.mcp.json` allows no comments. So
what the ledger will keep of an entry Mindflayer wrote (#93) is its
fingerprint, `Value::fingerprint`: the SHA-256 of its canonical JSON form, as
[RFC 8785](https://www.rfc-editor.org/rfc/rfc8785), the JSON Canonicalization
Scheme, writes it. Members sorted by UTF-16 code units at every depth, no
whitespace, strings escaped only where JSON must be, and numbers written as
ECMAScript writes a double, so `1`, `1.0` and `1e0` are one value. A published
scheme rather than one of ours, and one that keeps no integer apart from a
float: a file Claude rewrites goes through `JSON.stringify`, which keeps
neither, so anything stricter would call an entry nobody changed edited. It
departs from the scheme only for what JSON cannot spell. An integer past 2^53
is written digit for digit, since TOML's are 64-bit and two of them must not
read as one, and a float that is not finite as ECMAScript names it.

The fingerprint stays the same whatever a file does to an entry's layout (its
indentation, the order of its keys, a table or an inline table, its quotes) and
changes with any value. So a reformatted file still holds Mindflayer's entry,
and an edited one holds its editor's. A digest rather than the canonical text,
so every row is as long as every other, as a copy's `hash::tree` is.

### The file around the entry

`ConfigFile` reads the text from a file and writes it back. The file is named
by a route of plain folder names from a folder the caller trusts, a project's
root, and the route is checked by its spelling before anything is read, as a
project's artifact directories are kept inside it. A symbolic link anywhere on
that route, the file included, is refused, for a read as for a write. An
agent's folder, `.claude/` or `.codex/`, is somebody's to point anywhere, and a
gathered repository can hold one that points at the user's own `~/.claude`: an
entry written through it would land outside the project, in a file of the
user's, which the project's ledger row would then claim. The root itself may
be reached through a link, as a project often is. The route is looked at when
the file is read and again just before it is written, since a folder on it may
have become a link in between, and the write goes to the path as spelled,
never resolved: `replace_bytes`, which the markers go through, resolves it
first, to follow a config somebody shared by a link
([open questions](#open-questions)).

A file that does not parse is refused and left as it is: what it was meant to
say is its owner's to settle. A changed file is written whole through
`replace_bytes_unresolved`, a temporary of its own and a rename, with its
permissions kept, so a crash leaves the old file or the new one. A file not
there yet is
made, with the folders on its route, by `create_dir` and `create_new`, where a
rename would put it over one an agent's CLI made after the read found none. A
replace that changes nothing writes nothing, modification time included.

The caller holds the workspace's lock, as an install does, so two of
Mindflayer's writers take turns over one file. `ConfigFile` takes none: the
lock is not re-entrant, and nothing here knows about workspaces.

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
two are dropped, continuation lines and all, before anything parses the rest,
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
(`artifact::rule_issues`): front matter that does not parse, a `paths` or a
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
tightened (`a--b`) or one made by hand is exactly what has to be found to be
fixed. A template is looked up by one folder name for the same reason, since
`mind templates` lists whatever is there. More than one match is an error: changing the wrong artifact is
not something to guess about. Both looks are only in the folders the project
authors in (`Catalog::discover_authored`): a listing also shows what sits in
a project's agents' folders, and none of that is the project's to change, so
a name found only there is refused (`NotAuthored`). So is a rule the project
only received, a section of its block, never authored in place
([D-23](#d-23-what-a-project-received-is-an-installed-view)): a front end
finds what to change through `lifecycle::find_to_change`, which refuses one
the ledger records as received (`Received`), saying how to take it out. The
TUI lists unreadable files in the same way, by the name their place gives
them (`lifecycle::identify`), so they can be reached with a key rather than
only read about in a warning.

### Rename rewrites one line

A skill's name is in two places, its folder and its front matter, and they have
to move together or `validate` fails. The front matter is somebody's writing,
so it is not re-serialized: the one `name:` line is replaced and every other
byte kept (`frontmatter::with_key`). A name spread over several lines is not
touched, and neither is a manifest where the replacement would change anything
but the name: the result is parsed back and compared, field by field, before
anything is written. The folder moves first and the manifest is replaced
through a temporary file; if the second fails, the folder moves back.

### Remove is a dry run until it is told

`mind remove` without `--yes` deletes nothing and says what it would have, the
way `git clean` refuses without `-f`. It is the one command here that cannot be
taken back, so it is asked for twice: once by naming what, once by saying yes.
The TUI asks with a question and then runs the command with `--yes`, so the
transcript shows the yes that was given. A `flayer remove` without `--yes`
resolves its workspace as a read, as the dry runs of `update` and `migrate`
do, so a dry run in a home nothing has made a workspace in leaves none
behind.

### Removing one of the workspace's own everywhere

One of the workspace's own reaches a project by being installed there, so
removing it from the library alone leaves every copy installed from it with
a record nothing offers any more: orphaned, never updated again, and taken
out only by an uninstall somebody remembers to run. `flayer remove`
therefore names, beside a removal of one of the workspace's own, each
project that has a copy of it, in the dry run with what `--everywhere` would
do about it and after the removal with what takes them out now ([taking out
what nothing offers](#taking-out-what-nothing-offers)), and a warning, since
the copies left were not asked about. A removal that fails names none of
them: it may have left the library copy whole, offering it still, or a
skill's folder half deleted, offering nothing, and saying them orphaned, or
sending somebody to `--everywhere` against a folder no longer found, would
each be wrong half the time. `flayer remove --everywhere` is that
done for you: core's `everywhere::plan` says what it would do, reading the
ledger without making it, and `everywhere::apply` does it.

- **The copies first, then the library.** Each project the workspace links
  is surveyed for the artifact's name (`install::surveyed_one`), and each
  copy nobody has changed since it was installed is uninstalled as `flayer
  uninstall` would (`install::uninstall_with`, never forced): current,
  outdated, missing one of its copies, or orphaned already. The library copy
  goes last (`lifecycle::remove`), because a record from before hashes were
  kept is told from an edit only against its offer, which the library copy
  is.
- **D-04 decides what stays.** An edited copy is somebody's work: left, every
  copy of it, with its record, and from then on offered by nothing, which a
  note says with the uninstall that takes it out once somebody has looked.
  Somebody's own of that name, which nothing records (`install::somebodys`),
  is never Mindflayer's to take out, and is listed as foreign so it is not
  taken for a copy left behind.
- **Only what came from the library.** A record naming the workspace's own as
  its origin came from it. One installed from a loaded workspace or the
  shelf under the same name stays, offered still by where it came from. One
  from before origins were kept may have come from the workspace's own or
  from any loaded workspace, and counts as the library's only while no
  loaded workspace offers its name (`offered_for` matched against the
  library alone and against everything else): taking out what may be
  another workspace's would be a guess. The library copy is matched by its
  place rather than found among the offers, so one whose front matter no
  longer reads still finds what was installed from it.
- **Only the projects it links.** A project another workspace links, or none,
  keeps what was installed there, since the ledger that could say so is not
  this one; a project linked but unreadable is a warning, as for `update
  --all`.
- **The library copy goes whatever the projects came to.** A copy whose
  uninstall fails is an error of its project, said beside the rest, and the
  library copy is removed all the same: what is left is orphaned, as a copy
  left because it was edited is, and `uninstall --orphaned` finishes it.
  Keeping the library copy back would ask for the whole removal again to
  finish one project, and would make a failed project and an edited one two
  different leftovers of one command.
- **Each project's uninstall takes the lock for itself,** as `flayer
  uninstall --all-projects` does ([one artifact into many
  projects](#one-artifact-into-many-projects)), and looks again under it at
  what the plan saw: a copy edited between the two is left all the same,
  and said as left.
- **A rule goes the same way.** Its uninstall renders the project's block
  without it, or takes its copy out of a project a `mind.toml` marks.

The TUI's confirmation for `d` on one of the workspace's own offers a second
yes, `e`, which runs `flayer remove --everywhere --yes`, and shows the line
each key runs; on a project's own, which was installed nowhere, it keeps the
one yes.

### A record must not outlive the copy it is about

The ledger's record of an installation is what gives an install leave to
overwrite and an uninstall leave to delete. Remove an installed skill and the
copy it was about is gone; a record left behind would be inherited by
whatever is written there next, and the install screen would then delete
somebody's work because a box looked untidy, or, now that a copy gone reads
as missing, put back what was removed on purpose. So `lifecycle::remove`
uninstalls the copy it reached, dropping its record, and `lifecycle::add`
drops any stale one under the name it writes, as `lifecycle::rename` does
under the new name.

Edit or rename an installed copy and it is still there. Its record stays
([D-04](#d-04-an-edited-copy-is-detected-not-disowned)), and the hash it
holds is what tells an edit apart ([an edited copy is left
alone](#an-edited-copy-is-left-alone)): edited, status says so, and no
install writes over it, nor uninstall takes it out, unless forced.
`finish_edit` leaves the record as it is and only reads the ledger to say
so (`Record::Kept`); `rename` carries it to the new name and place
(`install::carry`, `Ledger::renamed_copy`), from where it came and with the
hash of what was written. A skill's rename rewrites its `name` line, which
reads as the edit it is; a rule's moves its file as it is, so it reads as
it did, edited only if it was, and otherwise as what was installed, which
an uninstall takes out unforced. Which of the two it is is told as an
install tells it, and handed back (`Record::Moved`'s `edited`), so what
`mind rename` says of the copy is what an install and an uninstall will
find. Dropping the record, as both once did, made the
copy its author's: safe, but never updated again and never taken out by the
uninstall that put it there. Where another installation holds the new name
already, the renamed copy cannot join it, a name being one installation a
project, and its record goes instead, as it did (`Record::Dropped`).

Only a record that names the copy the change reached, by its path
(`install::disown`'s and `install::carry`'s `at`), not every record of that
name. A project marked by its settings authors in a folder of its own,
beside the copies installed for its agents: adding, editing, renaming or
removing its own `deploy` changes nothing in `.claude/skills/deploy`, and
seeing to that copy's record by name would drop or carry a record over a
change to something else. In a project a `mind.toml` marks, the folder it
authors in is the one it was installed into, so a change there reaches the
copy. A skill's record naming no target, which only a build from before
targets wrote, is dropped by name as records were. A rule's naming none is
one a build writes, for a rule that reaches no agent the project names
([installing rules](#installing-rules)), and a change to the project's own
rule of that route reaches nothing it installed, so it stays.

And only that copy's part of the record (`Ledger::uninstalled_copy`,
`Ledger::renamed_copy`), in one transaction: settings can make the folder a
project authors in one of its targets, `"skills": ".claude/skills"`, and a
change there reaches the copy an install put there and no other. The copy in
`.agents/skills` is still what was installed, so its target stays recorded,
for `flayer install` to update and `flayer uninstall` to remove; the
installation goes with its last copy, or takes the new name whole when a
rename reached its last. A renamed copy that leaves others behind is an
installation of its own under the new name, from the same place. In a local
project the place a removal reached also leaves the exclude block, after the
record, as an uninstall takes its entries out after its copies: what is
written there next is its author's, and a line left behind would hide it
from `git status`. A rename moves the entry to the copy's new place, which is
still an install's and kept out of git. A block that cannot be edited is
reported beside the change as a record that cannot be changed is
(`InstallError::StillExcluded`, `InstallError::NotExcluded`).

Core applies that rule inside each change. Dropping the record in the CLI
after the change, as it once did, put a rule outside core, and
[D-18](#d-18-the-panel-writes-by-running-commands) asks for the orchestration
left in the CLI to move into core before the panel runs its writes through
the CLI. Installed rules and MCP entries, when they come, would each have
needed calls of their own there, and each could have got them wrong. Each
change returns what it did to the record (`lifecycle::Record`), with what
opening the ledger to do it scrubbed out ([A URL is kept without its
credentials](#a-url-is-kept-without-its-credentials)), and the CLI only says
so; `install::disown` and `install::carry`, which change records, are
private to core so that nothing else can. That keeps a front end from
applying the rule differently, not from skipping it: `create` and
`create_from`, the writers `lifecycle::add` is built on, see to no record and
stay public for core's tests, so a front end adds through `add`. A record
that cannot be changed is reported beside the change rather than instead of
it, and the CLI prints it as a warning: the change has been made, and saying
it was not would be false.

Editing is split in two, because the editor is not core's to run: which one
comes from `$VISUAL` or `$EDITOR`, and core never reads the environment.
`lifecycle::start_edit` notes what the file holds, the CLI runs the editor,
and `lifecycle::finish_edit` decides whether anything changed and says
whether an install of it stays recorded. The CLI finishes the edit even when
the editor fails: one that writes the file and then exits non-zero, as vim
does on `:cq`, has changed the copy all the same. Nothing is written to the
ledger either way, so nothing is scrubbed out of it for the error to carry.

The record seen to is the one held by the workspace the command resolved,
the one its templates come from, and each change is given it for the same
reason: a workspace beside the project, which a walk up from the project
never reaches, would keep a record of a copy removed. A change takes that
workspace rather than its ledger, because opening a ledger creates one, and a
workspace that never installed anything must not get a database because
something in a project changed; core opens it only when it is there. Another
workspace that links the same project keeps its own, an [open
question](#open-questions).

## Scanning

`flayer scan` walks down from the workspace root, or from each folder it is
given, looking for mind projects and git repositories, and says which of them
the registry already points at. It links nothing (a workspace manages what it
was told to); it only makes the telling easier, which is what the TUI's link
form and the minds screen are built on.

It had to be pointed somewhere else once the default workspace moved to
`~/.mindflayer`: a workspace in a home directory has nothing under it but the
home, and the projects are wherever somebody keeps them, say `~/Work`. So
core's `scan` takes a `Request`: the roots, none meaning the workspace root,
and the depth, `DEFAULT_DEPTH` (three) unless told otherwise. A front end
makes the folders it was typed absolute from the directory the command works
in, as it does the paths `link` is given, and each root is taken once
however it was spelled. A root named that is not there is a failure beside
what the others hold, said even at depth 0, so a typo is never an empty
answer.

The walk is bounded, because a root can sit at the top of a home directory.
It goes three levels down unless told (`repo`, `org/repo`,
`github.com/org/repo`), breadth first, so a budget of listed folders
(`MAX_LISTED`) runs out on the deepest rather than on a whole neighbouring
tree, and never into hidden folders, symlinks, `node_modules` or `target`. A
repository is a leaf, since its folders are its own. A root is the exception:
it is a candidate itself and is looked inside even when it is a repository,
whatever it is called and however it is reached, because somebody named it.
Every root starts the walk at once rather than one after another, so the one
budget, however deep the scan, is shared breadth first across them: when it
runs out it is the deepest folders that went unseen, never a whole second
root behind a first that filled it, and a folder two roots reach is looked at
once. A scan that runs out of budget says so (`truncated`) rather than
passing for complete. A folder that cannot be listed is a failure reported
beside what was found, except the workspace root when it is not there at
all: that holds nothing, and only the default workspace before it is made
can have one, in a home nobody has created yet.

What it reports is spelled to be typed. Each one found carries the route
`flayer link` takes for it from where the request says (`Request::from`):
the directory `flayer scan` runs in, and the workspace root for the TUI,
whose lines run there. A directory inside gets the route from there; one
outside, its path, rather than a route that climbs out with `..`, which reads
right from one directory only. A project marked by its settings carries the
id they name it by ([D-09](#d-09-project-settings-live-in-the-project)),
which is what names it for good whatever it is called and wherever it is; a
git repository, a project a `mind.toml` still marks and one whose marker will
not open have none, and the last is listed all the same, with a failure
beside it, since its marker still says it is a project.

A project found and not linked that holds the id the ledger last saw at an
entry where no project is any more is that entry's, moved, and carries the
entry (`moved`), spelled as `route` is, so the line it is listed with,
`moved: <old> -> <new>`, is the `flayer relink` to type from there. Only an
entry nothing marks (`MindProject::is_vacant`), not every stale one: one
whose marker is there and would not open still holds its project, whose id
nothing read, so a copy of it would pass for it moved; and one that opens
holding another id holds another project, which the ledger follows the next
time it is written. The ledger is read only when
something found could match, as a read ([a read leaves the ledger as it
finds it](#a-read-leaves-the-ledger-as-it-finds-it)), and one that cannot be
read is a failure beside what was found, listed with no move. An id two
directories hold, among those found and those the workspace links, is a
failure naming both, and neither is a move: a copy, which only
`mind init --new-id` tells apart.

The ledger learns an id only when it is written, so a project linked and
moved before anything wrote it is found by nobody, though the scan may list
it. While something found and not linked holds an id no entry was matched
to, each entry nothing marks whose id the ledger never learned is named
(`untraced`), and the text says to `flayer relink` it: the hint to
`flayer link` alone would leave the entry stale beside a second one for the
same project, which is what a relink is for. Which of them went where is
the person's to say, never a guess.

`flayer scan --json` prints the scan in [the envelope](#json-in-one-envelope),
each field by its name (`roots`, `depth`, `found`, each with `path`, `route`,
`id`, `mind`, `linked` and `moved`, `untraced` and `truncated`); its
failures, and that it stopped early, are the envelope's warnings. `moved`
and `untraced` came after the others, names added, so the envelope's
version stayed. The table leaves the id out: the path already tells its
rows apart, and the id is for a script to match a project by.

`flayer minds DIR` opens the minds screen looking under the folders it is
given rather than the workspace root, and the TUI keeps them and looks there
each time it reads the disk again, so a change made from the screen does not
send it back to the workspace root. It still lists every project the
workspace links: a folder says where to look for more, not which of its own
to show. What the scan could not read, and that it stopped early, the screen
says in a strip below the list, in the words `flayer scan` warns with, so a
folder typed wrong is said there too rather than drawn as one with nothing
in it.

## Writers at the same time

The commands were written for a person at one terminal, running one at a time.
The TUI runs them while another terminal runs its own, and the server under the
panel runs them while both do. Three things keep two writers from undoing each
other, and none of them makes a reader wait for the workspace.

- **A temporary of its own.** A file Mindflayer replaces (a marker, the
  `SKILL.md` a rename rewrites, git's exclude file) is written whole into a
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
  the file's header, so a ledger switches once, at the first open to write,
  and SQLite keeps `mindflayer.db-wal` and `mindflayer.db-shm` beside it while
  it is open. It switches only once `migrate` has accepted the version, since
  switching first would change a ledger a newer build wrote before refusing
  it, and a switch that fails is never a reason to refuse the ledger: one that
  finds another connection writing fails at once, because SQLite will not wait
  where waiting could deadlock, and is left to the next open. `open_existing`
  sets the timeout and not the mode, because switching is a write, and a read
  makes none but a migration.

  A ledger that keeps its log needs those two files made beside it even to be
  read, and SQLite takes them away when the last connection closes, so in a
  folder the reader may read and not write (a sandboxed agent's view of the
  workspace, a read-only mount, another user's workspace), the first read
  fails, where the journal the ledger had before let it be read.
  `open_existing` then opens it as SQLite's `immutable`, which needs neither
  file and takes no lock, but only when there is no log: then everything is
  in the file itself. Somebody who can write there writes into a log the read
  does not see, so what it reads is the ledger as their last checkpoint left
  it, and a checkpoint made between two of its reads is the one thing that
  could change the file under it.
- **A lock on the workspace.** Unique names keep each file whole, not each
  entry in it: two `flayer link`s that both read `flayer.toml` before either
  writes each put back a marker with only their own entry, and the second
  rename wins. So a writer holds `.mindflayer/lock` from before it reads what
  it will change until the change is written: an edit of the marker (`link`,
  `unlink`, `load`, `unload`, and the default workspace's, which `flayer
  init` and `workspaces forget` change), an install or an uninstall, a
  gather (clone and all, since two gathers of one source clone into one
  folder) and the scrub that opening the ledger to write runs
  (`ledger_to_write`), while it rewrites clones an older build left a
  credential in and removes the folders of a source made one with another,
  which a gather beside it would be filling.
  A migration of the workspace's own into its library holds it from its plan
  to its rewrite of the marker, and a project's move from its `mind.toml`
  from before it reads the project again until its ledger is written
  (`migrate::project`); a move from a `settings.json` changes nothing of the
  workspace's, and takes none.
  Core takes it inside each of them (`FlayerWorkspace::lock`), so no front end
  has to remember to. An install looks again at what the project holds once
  it has the lock: the standing it is handed comes from a survey made before,
  minutes before on the install screen, and a copy somebody has written or
  edited since is left alone rather than written over and recorded again.
  A change to a project's agents or share mode holds it too, from before it
  reads the project's settings until it has written them and carried the
  installs along (`reapply::apply`): the settings are the project's, but
  what changing them moves is the workspace's, and two changes at once in
  one workspace would otherwise each keep only their own. An update holds
  it from the plan it makes again until its last installation is written
  (`update::apply`), writing each through the body an install hands to once
  it holds the lock (`install::install_held`), since asking for the lock
  again, as `install_with` does, would wait for itself. A relink holds it
  from before it reads the ledger for its checks until the entry and the
  records it moves are written (`relink::relink`), and a new id from before
  it reads the project's settings again until the ledger knows it
  (`relink::new_id`), as a change to the agents does. An adoption holds it
  from the plan it makes again until its record is written
  (`adopt::apply`), so what the ledger says was adopted is what the library
  holds. Two workspaces
  linking one project are kept apart by neither's lock
  ([open questions](#open-questions)).
- **Opening the ledger takes no lock**, though it is when a ledger is made or
  migrated. A migration is one transaction that reads the version again once
  it holds SQLite's write lock (`Ledger::migrate`), so two processes opening
  one an older build left take turns there and the second finds nothing to
  do (a read's migration included, which the workspace's lock could not keep
  apart, since readers never take it). Taking that lock as well made every
  change to an artifact wait behind a gather to see to its record, once the
  change was made, and fail as busy when the gather ran long, leaving the
  record as it was. The scrub takes it only when it has clones to rewrite,
  the first write after an upgrade from a build that stored a credential,
  and a record dropped or carried then waits for it without a deadline: the
  change it follows is made, and trying it again would find nothing to
  change. Telling the ledger which id each linked project holds, which
  opening it to write does every time (`Ledger::knew`), takes none either:
  it writes rows, in a transaction that takes SQLite's write lock before it
  reads, so a relink at the same moment comes wholly before or after it.

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
  prints it, and the install screen prints it under the line it was running
  and goes on to the next.
- **Who holds it, in a file of its own.** The writer holding the lock writes
  its process id to `.mindflayer/lock.pid`, and takes it away while it still
  holds the lock, so it never names a writer that has let go. Not in `lock`
  itself, because a lock is mandatory on Windows: while one process holds a
  file locked, no other can read a byte of it, and the id is wanted by exactly
  the processes that cannot have the lock. Writing it is best effort, and a
  waiting writer that finds none says "another process".
- **Readers never take it.** A marker is replaced by a rename, and a read of
  the ledger sees only what was committed, so each file a read opens is
  whole, as it was before a write or after it. A change made in several steps
  is not seen whole: a read between an install's copy and its record finds the
  copy without the record, and calls it somebody's, and a read during a gather
  finds some shelf copies replaced and others not yet. With its log the ledger
  never keeps a read waiting for a write; in the journal a filesystem without
  one leaves it in, a read waits for a write to be committed, as readers and
  writers take turns there.
  `ledger_for_reading` migrates an older ledger as `ledger` does, in the same
  transaction, so the most a read waits for is another process's migration,
  once after an upgrade.
- **Not re-entrant.** Two handles on the file are two writers even in one
  process, which is what lets threads stand in for processes in the tests,
  from a link on each of two threads at once to every writer waiting for a
  lock the test holds. It also means a writer that asks for the lock while it
  holds it waits for itself, and fails as busy. Core's writers each take it
  once and call none of the others while they hold it, and a front end does
  not take it at all.
- **A filesystem without locks lets writers through unlocked**, as it lets
  installs through without a sweep: refusing would refuse every change there.
  Any error taking the lock counts, for the reason a stage gives: NFS
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
`.mind` committed, as a `mind.toml` project's was, against the owner's ask for
an ignored one; an
anchored `/.mind/` line in `$GIT_COMMON_DIR/info/exclude`, which stays the
mechanism for installed copies (D-02) because they have no Mindflayer folder to
hold a `.gitignore`; and a line in the repository's `.gitignore`, which edits a
team file. A fresh clone has no `.mind`, and `scan` already offers plain
repositories. Unblocks #29, #35 and #38.

Carrying it out, #29 writes the `.gitignore` (`MindProject::init_settings`),
and #35 makes it what `mind init` does, with `--shared` to leave it out (see
[a project's settings](#a-projects-settings)). #38 writes it for a project
moved from its `mind.toml` with `--local`, and says how to stop git tracking
a `.mind` committed before, which the `.gitignore` does not ([moving a
project to its settings](#moving-a-project-to-its-settings)).

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
agents reading git-ignored files, which none documents. #17 confirmed it on
Claude Code 2.1.309 for skills, `AGENTS.md`, `.claude/rules` and
`CLAUDE.local.md`; for Codex and OpenCode it rests on their source, with
shared mode as the fallback ([docs/agents.md](agents.md)). Local copies never
reach teammates or cloud runs, and reach another worktree only for Claude, and
only when that worktree has no `.claude/skills` of its own. Unblocks #17, #32, #34, #37, #38, #43, #44, #45 and #94.

Carrying it out, #34 keeps a local project's installed skills out of git
with one entry per copy in its group of the exclude block, keyed by the
project's id, written before the copies land and taken out after they go;
a shared project's exclude file is left alone ([What keeps an install out of
git](#what-keeps-an-install-out-of-git)). #37 lets the share mode change,
with `mind share`: a project turning shared loses its `.gitignore` and its
whole group, one turning local gains both ([when a project's agents, targets
or share mode change](#when-a-projects-agents-targets-or-share-mode-change)).
#38 moves
a v2 project to settings shared unless `--local` ([moving a project to its
settings](#moving-a-project-to-its-settings)). #43 keeps Claude's rule files
out of git the same way, an entry a file ([Claude's rule
files](#claudes-rule-files)). #44 adds the `@AGENTS.md` import to a
project's `CLAUDE.md` that git tracks only in a shared project or with
`edit_tracked`, and otherwise gives Claude every rule through those files
([how Claude reaches the block](#how-claude-reaches-the-block)). #45 keeps
an `AGENTS.md` Mindflayer made out of git, and writes no block into one git
tracks in a local project without `edit_tracked`, saying which agents then
miss the rules ([installing rules](#installing-rules)). #59 writes no line
for a copy git tracks, as an adopted one often is, whether an install, an
update or a project turning local would have written one, since it keeps
none of the copy out ([what keeps an install out of
git](#what-keeps-an-install-out-of-git)).

### D-03: Rules reach every agent through `AGENTS.md`

Assigned rules are rendered between `<!-- mindflayer:begin -->` and
`<!-- mindflayer:end -->` in the project's root `AGENTS.md`, one section per
route. Where Claude would not read that file, a managed `@AGENTS.md` import
goes into the project's own `CLAUDE.md`, or
`.claude/rules/mindflayer/<route>.md` when no file may be edited (D-02); a rule
that declares `paths:` goes only there. One core predicate (#44) decides,
walking every ancestor directory but `/` (see below) and ignoring the user
memory file. The root
`AGENTS.md` is the only per-project file all three agents read, and Codex has
no include mechanism. Claude 2.1.277+ reads it natively only when no
`CLAUDE.md`, `.claude/CLAUDE.md` or `CLAUDE.local.md` sits in the start
directory or above it; the import covers the other case, and Claude before
2.1.277. Rejected: per-agent files, under which Claude reads every rule twice
when there is no `CLAUDE.md`; and OpenCode's `instructions` globs with Codex's
`developer_instructions`, unverified and honoured only in trusted projects.
Mindflayer never creates a `CLAUDE.local.md`, which silently disables
`AGENTS.md`, and warns when an `AGENTS.override.md` hides the file from Codex
or the chain nears Codex's 32 KiB budget. #17 confirmed these Claude behaviours
on Claude Code 2.1.309: a `CLAUDE.md` above the git root or a `CLAUDE.local.md`
stops `AGENTS.md` being read, the user memory file and `.claude/rules` do not,
the block's text reaches Claude without its markers, the import loads it once,
and a rule with `paths:` loads only when a matching file is read
([docs/agents.md](agents.md)). Two it did not try: that a `.claude/CLAUDE.md`
counts as well, seen only among the names in Claude's debug output, and the
2.1.277 floor, which is docs-only. One corner differs from what this said
when validated: Claude's check most likely stops below `/`, so a `CLAUDE.md`
at `/` alone does not stop it reading `AGENTS.md` (read from the directory
counts in its debug log, not tried). #44's walk stops below `/` too: every
ancestor directory but the filesystem's root, so the predicate agrees with
Claude, where walking up to and including `/` would choose Fallback while
Claude still read the block, and give Claude each unscoped rule twice.
Unblocks #17, #42, #43, #44 and #45.

Carrying it out, #42 renders the block, between those markers and under a
`<!-- mindflayer:rule <route> -->` for each rule, leaving out a rule that
states `paths:`. It leaves no block in a folder above a project the
workspace links, nor in the workspace's own root where no project it links
is, nor inside an agent's own folder, since the agents of other projects
would read an `AGENTS.md` there too, and takes out one it finds there; a
project at the workspace's root has its rules in its own. It warns of the override, the
budget, and a project in no git work tree, where Codex reads only the
folder it starts in ([how a rule reaches the
agents](#how-a-rule-reaches-the-agents)).

#43 writes a rule that states `paths:` as
`.claude/rules/mindflayer/<route>.md`, front matter kept, and every rule there
in the fallback mode #44 chooses, never because a block was written alone.
It says that such a rule reaches Claude alone, naming the project's agents
it misses. What is Mindflayer's there is what its caller recorded of an
earlier render, each file's hash among it, so a file nothing recorded, or
one changed since, is never written over or taken away. It is refused where
the block is, and through a link out of the project ([Claude's rule
files](#claudes-rule-files)).

#44 is the predicate, `claude_md::reads_agents_md`: the three names in the
project's root and every folder above it but `/`, the user memory file left
out by the file it is. `claude_md::reach` turns it, the project's settings
and whether the block was written into Native, the import in the project's
own `CLAUDE.md` or `.claude/CLAUDE.md`, or the fallback with its reason, and
two more: a `CLAUDE.md` that is the `AGENTS.md` through a link, which Claude
reads the block as, and an `@AGENTS.md` line somebody already wrote in one
of Claude's files, which Claude reads the block through, so that a team's
own import, in a file Mindflayer may not edit too, never leaves Claude
reading a rule twice. `claude_md::render` adds the import once, between the
block's markers at the top of the file, and takes it out with the last rule;
it never makes a `CLAUDE.md` or a `CLAUDE.local.md`, and never edits a file
above the project, reporting it as the fallback's reason instead ([how
Claude reaches the block](#how-claude-reaches-the-block)).

#45 wires the three to installing: a rule installed into a project marked by
its settings, or taken out of it, renders every rule the project has, the
block, then the import, then Claude's rule files in the mode the reach says,
and records each rule with a hash of its section and of its file, and
whether Mindflayer made `AGENTS.md`. A project a `mind.toml` marks is given a
copy of the rule's file in its rules folder instead ([installing
rules](#installing-rules)).

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
differs from its installation's in form (a rule's section of the managed
block in `AGENTS.md` beside its `.claude/rules/mindflayer/<route>.md` file,
as D-03 has it) shares no one tree with the others, and would need a hash of
its own on `installation_targets`, added by the schema step that first
records one, which #45 did for rules. #38 holds a v2 project's records from
before hashes to their offers when it moves the project to settings, and
gives a record whose copy matches the hash every copy then holds ([moving a
project to its settings](#moving-a-project-to-its-settings)).

#47 reads the hashes: `Standing` tells an installation current, outdated,
edited, missing or orphaned ([an edited copy is left
alone](#an-edited-copy-is-left-alone)), for a skill's copies, a rule's
section and its file for Claude, and a rule's copy in a project a
`mind.toml` marks. `install` and `uninstall` refuse an edited copy, the whole
change with it, unless `--force`; a render keeps an edited section as the
block holds it, as Claude's rule files were kept; `status` says `edited`;
and the lifecycle keeps the record through an edit, carries it through a
rename, and drops it with a removal, which uninstalls the copy ([a record
must not outlive the copy it is
about](#a-record-must-not-outlive-the-copy-it-is-about)). A record with no
hash is held to its offer in code, with no schema step, and the first
install over a copy that matches stores its hash. A change of agents, or a
move to settings, still drops the record of an edited copy it leaves in a
folder no agent the project names is sent copies to: no install writes
there again, and no listing reads it ([when a project's agents, targets or
share mode
change](#when-a-projects-agents-targets-or-share-mode-change)).

#48 acts on them: `flayer update` and `mind update` write every outdated or
missing installation again from where it came from, leave an edited one,
every copy of it, unless `--force`, and say an orphaned one without failing
([bringing what was installed up to
date](#bringing-what-was-installed-up-to-date)); and `status` says all five,
`current`, `outdated`, `missing` and `orphaned` taking over from `installed`
and `partial` ([what each project has](#what-each-project-has)).

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
v3 project marker was a JSON file, `.mind/settings.json`, which names those
folders (`directories`) beside the project's agents, share mode and id, so any
tool or editor can read and change it. Amended again by the owner
(2026-10-27): the settings are TOML, `.mind/settings.toml` at version 5, like
`flayer.toml`, so both markers are written from a template whose comments say
what each key does and edited through toml_edit alike ([the
markers](#why-the-markers-are-toml-written-from-a-template)); a
`settings.json` is still read as written, and migrated.
Installs go to the agent folders the project targets (D-07), not to the
authored ones. That keeps every existing workflow, makes the workspace the
default source, and keeps the marker minimal. Rejected: receiving only, which
would strand what v2 projects authored; and today's model unchanged, against
vision points 2 and 8 (the flayer as the source, a minimal `.mind`). The plan
made the folders opt-in (`--skills`, `--rules`); the owner prefers them to
exist from the start. Unblocks #29, #33, #35 and #59.

Carrying it out, #35 makes `mind init` write settings that name `skills/` and
`rules/`, or the folders `--skills` and `--rules` give, and make them. #166
carries out the second amendment: `mind init` writes a `settings.toml`, a
change edits it a key at a time through toml_edit, a `settings.json` is read
as written and changed by nothing but `mind migrate`, which moves it, and a
`mind.toml` straight, to a `settings.toml` ([a project's
settings](#a-projects-settings), [moving a project to its
settings](#moving-a-project-to-its-settings)). #59 brings what a project's
agents read and nothing installed, a skill somebody keeps in
`.claude/skills` by hand, into the workspace's library when asked, its
copies recorded as installed from there and left as they are; what a
project authors is never adopted ([adopting what a project already
keeps](#adopting-what-a-project-already-keeps)).

### D-07: One copy per agent folder

Amended by the owner. An installed skill is a real copy in
`.agents/skills/<name>`, which Codex and OpenCode read, and an identical one in
`.claude/skills/<name>` for Claude, staged and written in one step; the ledger
records one installation with several targets. That layout is the default, and
a project's settings (`.mind/settings.toml` since D-06's second amendment)
may override it per kind, for example with one `.skills` folder for every
agent (#40). Copies fit today's install model
and the Windows CI, survive the hub moving, and keep gathered content
contained. OpenCode sees the skill twice, which is harmless only while the
copies are identical, so an install is refused if either target holds foreign
content. Rejected for now: a relative symlink in `.claude/skills`, which may
come later as a Unix opt-in, since #17 showed Claude 2.1.309 loads a symlinked
skill once (its exclude entry has to drop the trailing `/`, because git never
follows a symlink and sees a file);
and symlinks into the hub, which break projects when `~/.mindflayer` moves and
leak its path into repositories. `.codex/skills` and `~/.codex/skills`,
deprecated or undocumented, are never targeted. Unblocks #17, #28, #34 and #40.

Carrying it out, #28 puts the layout in core's target table
(`TargetTable::standard`, see [Agents, and where each kind reaches
them](#agents-and-where-each-kind-reaches-them)), a value every caller is
handed so the override can take its place. #34 installs into it:
every copy is built from one tree in the project's stage and put in place
with the others or not at all, the installation is recorded once with a
target for each, and somebody's work in either folder refuses it, in both
([Installing](#installing)). #40 reads the override from a project's
settings, `targets`, which sends a kind to the folders it names for every
agent, at version 4 of a `settings.json` and in any `settings.toml`, and warns
of agents that read none of them ([a project's own
targets](#a-projects-own-targets)).

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
uses](#which-workspace-a-command-uses)). #55 added what sets the variable,
`flayer activate` and `flayer-deactivate`, and `flayer which`, which says the
reason (see [activating a workspace](#activating-a-workspace)). #56 lists the
workspaces this machine knows, with the one `resolve` chooses marked (see
[the workspaces this machine knows](#the-workspaces-this-machine-knows)).

### D-09: Project settings live in the project

A project's agents, share mode and `edit_tracked` live in its own settings,
the v3 marker `.mind/settings.json` and from version 5 `.mind/settings.toml`
(D-06), ignored by default (D-01). They describe the
files physically in the project, and several workspaces may link one project,
so keeping them beside it means every workspace sees the same targets.
Rejected: `[[project]]` tables in `flayer.toml`, which let two workspaces
disagree about one project's files; and the ledger, which hides editable
configuration in a database. Unblocks #29 and #37.

Carrying it out, #29 adds the format, `Settings` in `settings.rs`, which
`MindProject` reads from a settings file before a `mind.toml`, and
`MindProject::init_settings`, which writes it with D-01's `.gitignore` (see
[a project's settings](#a-projects-settings)), and #35 makes `mind init`
write one, naming the agents `--agents` gives and the share mode `--shared`
chooses. #37 changes them, with `mind agents` and `mind share`, and carries
what is installed into the project along ([when a project's agents, targets
or share mode change](#when-a-projects-agents-targets-or-share-mode-change)).
#40 adds `targets`, changed with `mind targets` through the same path ([a
project's own targets](#a-projects-own-targets)).

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

Until then, #34 refuses an install whose copy would land inside one of the
agents' own folders, which a project at the user's home would otherwise
reach by accident ([never into an agent's own
folder](#never-into-an-agents-own-folder)).

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

Carrying it out, #45 lists the rules a project marked by its settings
received in `status`, from the ledger and the block of its `AGENTS.md`, and
never through discovery ([what each project has](#what-each-project-has)).
A lifecycle change finds no section, since it looks only where the project
authors, and #46 says so with a hint to uninstall: the front ends find what
to change through `lifecycle::find_to_change`, which refuses a name the
project authors nothing of but the ledger records as a rule it received
(`LifecycleError::Received`), where "nothing named" would deny what `status`
just listed.

## Open questions

- **A copy nobody linked.** `flayer relink` refuses a copy it can see: the
  entry still holding the id, or another project the workspace links
  holding it. A copy somewhere on disk nobody linked is beyond what it
  reads, so a project moved and also copied, both holding its id, is
  followed to whichever directory the command names. `flayer scan` says the
  two are one id and offers neither as a move, and the TUI relinks only what
  a scan offered. Scanning from the relink would close it, at the price of a
  walk of the disk to change one line of a marker, and of an answer that
  depends on where the walk looked.
- **A moved project linked again by hand.** Linking the directory a project
  moved to, rather than relinking it, leaves two entries for one id: the
  stale one keeps it, for a relink, and the new one is known by none, so
  what is installed there from then on is recorded with no id. `flayer
  relink` refuses a directory linked already; unlinking it first hands the
  stale entry's records to it and drops those, which an install made since
  wrote. Merging the two, the newer record of a name winning, would keep
  both, at the price of a relink that sometimes drops an entry rather than
  rewriting one.
- **A project moved before its id was learned.** The ledger learns ids when
  it is written, so a project linked and moved before then is never offered
  as a move; the scan names its entry (`untraced`) for a relink somebody
  types. `flayer link` telling the ledger would close it, at the price of
  every link opening the ledger to write, and making it, which only a
  gather, an install or an uninstall does now.
- **A route that holds another project now.** An entry whose directory is
  given to another project once its own has moved opens, so it is not stale:
  the ledger follows the project there (`Ledger::knew`), and the records the
  route had become the newcomer's, which names copies it may not hold. The
  moved project is then found by nobody. Telling the two apart needs more
  than the route and the id: the records' copies being there, or the
  settings' name, which a person can change.
- **Adopting into one project at a time.** `flayer adopt` takes one
  project's copy, so a skill several repositories keep by hand, each its own
  copy, is adopted from one of them, and in the others it is still foreign,
  and refused by an install of the adopted one. Adopting from every project
  holding the same bytes at once would bring them all under one record each,
  at the price of a batch that must agree on what the skill is before it
  writes anything, and say why one project differs.
- **A tracked copy in a local project, written by an update.** An adopted
  copy git tracks stays in git, so an update in a local project changes a
  file the team committed, where local mode otherwise edits a tracked file
  only with `edit_tracked`. Adopting by name is taken for consent, and the
  dry run says so; holding such an update back until the project is shared
  or sets `edit_tracked` would make the consent a setting, at the price of
  an adopted copy no update reaches by default.
- **Precedence between projects.** Two projects in one workspace can declare
  the same skill name. Core returns both matches and says nothing about which
  wins, because nothing here yet has to choose. Whatever resolves it (a
  workspace-level override, an explicit order in `flayer.toml`) belongs in core
  when it exists, not in a front end.
- **Forcing from the install screen.** An edited install's box is held on
  the screen, and only `--force` on the command line writes over or takes
  out its copies. A key that marks a row to be forced, confirmed as the
  batch is, would let the screen do what the command line does: the line
  its marks amount to would say `--force`, for the projects forced apart
  from the rest.
- **Gathering still stages beside.** `copy::replace` builds a shelf copy as a
  hidden sibling, `.<name>.mindflayer-new`, under names taken from the skill,
  so a gather stopped halfway leaves its staging on the shelf. Two gathers no
  longer meet there, since each holds the workspace's lock ([Writers at the
  same time](#writers-at-the-same-time)). No agent reads the shelf, which is
  why installs moved first. A `copy::Stage` in a scratch folder of the
  workspace's own would do for gathering what `.mind/tmp` does for installs.
- **Removing is not staged.** `uninstall` deletes each copy where it is, so
  one stopped halfway leaves part of a skill in a folder agents read, or one
  agent's copy gone and the other's still there, with the record naming both.
  Renaming every copy into a stage first, and deleting them there, would make
  a removal as all or nothing as an install, at the price an install already
  pays: a target on another filesystem than the project's `.mind/tmp` could
  no longer be removed, where today it can.
- **Removing everywhere is not one change.** `flayer remove --everywhere`
  uninstalls project by project, each taking the lock for itself, and then
  removes the library copy whatever they came to, so a project whose
  uninstall failed, or a writer that came between two of them, leaves an
  orphan behind for `uninstall --orphaned`. Holding the lock across the
  projects and the library would make it one change, at the price of a body
  of each uninstall, rules' render included, that runs under a lock it does
  not take, as an update has.
- **A batch takes turns line by line.** The install screen's lines run one
  after another, and each install in them takes the workspace's lock for
  itself, project by project, as `flayer install` given several projects
  does ([one artifact into many
  projects](#one-artifact-into-many-projects)): another writer can come
  between two, and with the workspace busy each waits the whole ten seconds
  before it is refused. Holding the lock across the screen's batch needs
  core to apply it as one, which is where #78 plans it, and would want the
  transcript to say it in a line that runs it again as one.
- **One origin for two folders of one repository.** A source is one URL,
  branch and folder, but its origin (`Offer::origin`) says only the first
  two, so one repository gathered at one branch from two folders that each
  hold a `deploy` offers it twice under one origin: `gather list` prints
  the same line for each, `--from` names neither, and the install screen's
  line for either box is refused as offered twice, where the screen, when it
  installed what it marked itself, installed the one ticked. What it then
  recorded named the source by id, but the origin it recorded named both,
  which is what a record's offer is found by (`install::offered_for`). An
  origin that says a folder other than the kind's own would name each, and
  would change what a record holds, so it needs a ledger step that writes
  the folder into the origins older records keep.
- **Changes to an artifact take no lock.** `add`, `edit`, `rename` and
  `remove` write into a project or the workspace's own folders and then see
  to a record ([A record must not outlive the copy it is
  about](#a-record-must-not-outlive-the-copy-it-is-about)), and neither step
  holds the workspace: an edit is made in an editor, for as long as somebody
  keeps it open, and holding the lock that long would keep every other writer
  waiting. An install that takes the lock after the change finds the copy
  edited, or somebody's, and leaves it. One already past that look when the
  change lands writes over it: an edit is then lost, and the record a
  removal drops or a rename carries is that install's, which leaves its copy
  unrecorded or the renamed one recorded as the install wrote it. Taking the
  lock for the change itself, where there is no editor to wait for, would
  close it for `add`, `rename` and `remove`.
- **Two first writers making one workspace.** `init` makes a marker with
  `create_new` and then fills it, so a second writer making the same
  workspace at that moment (two first commands in a home with no
  `.mindflayer` yet) finds a marker it cannot read, or fails to create one.
  Writing the marker whole under a name of its own and linking it into place
  would let the second find it whole.
- **Examples outside a clone.** The examples are a folder in the repository,
  so only somebody with a clone has them to load; a binary that arrives some
  other way, as a release would, has none. Embedding them in the binary, or
  copying them into a workspace on `flayer init`, would reach everybody, at
  the price of a second way into a workspace beside loading, and of files a
  workspace holds without having asked. #134 left both out, with gathering
  paused; a gathered or published copy of `examples/` would answer it
  without either.
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
- **Seeing to a record from elsewhere.** A change sees to the record of the
  workspace its command resolved, wherever that sits, but not of any other
  workspace that links the same project: nothing in the project says which
  workspaces link it, so each of them keeps a stale record of a copy
  removed, or of one renamed under its old name. Every workspace linking a
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
  uninstall -p` (not with `--all-projects`, which resolves as a read), and
  `unlink` at either level leave a `~/.mindflayer` behind and then refuse,
  since a workspace just made manages nothing. `flayer edit`,
  `rename` and `remove` make it to change its own artifacts, `remove`'s dry
  run included. Each could resolve with `Intent::Read` first and make it only
  when it is about to write; whether each should is a choice per command, and
  they were left as writes when the reads stopped making it.
- **A workspace marker older than its directories.** A `flayer.toml` written
  before workspaces kept artifacts of their own names no folders, so it now
  reads them from the library. One that had artifacts added by a build in
  between, which put them in `skills` and `rules` at its root, stops listing
  them: they stay where they are, untouched, until `[directories]` names
  them. `flayer migrate --library` moves only what a marker names, so it plans
  nothing for such a workspace. It could look for those folders wherever a
  marker names none, leaving them to a project at the root that keeps a kind
  there.
- **An unlinked project at the root.** `own_kinds` counts what a project at
  the workspace's root shares with it as the workspace's own unless the
  workspace links that project; a migration leaves it to the project linked
  or not, so the workspace stops offering it and an install of it keeps a
  record nothing offers. Counting the root as a holder in `own_kinds` too
  would make the two agree, at the price of a home's skills dropping out of
  `flayer list` and the install screen until the home is linked; moving them
  would take a project's artifacts, which a migration never does.
- **Links inside a moved skill.** A skill holding a link aimed by a relative
  route out of itself, `scripts` to `../../shared/scripts` say, points
  elsewhere once a migration has moved the skill to another depth. The plan
  refuses an artifact that is itself such a link, but does not look inside
  one. Re-aiming every such link would let a migration through where it now
  refuses, at the price of rewriting links somebody made.
- **Migrating from a screen.** The TUI has no key for `flayer migrate
  --library`, nor for moving a project to its settings, which run only from
  the command line; its header names the second's command where a project
  needs it. A key producing a migration's two lines, the dry run and then
  `--yes`, would bring each to the TUI the way every other change gets there.
- **A project's move stopped by a crash.** A migration that fails puts back
  what it had done, but one whose process is killed part way puts nothing
  back: copies it placed are recorded nowhere, and read as somebody's, and
  copies it moved into its stage go with the next install's sweep while the
  `mind.toml` may still name them. Writing down each step before it is
  taken, for the next run to undo or finish, would close it, at the price
  of a journal in `.mind`.
- **An edited install left in the folder a project authors in.** Moving a
  project to settings leaves a copy edited since it was installed where it
  is, and drops its record, as a change of agents does. In a project marked
  by its settings, that folder is read by no agent unless it is one of
  theirs, so the edit stops reaching any until somebody moves it where one
  reads, or installs the skill again. Offering to move it into the agents'
  folders would keep the edit reaching them, at the price of a copy
  Mindflayer would then write without a record.
- **Gathering a repository that is a workspace.** A team repository made by
  `flayer init` and `flayer add` keeps its own skills in
  `.mindflayer/library/skills`, where a workspace made before the library kept
  them in `skills`, the folder `flayer gather` harvests by default. So a plain
  `flayer gather git <url>` of it, which used to work, now says there is no
  `skills` folder, and `--path .mindflayer/library/skills` is needed. Gather
  could read a cloned workspace's `flayer.toml` for where its own are when no
  `--path` is given, as `load` reads a loaded one's, or look in
  `.mindflayer/library/skills` among the layouts #101 harvests; whether to
  trust a marker that came from a source is part of that choice.
- **Directories edited into a marker by hand.** Where a holder's directories
  may be (inside it and, for a workspace, clear of the shelf and the cache) is
  checked when `init` writes the marker, not each time it is read, so a marker
  edited afterwards is read as written. Checking on every open would refuse
  workspaces configured on purpose: one keeping its own skills at its root
  (`skills = "."`), from which discovery reads nothing in `.mindflayer`, holds
  the shelf by the same rule. Whether an open should hold a marker to those
  rules, and what a refusal then says, is still to choose.
- **Two workspaces changing one project's settings.** A change to a
  project's agents or share mode holds the lock of the workspace that links
  it (#37), which keeps out that workspace's installs and changes, but not a
  second workspace linking the same project: two changes at once from two
  workspaces each read the settings, and the one renamed last keeps only its
  own, each carrying its own installs along by its own ledger. A lock in
  `.mind`, taken by every change to the settings beside the workspace's, would
  keep the settings whole, at the price of a file a shared project would have
  to ignore, and still not the installs, which each workspace records apart:
  the wall disowning from elsewhere meets too.
- **A leftover older marker nobody is told about.** A project holding more
  than one marker is read from the newest, and `MindProject::warnings` names
  each one ignored, but no front end prints it yet. This build leaves none:
  `mind init` gives a project an older marker marks no settings, and the
  migration moves the old marker out of the way. One is still left by hand,
  by a copy, and by an older build: one from before #29 asks only whether a
  `mind.toml` is there, and one from before #166 whether a `settings.json`
  or a `mind.toml` is, so its `mind init`, or its TUI's making a repository a
  project, writes its own beside a `settings.toml` and reads the project
  from it from then on, while this build reads the `settings.toml`. With
  `mind init` making a `settings.toml` by default, two builds on one machine
  are enough. A `warning:` line wherever a project is opened, or a doctor
  check (#96), would make it seen.
- **`targets` written by hand into a version 3 file.** `targets` came with
  version 4 of a `settings.json`, which a build from before it refuses, and a
  file that set them through Mindflayer was written at 4 ([a project's own
  targets](#a-projects-own-targets)). One a person gives `targets` while
  leaving it at 3 is read with them by this build and read past by an older
  one, which installs into the standard folders and says nothing, until `mind
  migrate` moves it to a `settings.toml`, which no such build reads. Refusing
  such a file as malformed would close it, at the price of a project no build
  opens over a version number; a warning naming the version it needs is the
  other way. A version 3 file whose `targets` do not read as targets at all
  (not an object, a kind with no folder or something other than a list of
  them) is read past, as every build before them read the key, rather than
  refused: a project those builds opened still opens, and installs where its
  agents read without a word, where a version 4 file holding the same is
  refused as any key the format defines is. The move to a `settings.toml`
  writes what this build read, no `targets`, and the copy it keeps holds the
  key as it was.
- **A key a settings.json holds that this build does not know.** Its move
  to a `settings.toml` writes what this build reads, the keys the format
  defines and an unknown one inside `directories` or `targets`, and leaves a
  key it does not know at the top of the file, a `$schema` say, in the copy
  kept in `.mind/tmp` alone, as #38's move leaves a `mind.toml`'s. Nothing
  wrote one but a person, and none means anything to Mindflayer, so it is
  not said. Carrying each into the TOML is the other way, at the price of a
  JSON `null`, which TOML cannot hold, refusing the move.
- **An agent pointed at a target some other way.** Whether an agent reads a
  project's own target is told by where docs/agents.md says it reads, so one
  pointed there by a link from a folder it reads is seen, and one pointed
  there by a setting of its own, an OpenCode configuration naming another
  skills folder say, is warned of as reading none, with a non-zero `status`
  for good. Reading each agent's configuration where it says where it reads
  (#44, #96 read those files for other reasons) would tell the two apart.
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
- **A prompt rebuilt every time.** Starship, powerlevel10k, oh-my-posh and
  the like write the prompt anew before each one, which drops the prefix
  activating puts in front of it: the workspace is still active, and `flayer
  which` still says so, but the prompt does not. A
  `$MINDFLAYER_DISABLE_PROMPT`, as a virtualenv has
  `$VIRTUAL_ENV_DISABLE_PROMPT`, and a word on showing `$MINDFLAYER_WORKSPACE`
  from the prompt's own configuration would serve them.
- **A wrapper a virtualenv throws away.** In fish and PowerShell, a
  workspace activated on top of a virtualenv wraps the virtualenv's prompt,
  and the virtualenv's `deactivate` puts back the prompt it saved, without
  the wrapper. The workspace stays active without its prefix, and since the
  prompt is wrapped once per shell, a later activation in that shell shows
  none either. Nothing breaks, and the prefix is missing. Wrapping again
  whenever the wrapper is no longer reachable would mend it, given a way to
  tell that without breaking whatever wrapped it since.
- **Shells elsewhere.** The tests that run a shell run on Linux only. macOS
  has bash 3.2 and zsh, and Windows has PowerShell, and each wants running
  there before activation is said to work on it.
- **Worktrees share one exclude file.** Every linked worktree of a repository
  reads the same `info/exclude`, and an entry is anchored at whichever work
  tree reads it. An install keys a project's group by its id, so a project
  checked out in two worktrees with one settings file, committed in shared
  mode or copied in local mode, has one group for both, and uninstalling in
  one stops excluding the copy the other still has; `mind share shared` in
  one forgets the group for both. A shared project writes no entries, so
  only a copied local `.mind` meets it. A key that names the checkout rather
  than the project would keep them apart.
- **A project that moves.** Exclude entries are anchored at the route the
  project had when they were written. Moved within its repository, a project
  keeps a group of lines pointing where it was, which removing its entries by
  their new route never matches. Rewriting a project's whole group from what
  the ledger says it has installed would heal it.
- **A copy kept out of git through a link.** An exclude entry names a copy
  by the route its record names, so an uninstall takes out the line it
  finds, and git does not follow a link: a copy in `.skills`, recorded as
  `.agents/skills/<name>` because the agents read it through links there, is
  kept out of git by a line that matches nothing, and shows as untracked.
  An install into a linked folder writes such a line too, and so does a
  change that respells a kept copy's record. Writing the entry by the folder
  on disk would keep it out, at the price of a line an uninstall has to work
  out again from the filesystem rather than from the record. Claude's rule
  files meet it too: a link inside the project on the way to one, `.claude`
  linked to `config/claude`, is followed, while the entry names
  `.claude/rules/mindflayer/<route>.md`, so in a local project the file
  shows as untracked. Their entries are spelled from the route, as an
  install's are from the record, and one answer should serve both.
- **Agents changed by hand.** `mind agents` carries what is installed along
  (#37), and `mind targets` (#40), but the settings are TOML anybody can edit,
  and the comments in them say how. An install after such an edit replaces its
  record's targets with the ones the project has now, so a copy in the folder
  of an agent the project stopped naming, or one its `targets` stopped naming,
  is no longer recorded: Foreign, left where it is with its exclude entry.
  `mind agents` naming the agents the file already names, or `mind targets
  set` naming the folders it names, re-applies the installs, and takes such
  copies out, only while the install has not dropped them from its record
  first. An install that carried the installation's other copies along itself,
  as `reapply` does, would close it.
- **An install surveyed before a change.** An install works out its targets
  from the project it is handed, read before it takes the workspace's lock,
  as it does its standing, which it looks at again under the lock. One
  surveyed before `mind agents` or `mind targets` changed the project's
  settings, and applied after, puts a copy where the settings no longer send
  one; recorded, so the next `mind agents` with the same agents takes it
  out. Reading the settings again under the lock would close that.
- **A copy as installed, to make another from.** A change of agents gives a
  new folder a copy of one the record names that still holds what was
  installed, by its hash. An installation whose every copy was edited, or
  was recorded without a hash, has none, so the new agent gets nothing and
  that is said; installing it again is refused over the edited copy, and
  forced it puts every copy back, the edited one written over. Copying from
  the offer instead would hand the agents two revisions of one skill
  whenever the offer has moved on.
- **Two workspaces, one exclude file.** The workspace's lock keeps one
  workspace's writers apart, and an exclude edit is made under it, but
  projects of one repository linked from two workspaces edit `info/exclude`
  under two locks. The rename keeps the file whole, and one of two edits at
  the same moment can still drop the other's line. A lock of the exclude
  file's own, beside it in the common dir, would close that.
- **A team's `.gitignore` outranks the block.** Git ranks every `.gitignore`
  in the tree above `info/exclude`, so a negation there (`!.claude/skills/**`)
  takes back what the block excludes, and a local install shows up in `git
  status` with nothing said: the entry is written all the same, and only
  `Excluded::Outside` warns. Asking gix's `excludes` whether each path is
  ignored once it is written, as the tests do, would let the installing side
  or a doctor check say so.
- **Ctrl-C during `flayer gather`.** The watch's flag is there to be set, but
  the CLI installs no signal handler, so Ctrl-C ends the process wherever the
  gather has got to. Ended before it places anything, it costs the clone and
  can leave registered a source nothing came from; ended while it places, it
  can leave the shelf between two revisions, and the ledger behind it. A
  handler that cancels the watch would make the command line stop where the
  TUI's `esc` does.
- **Gathering rules.** Only skills are gatherable. A rule is a loose file at any
  depth, so harvesting one means deciding what its name is relative to: the
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
  too would close it. In the TUI either prompt is worse than at the command
  line: a gather runs apart while the loop goes on reading the terminal and
  drops every key but `esc` and Ctrl-C, so the loop races the prompt for its
  answer; and neither key stops a gather waiting on one, since `gix` takes a
  cancel only once it is receiving. `BatchMode=yes` would close the TUI's ssh
  case too.
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
  programs that follow the XDG base directories do, is unverified: #17 ran
  only Claude Code, and [docs/agents.md](agents.md) marks it docs-only, the
  docs being silent. If a check on the binary finds that it does, that
  variable becomes the step before the default for `--opencode-config-dir`,
  and tests that spawn a binary clear it too.
- **`$OPENCODE_CONFIG_DIR` beside OpenCode's folder, not instead of it.**
  `--opencode-config-dir` takes the variable in place of `~/.config/opencode`,
  as `--codex-home` takes `$CODEX_HOME` in place of `~/.codex`. OpenCode's
  documentation says otherwise: the folder the variable names is loaded after
  the global one and the project's `.opencode`, so it can override their
  settings, and the global folder is still read. #17 did not run OpenCode, so
  that stays docs-only. If a check on the real binary confirms it, a check
  that reads only one folder misses what OpenCode reads from the other
  whenever the variable is set, and `AgentHomes` needs both for OpenCode:
  `~/.config/opencode`, and the variable's folder on top.
- **An empty agent variable.** An empty `$CLAUDE_CONFIG_DIR`, `$CODEX_HOME` or
  `$OPENCODE_CONFIG_DIR` means no folder at all here, following
  `$MINDFLAYER_HOME`. An agent that reads an empty one as unset uses its
  default instead, and a check would then leave out a folder that agent does
  read. #17 did not settle it: it ran only Claude Code and did not try an
  empty `$CLAUDE_CONFIG_DIR`, so how each agent reads one is for an addition
  to `scripts/agent-probe` to record.
- **A `CLAUDE.md` at `/`.** #44's walk stops below `/`, as Claude's own check
  most likely does ([docs/agents.md](agents.md)): #17 inferred it from the
  debug log's directory counts and could not put a file at `/` to try it.
  Were Claude to count `/` after all, a `CLAUDE.md` there with none nearer
  would have the predicate say Claude reads the block while it reads none,
  and the rules about every file would miss Claude. A probe run allowed to
  write at `/`, in a container say, would settle it.
- **An import from `.claude/CLAUDE.md`.** The import goes there, as
  `@../AGENTS.md`, only when the project's `CLAUDE.md` is not there or may
  not be edited. That Claude resolves an import from the file holding it,
  and reads both `CLAUDE.md` and `.claude/CLAUDE.md` in one folder, is
  docs-only ([docs/agents.md](agents.md)); were either wrong, the block
  would miss Claude in that one case. A `claude-import` run with the line in
  `.claude/CLAUDE.md` would settle both.
- **Claude started below a project's root.** The predicate looks from the
  project's root up, for Claude started there. A `CLAUDE.md` in a folder
  between the root and a deeper start keeps Claude from reading `AGENTS.md`
  natively there, while the reach says it does, and no import is added.
  Looking below the root too would tell, at the price of walking the whole
  project, and of an import for a start nobody may use.
- **A `CLAUDE.md` hard linked to `AGENTS.md`.** One file under two names is
  told by resolving both paths, which sees a symbolic link and not a hard
  one, and the block's render replaces `AGENTS.md` through a rename, which
  splits a hard link. What follows depends on when the link was made. One
  made before any block was written is split by the first render: the two
  are then files of their own, the import goes into `CLAUDE.md` as into any
  other, and Claude reads the team's text twice, once in each, though each
  rule once. One made once `AGENTS.md` holds the block, the likelier order,
  since the block is there whenever a team links the two, gives a
  `CLAUDE.md` holding the block's rule markers, which `ClaudeMd::parse`
  refuses (`Malformed::Ruled`), as it refuses any copy of `AGENTS.md`: every
  reach and every render of the import is refused, `render(false)` too,
  until somebody edits that file or takes the link away, and a block render
  that changes the rules meanwhile splits the link and leaves `CLAUDE.md`
  the copy with the rules as they were. Telling a hard link by the file's
  device and inode alone would not end it, since such a render splits the
  link before the import is decided. Writing `AGENTS.md` in place rather
  than by a rename, where it has more than one link, would keep them one
  file for that to tell as `Reach::Same`, at the price of a write that is
  no longer all or nothing.
- **An import left in a file git tracks.** An import no longer wanted in a
  file that may no longer be edited, a `CLAUDE.md` committed with it in a
  project turned local, stays and is said at every render
  (`claude_md::Warning::Tracked`), until `edit_tracked` or shared mode lets
  it be taken out. Once the last rule is gone it imports an `AGENTS.md`
  holding no block, which gives Claude nothing more. Beside a `CLAUDE.md`
  that is the `AGENTS.md` through a link, Claude reads the block through
  both, and whether it loads the one file twice is not recorded.
- **Codex and OpenCode on their binaries.** #17 ran only Claude Code, so
  every Codex and OpenCode fact in [docs/agents.md](agents.md) is docs-only,
  each with a fallback. Its checks 2 (OpenCode given the same skill in both
  folders) and 5 (Codex and project trust), and the Codex and OpenCode halves
  of 1 and 4, are what a `codex.sh` and an `opencode.sh` beside the probe
  would run. #34 relies on them already, writing `.agents/skills` for both
  and trusting OpenCode with identical copies in two folders, and #42 will.
  The [target table](#agents-and-where-each-kind-reaches-them)'s
  `.agents/skills` for Codex and OpenCode rests on them too; were one to read
  elsewhere, its arm in `standard` is the one line to change.
- **Folders an agent reads and is sent nothing in.** A project marked by its
  settings is read in its agents' targets and in its authored folder, not in
  every folder they read ([read where its agents
  read](#a-project-marked-by-its-settings-is-read-where-its-agents-read)):
  `.opencode/skills`, and `.claude/skills` in a project that names OpenCode
  and not Claude, are read only when the project authors there, though
  OpenCode loads what is in them. The targets are where an install lands and
  can meet somebody's work, so those are what a listing has to know; reading
  every folder `Agent::reads` names would list the rest as well, from folders
  no install of Mindflayer's reaches. Doctor (#96) is the other place a skill
  only there could be reported. An install refuses on the targets alone too,
  so in a project that names OpenCode and not Claude it writes
  `.agents/skills/<name>` beside somebody's `.claude/skills/<name>` or
  `.opencode/skills/<name>`, and OpenCode loads two versions of one skill.
  Refusing on every folder an agent reads would close that, at the price of
  refusing over a copy no listing shows; reading those folders for both is
  the other way, and whichever is chosen, a listing and an install should
  agree on it.
- **Listing reads every copy it compares.** Copies of one name in a v3
  project's agents' folders are compared on every listing by `hash::tree`,
  which reads each of their files whole: with a copy installed in each
  target, that is every installed skill, on every `list`, `show`,
  `validate`, `status` and refresh of the TUI. A skill is a few small files,
  so it is accepted for now. The cheaper ways each miss something:
  `copy::same` stops at the first difference but follows links, which a copy
  here may hold to anything; comparing sizes first saves nothing when the
  copies are alike, which after an install they are; and the hash an
  installation records is the ledger's, which discovery does not open, and
  a foreign copy has none. If a skill's assets make a listing slow, that
  recorded hash, compared where `Catalog::with_records` already reads the
  ledger, is where to start. `status`, and so the TUI's rows, reads more:
  each installation's standing (`install::standings`) hashes every copy its
  record names again, and compares one of them with the offer it came from,
  reading both, so that an outdated install can be told from a current one,
  as `status` and an update both say. Telling an edit alone would need no
  offer but for a
  record with no hash; the price was taken so that a report and a refusal
  are told by one function.
- **A screen that lists what it cannot change.** The TUI's project rows
  come from the same discovery, so a v3 project's rows list the skills in its
  agents' folders as well. A row is one name, so a change aimed at it reaches
  the project's own copy when it authors one, and is refused (`NotAuthored`)
  when the name sits only in its agents' folders. An own copy that does not
  read takes the row its name has rather than a second one, whose keys would
  reach it all the same. Each row says its state, so one that is installed
  or foreign reads as such before a key is pressed, but the keys that change
  an artifact are still offered on it and refused only once pressed; leaving
  them out of the footer on such a row would say so sooner.
- **An install gone under a name somebody keeps.** When every copy of an
  install has gone and the project holds something else by that name, status
  says the name as what is there, and nothing of the record, which lingers:
  the next install of it is refused as foreign when what is there sits where
  it would write, and `flayer uninstall` forgets the record without touching
  what is there. While a copy of the install is left, the entry holds it and
  says missing, even over the project's own copy. A second entry of the same
  name would say it, at the price of the one name a row the screen and every
  listing keep to.
- **Errors outside the envelope.** A command that fails says why as text on
  stderr, `--json` or not, and prints no envelope, so a script tells failure
  only by the exit status. #61 gives errors codes, and is where an error
  joins the envelope, if it does.
- **An agent's CLI writing the same config file.** The workspace's lock keeps
  Mindflayer's writers apart, not the agents': `claude mcp add` rewriting
  `.mcp.json` between `ConfigFile::edit`'s read and its rename is undone by the
  rename. Reading the file again just before the rename would narrow that to
  the rename itself; nothing closes it while the agents take no lock of ours.
- **A folder made a link while a config file is written.** `ConfigFile::edit`
  looks for links on the route again just before it writes, and the rename is
  onto the path as spelled, so a link that the file itself has become is
  replaced, not written through. A folder on the route that becomes a link in
  the moment between that look and the rename is still followed by the
  operating system. Only opening each folder without following links and
  writing relative to it (`openat` with `O_NOFOLLOW`, then `renameat`) closes
  that, which is a platform layer of its own for a window that small.
- **What Codex reads from below a project's root.** The override and the
  budget are worked out for Codex started at the project's root, where its
  `AGENTS.md` is. Started in a folder below, Codex reads that folder's
  `AGENTS.md` files too, after the project's, and its config can rename the
  files it looks for (`project_doc_fallback_filenames`), say what marks the
  root (`project_root_markers`) or change the budget, none of which
  Mindflayer reads yet. Reading Codex's config, as a doctor check would
  (#96), would make each warning exact for one more case.
- **`.claude/rules` above where Claude starts.** Claude reads every
  `AGENTS.md` and `CLAUDE.md` from where it starts up the tree, but whether
  it reads a `.claude/rules` in a folder above, as it reads `.claude/rules`
  at any depth below, is in neither [docs/agents.md](agents.md) nor the
  probe. Claude's rule files are refused above a project the workspace links
  as the block is, which at worst keeps rules from a project above others
  that Claude would not have read in them anyway. A probe check with a rule
  one folder above the start directory would settle it, and let that one
  refusal go if Claude reads no such folder.
- **A project that authors rules in `.claude/rules`.** A project may keep
  its own rules where Claude reads them, and its listing then reads
  `.claude/rules/mindflayer` as its own rules, named `mindflayer/<route>`,
  which a lifecycle change can rename or remove. Nothing is lost, since a
  render writes a file it recorded again where it finds none, and never
  writes over one it did not record; but the listing shows Mindflayer's
  files as the project's. The ledger records them now ([installing
  rules](#installing-rules)), so a listing could tell them apart as it
  tells installed copies from authored ones; leaving out the `mindflayer`
  folder where a project authors in `.claude/rules` would keep the two
  apart, and the status entry of each rule its one.
- **A file replaced by a render stopped part way.** A render records each
  file it may make before it makes it ([installing
  rules](#installing-rules)), so one killed part way leaves no made file
  unrecorded. A file it replaced, a rule changed since the last render, is
  recorded by its new hash only once the render is done: killed between,
  the file reads as edited, and is left, until a render finds it holding
  its rule as written again. Recording both hashes for the moment between
  would close that, at the price of a target that holds two.
- **A rule whose offer has gone.** Every render an install runs gives each
  rule from the offer it was installed from, so one no longer offered there
  (deleted from the library, a load gone) refuses every install of another
  rule into the project until it is put back or uninstalled ([installing
  rules](#installing-rules)). An update's render, and an uninstall's, keep
  it instead, as the project holds it, which reads as orphaned
  (`Standing::Orphaned`) and is said ([bringing what was installed up to
  date](#bringing-what-was-installed-up-to-date)); an install's render
  doing the same would let the rest change there too, at the price of an
  install that quietly carries a rule no offer stands behind.
- **Every render brings every rule up to date.** A render gives each rule
  as its offer holds it now, so installing one rule updates the sections of
  the others too, where installing a skill touches no other skill. The
  block is one file written whole, and rendering only one rule would mean
  reading the others back from it, as a render now reads back a section
  somebody edited, and keeps it. #48 chose to keep that for an update: one
  rule named brings the project's other outdated or missing rules along,
  and its plan names each, so what is written is what is said. An install
  of a rule still brings them along unsaid, and keeps only the copy of a
  rule somebody edited, writing its other copy from the offer, where an
  update keeps every copy of it. Holding an install's render to the same
  would spare its agents two versions of one rule too.
- **A copy recorded where nothing reads it.** Status calls an installation
  missing when discovery does not find one of its copies, and an update
  writes it when its standing says a copy is gone. A copy that is still
  there holding what was written, but where discovery no longer reads it (a
  folder dropped from the project's settings by hand, which `mind agents`
  would have carried along), reads as missing in status and as current to
  an update, which writes nothing. Telling such a copy as missing in the
  standing too would make the update rewrite every target, as an install
  does, and record the copies afresh.
- **A `git clean -dX` takes a local project's settings.** A local project
  keeps its whole `.mind` out of git (`MIND_IGNORE`), so the `-dX` that
  takes its copies, each in a folder `git clean -X` does not go into, takes
  its settings with them, and the project is no project any more: an
  update finds nothing to put back, and `flayer update --all` says the
  workspace's entry would not open. `mind init` again, with the agents,
  share mode and folders it had, marks it once more, and the next update
  puts every copy back from the ledger, which knows the project by its
  route; the new settings come with a new id, so the copies are kept out
  of git under a new group, and the old id's group is left in the exclude
  file naming the same places, which nothing takes out. Keeping the
  settings out of the ignored folder would show them in `git status`,
  which is what a local project is for avoiding; keeping a copy of them in
  the workspace, beside its entry, would let a command mark the project
  again as it was, its id included.
- **Rules after a change of agents or share mode.** `reapply` carries a
  project's skills along when its agents, targets or share mode change, but
  not its rules: those are rendered again by the next install or uninstall
  of one, or by `install::rules::render`, which nothing calls on a change
  yet. Until then a project that stops naming Claude keeps Claude's files,
  and one turning local leaves an `AGENTS.md` Mindflayer made in git's
  view. Rendering the rules again at the end of `reapply::apply`, under the
  lock it holds, would close it, with what the render says in what the
  change says.
- **Claude's reach in a report.** Status says Claude reads a rule through
  the block when the render gave it no file of its own, as the ledger
  records it, rather than asking `claude_md` again, which needs the agents'
  own folders a report is not handed. A file for Claude that was somebody's
  when the render ran is the one case that reads wrong: no file is
  recorded, and Claude reads neither. Handing a report the agents' folders
  would let it ask.
- **A link made while `AGENTS.md` is written.** A link in place of the file
  is looked at before the read, and one leading out of the project is
  refused; a link made in the moment between that look and the rename is
  followed by `replace_bytes`, which resolves the path first. Writing to the
  path as spelled, as `ConfigFile` does, would close it, at the price of
  replacing the link to a file inside the project that some teams keep
  between `AGENTS.md` and `CLAUDE.md`.
- **A config file left empty by its last entry.** Removing the last server
  leaves `"mcpServers": {}` in a JSON file, and a `.codex/config.toml` that
  Mindflayer made holding nothing, and the file stays, with any folder made for
  it. Removing them needs a record that Mindflayer made them, which the
  ledger's MCP rows (#93) could carry.
- **A panel built after the server.** A release build embeds what
  `apps/web/dist` held when `mindflayer-server` last compiled, and a debug
  build reads the folder at run time only if it existed then, because
  rust-embed resolves the folder's real path when it compiles. Neither sees a
  panel built later until the crate compiles again. A build script that reruns
  when the folder changes would, at the price of rerunning on every build
  while the folder is missing, since cargo counts a missing path as changed.
  #68, which puts the panel into the binaries, settles the build order.
- **The palette in two places.** `src/theme.ts` names the palette for Nuxt
  UI, and `main.css` spells it out as the variables Nuxt UI's colours plugin
  would have written at run time. The build test holds the two to each
  other. A Vite plugin writing those variables into the stylesheet at build
  time, from `src/theme.ts`, would leave the palette in one place.
- **A light panel starts dark.** `index.html` carries `dark`, so the default
  never flashes light, but a browser that chose light sees one dark frame
  before the script applies the choice. A script in the page that read it
  first is what the CSP refuses. The choice is in a cookie the server
  receives with the page's request, but acting on it would need the server
  to rewrite the page as it sends it.
- **The panel's transcript ends with the server.** The TUI prints its
  transcript when it closes; the server keeps the panel's in memory and drops
  it when it stops. Whether `flayer ui` prints it on the way out, as the TUI
  does, is for #68 to choose.
- **A style nonce for the panel.** The policy allows no inline style, so a
  `<style>` element that a library writes into the page at run time is
  blocked, as theming and colour-mode helpers often do; style the panel's
  scripts set through the CSSOM, as Vue's bindings do, is not governed by
  it. The panel needs no nonce so far: Nuxt UI's palette is declared in the
  stylesheet and the colour mode switches by class ([Nothing the CSP
  refuses](#nothing-the-csp-refuses)). The one library left that writes a
  `<style>` is vaul-vue, under Nuxt UI's `UDrawer`, when it loads, with the
  nonce of a `meta[property=csp-nonce]` if the page has one; nothing loads
  it yet, since the sidebar opens over a narrow screen as a slideover. A view
  that wants a drawer needs vaul's styles in the stylesheet, or a nonce: the
  server would issue one with each launch, add it to `style-src`, and write
  it into `index.html` where Vite's `html.cspNonce` leaves its placeholder.
  `script-src` never gets `'unsafe-inline'` either way.
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
- **Folders a workspace scans.** `flayer scan DIR` and `flayer minds DIR`
  look where they are told for one command, or one session of the screen;
  nothing keeps the folders, so the TUI's `m` and link form opened another
  time look under the workspace root again, and a default workspace in home
  offers the whole home, three levels down. Keeping them in `flayer.toml`
  would be a format change, and whether it belongs with ranked suggestions
  (#58) is open. Nor does the screen take a depth: `flayer minds` looks
  `DEFAULT_DEPTH` levels down, and a project further down is linked by its
  path.
- **The shelf in other reports.** `flayer validate` reads the shelf; `flayer
  list` and `show` do not, because `gather list` is the shelf's listing. Seeing
  a gathered skill's contents before installing it means reading its folder.
- **An em dash in a commit message.** `make lint/em-dash` reads the tree, so a
  commit message is checked by nobody but its author ([No em dash in what the
  repository says](#no-em-dash-in-what-the-repository-says)). A step in CI
  over a pull request's own commits would catch one before the merge without
  tripping on the history from before the rule.
- **Remembering on activate.** #56 asked `flayer activate` to remember the
  workspace it activates as well as `flayer init`; it remembers none, since
  activating writes nothing ([the workspaces this machine
  knows](#the-workspaces-this-machine-knows)), and `flayer init` run again on
  a workspace remembers it. Remembering there would want a write of its own
  beside the code it prints and never on stdout, taking the default
  workspace's lock without waiting, so a busy one is passed over rather than
  waited for at every new shell, and saying a failure only as a hint, so a
  shell's startup file is neither slowed nor made noisy; and the rule that
  activating writes nothing would have to say why it no longer holds.
- **A remembered workspace that moves.** The list names a workspace by where
  it was, so one moved with `mv` is listed as one that cannot be opened until
  it is forgotten and `flayer init` run where it went. Nothing in a workspace
  names it for good, as a project's id names a project (#57 relinks those), so
  there is nothing to match a moved one by.
