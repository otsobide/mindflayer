# 🧠 Mindflayer

Manage what your coding agents read, the way git manages code: a project
carries a `.mind` directory holding its **skills** and **rules**, and a
workspace above it carries a `.mindflayer` that orchestrates several such
projects at once.

One shared Rust engine (`mindflayer-core`) behind every front end. The first
front end is the CLI — `mind` for a project, `flayer` for a workspace — and the
layout leaves room for a desktop app later without moving the engine.

> **MVP scope.** Mindflayer is being built incrementally. Today it creates the
> two kinds of directory, registers projects with a workspace, gathers skills
> from git repositories onto the workspace's shelf, installs them into the
> projects it manages, and reads the skills inside those projects. Creating a
> skill from scratch is the next step rather than a shipped feature.

## The two levels

| | Marker | Holds | Think of it as |
|---|---|---|---|
| **Mind project** | `.mind/mind.toml` | its artifacts, in the directories the marker names | a repository's `.git` |
| **Flayer workspace** | `.mindflayer/flayer.toml` | references to mind projects | the directory your repos sit in |

A mind project is meant to be committed: the marker and the artifacts travel
with the code they describe, so whoever clones the repository gets its skills
and rules. A flayer workspace is the level above, where several projects are
managed together.

## The two kinds

| | Lives in | Shape | Declares |
|---|---|---|---|
| **Skill** | `skills/<name>/SKILL.md` | a directory each, so it can carry scripts and references beside its instructions | front matter: `name`, `description`, optional `allowed-tools` and `license` |
| **Rule** | `rules/<name>.md` | one markdown file each | nothing — it is context, and its name is its filename |

Folders under `rules/` group and mean nothing else, so
`rules/git/no-force-push.md` is the rule `git/no-force-push`. Skills are flat,
because a skill's directory already belongs to it.

Those are the defaults, and a project can say otherwise — see [where a project
keeps its artifacts](#where-a-project-keeps-its-artifacts).

Both are found the way git finds a repository: by walking up from wherever you
are until the marker file appears.

## Getting started

From a fresh clone, one command builds both binaries and links them:

```bash
make dev/link                  # into ~/.cargo/bin, pointing at target/debug
```

They are **symlinks**, not copies, so every later `make build` updates the
binaries you are running without reinstalling anything. `make dev/unlink` takes
them back off. If you only want to use the tools rather than work on them,
`make install` puts real copies in the same place.

That place is `~/.cargo/bin`, which is on your PATH if `rustup` put it there
and is **not** if your Rust came from a system package. `make dev/link` says so
when it is missing; `BINDIR` is how you point somewhere else, and
[Development](#development) has the rest.

Then:

```bash
cd ~/Projects/collapse
mind init                      # a .mind marker, and empty skills/ and rules/

cd ~/Projects
flayer init                    # a .mindflayer, to manage several of them
flayer link collapse           # tell it which projects those are
flayer list                    # every skill across all of them
```

`init` never overwrites an existing marker. Run it twice and it says so and
changes nothing, so it is safe in a script.

## Commands

The surface is split the way the model is. `mind` acts on the project you are
standing in; `flayer` acts on the workspace above it.

```bash
mind init [--skills DIR]   # create a .mind here, saying where its artifacts go
mind list [KIND]           # this project's artifacts; `mind list rules` filters
mind show <NAME>           # one artifact; `rule:deploy` when a name is ambiguous
mind validate [KIND|NAME]  # check a kind, one artifact, or everything

flayer init                # create a .mindflayer here
flayer link <path>         # register a mind project with it
flayer unlink <path>       # drop one
flayer list [KIND]         # every artifact across every registered project
flayer show <NAME>
flayer validate [KIND|NAME]

flayer gather git <URL>    # collect skills from a repository onto the shelf
flayer gather list         # what is on the shelf, and where each came from
flayer install             # a screen for putting shelf skills into projects

mind flayer <cmd>          # the long way round; `flayer <cmd>` is the shortcut
mind ls / flayer ls        # alias for list
mind -C <dir> ...          # work in <dir> instead of the current directory
```

Both find what they act on the way git does, by walking up from wherever you
are. Both levels answer a different question, and that is the point:

```
$ cd ~/Projects/collapse && mind list      # just this project
skill  commit-style       How this repo writes commit messages
rule   git/no-force-push  Never force-push a shared branch

$ mind list rules                          # narrowed to one kind
git/no-force-push  Never force-push a shared branch

$ flayer list                              # every project it manages
collapse  skill  commit-style       How this repo writes commit messages
collapse  rule   git/no-force-push  Never force-push a shared branch
tanukeys  skill  ddd-reviewer       Review a change against the DDD rules
```

A column appears only when it tells you something. The kind column is absent
when only one kind is in play, the project column when only one project is —
the same rule in both cases.

A name is bare until it needs qualifying. When one name belongs to two kinds,
`mind show deploy` shows both and `mind show rule:deploy` picks one.

The qualifier is a **colon**, not a slash, because a rule's name is already a
route: `rules/skills/naming.md` is the rule `skills/naming`, and a slash
qualifier would have made that mean "the skill `naming`". With a colon, a name
is always a name.

A workspace lists the projects it was **told** to manage, not whatever happens
to sit inside it, so the answer does not change with the directory you ran it
from. `flayer link` is how you tell it:

```
$ flayer link ../collapse
linked collapse as ../collapse
```

The entry is stored relative to the workspace, so the two can be moved
together — unless that route would not actually resolve, in which case the
absolute path is stored instead. Linking the same project twice changes
nothing and says so, naming the spelling already in the file. `unlink` removes
every entry pointing at the project, and still works on one whose directory has
moved away, which is exactly the entry worth removing.

```
$ mind validate
skill:commit-style: ok
rule:git/no-force-push: ok

1 skill and 1 rule checked, 0 invalid
```

Each kind is checked against what it actually declares. A skill's name has to
match its directory and its description has to fit; a rule declares nothing, so
the only things left to check are that its name is usable and that the file is
not empty.

`validate` exits non-zero when anything is wrong, so it works in CI. An
artifact that cannot be read at all is a warning on stderr and does not hide
the ones next to it.

## Gathering skills from elsewhere

A workspace has a **shelf**: skills collected from somewhere else, held by the
workspace and belonging to no project yet.

```
$ flayer gather git https://github.com/acme/skills
https://github.com/acme/skills at a1b2c3d
  added  commit-style  How this repo writes commit messages
  added  ddd-reviewer  Review a change against the DDD layering rules

2 skills: 2 added, 0 updated, 0 unchanged
```

The repository's `skills` folder is what gets harvested; `--path agents` takes
another one, and `--ref v2` takes a branch or a tag instead of the default.

Gathering fills the shelf and stops there. **Nothing is written into a mind
project**: which of these a project should carry is a separate decision, and a
`git clone` that quietly edited your repositories would be the wrong kind of
convenient. [`flayer install`](#installing-into-a-project) is where that
decision is made.

Run it again and it says what moved, which is the only thing worth reading the
second time:

```
$ flayer gather git https://github.com/acme/skills
https://github.com/acme/skills at 9f8e7d6
  updated    commit-style  How this repo writes commit messages
  unchanged  ddd-reviewer  Review a change against the DDD layering rules

2 skills: 0 added, 1 updated, 1 unchanged
```

Two repositories may both offer `commit-style`, and both are kept: each source
gets its own folder under `.mindflayer/skills/`, and the workspace's database
records which came from where.

```
$ flayer gather list
commit-style  https://github.com/acme/skills  How this repo writes commit messages
commit-style  https://github.com/other/rules  Conventional commits, strictly
ddd-reviewer  https://github.com/acme/skills  Review a change against the DDD layering rules
```

A skill that cannot be read is a warning on stderr and does not stop the ones
beside it from being gathered.

### What a workspace keeps

```
.mindflayer/
  flayer.toml     the projects it manages
  mindflayer.db   what was gathered, from where, and every action taken
  cache/<source>/ the clone, kept so a gather can be looked at afterwards
  skills/<source>/<name>/SKILL.md
```

`mindflayer.db` is SQLite, and it is the one thing Mindflayer writes that is
not meant to be read by hand — it answers questions a file cannot, like which
of two identically named skills came from which repository, at which revision.
Timestamps are Unix seconds, so `datetime(at, 'unixepoch')` renders them:

```sql
SELECT datetime(at, 'unixepoch'), action, outcome, detail FROM actions;
```

## Installing into a project

`flayer install` opens a screen. Projects on the left, the shelf on the right,
ticked where that project already holds the skill:

```
┌ Projects ──────────┐┏ Skills in collapse ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓
│> collapse  (1)     │┃  [x] commit-style  (not installed by mindflayer) ┃
│  tanukeys          │┃      How this repo writes commits  [acme/skills] ┃
│                    │┃> [x] deploy  + install                           ┃
│                    │┃      Ship the service to staging   [acme/skills] ┃
└────────────────────┘┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛
↑↓ skill   space mark   ←/esc back   a apply   q quit
1 to install, 0 to remove
```

Move down the projects and the right column follows. `→` or `enter` goes into
the skills, `space` ticks and unticks, `←` or `esc` comes back. Nothing touches
a file until `a`, which asks first and then does everything at once — installs
and removals together.

Each project keeps its own ticks, so one pass across the list is one plan for
the whole workspace. The number beside a project is how many of its boxes you
have moved.

**Unticking removes.** A skill is copied into the directory that project's
marker names, and unticking it deletes that directory again. With one
exception, which is the rule the whole command works by:

> Mindflayer only manages what it installed.

A skill already in the project that Mindflayer did not put there is shown
ticked and marked `(not installed by mindflayer)`. It cannot be unticked and it
is never overwritten or deleted — somebody wrote it. The ledger is what knows
the difference.

Two shelves can offer `commit-style`, but a project has one directory of that
name, so ticking one unticks the other rather than letting both apply and the
second win quietly.

## Where a project keeps its artifacts

`.mind/mind.toml` is the marker and the configuration; the artifacts themselves
sit beside the code, because these are files the **agents** read and an agent
does not know what a `.mind` is. `mind init` writes where each kind goes:

```toml
version = 2
name = "collapse"

[directories]
skills = "skills"
rules = "rules"
```

Point them wherever the agents already look, and Mindflayer follows:

```bash
mind init --skills .claude/skills
mind init --skills docs/skills --rules docs/rules
```

Both are relative to the project, and have to stay inside it — a mind project
is meant to be committed and its artifacts with it, so `--skills /etc/skills`
is refused rather than made. `init` never overwrites an existing marker, so a
second `mind init --skills elsewhere` changes nothing and says so: moving a
project's artifacts is not something `init` should do behind your back.

The values are written out even when they are the defaults, so the answer to
"where do this project's skills go" is in the file rather than in somebody's
memory. A workspace reads them too: it is how `flayer install` knows where a
skill goes, without ever assuming.

## What a skill looks like

`skills/commit-style/SKILL.md`:

```markdown
---
name: commit-style
description: How this repo writes commit messages and branch names
allowed-tools: Read, Grep
---

Use `<area>: <imperative summary>`, imperative and in English.
```

`name` and `description` are required; `name` has to match the directory and
`description` stay under 1024 characters. `allowed-tools` accepts either a
comma separated string or a YAML list. Anything else in the front matter is
carried along untouched.

A rule is simpler, because it declares nothing at all. `rules/git/no-force-push.md`:

```markdown
# Never force-push a shared branch

Use `--force-with-lease`, which refuses when someone else has pushed.
```

Its name is `git/no-force-push`, from where the file sits. Listings show its
opening line, which is why leading with a heading or a one-line summary is
worth doing. Every name, declared or derived, is checked segment by segment:
lowercase letters, digits and inner hyphens, each segment under 64 characters.

## Repository layout

Every unit of the product is an app under `apps/`, so adding a front end means
adding a directory rather than reshaping the tree:

```
apps/core   mindflayer-core — projects, workspaces, skills. No I/O beyond files.
apps/cli    mindflayer-cli  — the `mind` and `flayer` binaries. Parsing and
                              rendering only; both share one parser.
```

See [docs/architecture.md](docs/architecture.md) for why the split is where it
is.

## Development

A root `Makefile` delegates to each app, and CI invokes the same targets, so
`make help` is the list of everything there is to run:

```bash
make test              # every suite
make build             # debug build of every crate
make fmt               # cargo fmt --all
make lint              # clippy across the workspace
make run ARGS="list"   # run the CLI without installing it

make core/test         # one app: make <app>/<target>
make cli/run ARGS="flayer list"
```

Working on it, the loop is `make dev/link` once, then:

```bash
make dev/watch         # rebuilds on every change; the symlink stays current
```

`make dev/watch` needs `cargo-watch` (`cargo install cargo-watch`) and says so
if it is missing. Watch out for the version of that message which is not about
being missing at all: `cargo install` puts it in `~/.cargo/bin`, so if that is
not on your PATH, `make dev/watch` reports it as uninstalled while it sits
there installed. Note also that `make clean` deletes `target/`, which leaves
the `dev/link` symlinks dangling until the next build.

`BINDIR` says where the symlinks go, if `~/.cargo/bin` is not where you want
them:

```bash
make dev/link BINDIR=~/.local/bin
make dev/link BINDIR=.          # `./mind` and `./flayer`, right here
```

The repository root is a fine answer if you would rather not put a
work-in-progress binary on your PATH at all: `.gitignore` already covers both
names.

## License

GPL-3.0-only. See [LICENSE](LICENSE).
