# 🧠 Mindflayer

Manage what your coding agents read, the way git manages code: a project
carries a `.mind` directory holding its **skills** and **rules**, and a
workspace above it carries a `.mindflayer` that orchestrates several such
projects at once.

One shared Rust engine (`mindflayer-core`) behind every front end. The first
front end is the CLI — `mind` for a project, `flayer` for a workspace — and the
layout leaves room for a desktop app later without moving the engine.

> **MVP scope.** Mindflayer is being built incrementally. Today it creates the
> two kinds of directory, registers projects with a workspace, keeps skills and
> rules in each project and in the workspace itself, gathers skills from git
> repositories onto the workspace's shelf, installs them into the projects it
> manages, and creates, edits, renames and removes skills and rules — every one
> of those at both levels, from the command line and from a TUI that runs the
> same commands.

## The two levels

| | Marker | Holds | Think of it as |
|---|---|---|---|
| **Mind project** | `.mind/mind.toml` | its artifacts, in the directories the marker names | a repository's `.git` |
| **Flayer workspace** | `.mindflayer/flayer.toml` | references to mind projects, and artifacts of its own it shares with them | the directory your repos sit in |

A mind project is meant to be committed: the marker and the artifacts travel
with the code they describe, so whoever clones the repository gets its skills
and rules. A flayer workspace is the level above, where several projects are
managed together — and where the skills and rules that belong to all of them
live, once, to be installed into whichever needs them.

Every command that reads or changes artifacts works at both levels: `mind
<cmd>` on the project you are in, `flayer <cmd>` on the workspace — its own
artifacts, or one of its projects with `-p`. What only a workspace has —
`scan`, `gather`, `minds` — is `flayer`'s, and `mind flayer <cmd>` reaches it
from inside a project.

### The workspace in your home

A workspace is found the way `.git` is, by walking up from where you are. When
nothing up there has a `.mindflayer`, `flayer` uses **the one in your home**,
`~/.mindflayer` — so projects scattered anywhere can still be managed together
without choosing a directory first. `flayer init` somewhere else makes a
workspace there, and from inside it that one wins. The TUI says when it is
showing the default one.

The first command that writes to it makes it: `flayer link`, `add`, `gather
git`, `load <path>`, `install -p PROJECT <SKILL>`. One that only reads — `load`
and `install` on their own among them — leaves your home alone, answers as an
empty workspace would, and says where the workspace will be:

```
$ flayer list
you manages no projects yet
  link one with `flayer link <path>`
hint: no workspace here or above, and the default one is not made yet; the first command that writes to it, such as `flayer link <path>`, makes it in /home/you/.mindflayer
```

`--home DIR`, or `$MINDFLAYER_HOME`, puts the default somewhere other than your
home; set to empty (`--home ""`) there is no default, and a command outside
every workspace says so instead.

## The two kinds

| | Lives in | Shape | Declares |
|---|---|---|---|
| **Skill** | `skills/<name>/SKILL.md` | a directory each, so it can carry scripts and references beside its instructions | front matter in the [Agent Skills](https://agentskills.io/specification) format: `name`, `description`, and optionally `license`, `compatibility`, `metadata` and `allowed-tools` |
| **Rule** | `rules/<name>.md` | one markdown file each | usually nothing — it is context, and its name is its filename — though it may open with front matter: `paths` and `description`, both optional |

Folders under `rules/` group and mean nothing else, so
`rules/git/no-force-push.md` is the rule `git/no-force-push`. Skills are flat,
because a skill's directory already belongs to it.

Those are the defaults, and a project can say otherwise — see [where a project
keeps its artifacts](#where-a-project-keeps-its-artifacts).

## Getting started

From a fresh clone, one command builds both binaries and links them:

```bash
make dev/link                  # `mind`, `flayer` and `mf` in ~/.cargo/bin, pointing at target/debug
```

`mf` is short for `flayer`, the command that manages minds: `mf init` makes a
workspace, `mf link` registers projects with it, and `mf` on its own opens
[the TUI](#the-tui). It is only a link that `dev/link` makes, never installed by
`make install`, because `mf` is METAFONT wherever TeX is installed.

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
mind add skill deploy "Ship the service to staging"

cd ~/Projects
flayer init                    # a .mindflayer, to manage several of them
flayer link collapse           # tell it which projects those are
flayer list                    # every skill across all of them
```

Or run `flayer` on its own and do all of that from [the TUI](#the-tui): it
opens the workspace at or above where you are — or, with none, the one in your
home, and says so — and lists the repositories under it so linking is a pick
rather than a path to remember. To give `~/Projects` a workspace of its own,
run `flayer init` there first.

`init` never overwrites an existing marker. Run it twice and it says so and
changes nothing, so it is safe in a script.

## Commands

The surface is split the way the model is. `mind` acts on the project you are
standing in; `flayer` acts on the workspace above it.

```bash
mind                       # the TUI, on this project
mind init [--skills DIR] [--rules DIR]
                           # create a .mind here, saying where its artifacts go
mind add <KIND> <NAME> <DESCRIPTION> [--template NAME]
                           # create a skill or a rule that validates as written
mind edit <NAME> [--editor CMD]
                           # open it in $VISUAL or $EDITOR, then check it
mind rename <NAME> <NEW>   # folder and declared name together; `mv` for short
mind remove <NAME> --yes   # delete it; without --yes, say what would go; `rm`
mind templates             # what `add --template` can start from here
mind list [KIND]           # this project's artifacts; `mind list rules` filters
mind show <NAME>           # one artifact; `rule:deploy` when a name is ambiguous
mind validate [KIND|NAME]  # check a kind, one artifact, or everything
mind link / mind unlink    # this project, into or out of the workspace above
mind load / mind unload <path>...
                           # another workspace's skills, into the one above
mind install <SKILL> [--from ORIGIN]
                           # put a skill in: the workspace's own, or the shelf's
mind uninstall <SKILL>     # take one Mindflayer installed back out

flayer                     # the TUI: everything below, a key each
flayer init [--skills DIR] [--rules DIR]
                           # create a .mindflayer here
flayer add|edit|rename|remove|templates [-p PROJECT] ...
                           # the same as mind's: the workspace's own by
                           # default, one of its projects with -p
flayer list|show|validate [-p PROJECT] ...
                           # the workspace's own and every project's, or one;
                           # validate checks the shelf too
flayer scan                # the repositories under it, and which are linked
flayer link <path>...      # register mind projects with it
flayer unlink <path>...    # drop them
flayer minds               # a screen for linking and unlinking many at once
flayer load <path>...      # offer another workspace's own skills, read live
flayer unload <path>...    # stop offering them

flayer gather git <URL>    # collect skills from a repository onto the shelf
flayer gather list         # what is on the shelf or loaded, and where from
flayer install             # a screen for putting skills into its projects
flayer install -p PROJECT <SKILL> [--from ORIGIN]
                           # one, without the screen
flayer uninstall -p PROJECT <SKILL>

mind flayer <cmd>          # the long way round; `flayer <cmd>` is the shortcut
mind ls / flayer ls        # alias for list; `flayer gather ls` for gather list
mind -C <dir> ...          # work in <dir> instead of the current directory
mind --home <dir> ...      # where the default workspace lives; "" for none
mind --claude-config-dir|--codex-home|--opencode-config-dir|--agents-dir <dir> ...
                           # where each agent's own folder is; "" for none
mind tui / flayer tui      # the TUIs, spelled out
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
tanukeys  skill  ddd-reviewer       Review a change against the DDD rules
collapse  rule   git/no-force-push  Never force-push a shared branch
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

Each kind is checked against what it actually declares. A skill is held to the
[Agent Skills rules](#what-a-skill-looks-like) — its name has to match its
directory, its description has to fit, and so on; a rule's front matter is
optional, so the only things left to check are that its name is usable and that
the file says something beyond its front matter. `flayer validate` checks what
is on the [shelf](#gathering-skills-from-elsewhere) too.

`validate` exits non-zero when anything is wrong, so it works in CI. What an
agent reads past — a `metadata` entry that is not a string, a `compatibility`
over 500 characters, a rule's front matter that does not parse — is listed as a
warning instead, under an artifact that is still `ok`. An artifact that cannot
be read at all is a warning on stderr and does not hide the ones next to it.

### The agents' own folders

Each agent also reads a folder that belongs to no project: Claude Code its
configuration folder, Codex its home, OpenCode its configuration folder, and
more than one of them the skills in `~/.agents`. Checks to come will look
there; no command reads them yet. Each is taken from its option, then from the
variable the agent itself reads, then from its folder in your home:

| Option | Then | Then |
|---|---|---|
| `--claude-config-dir` | `$CLAUDE_CONFIG_DIR` | `~/.claude` |
| `--codex-home` | `$CODEX_HOME` | `~/.codex` |
| `--opencode-config-dir` | `$OPENCODE_CONFIG_DIR` | `~/.config/opencode` |
| `--agents-dir` | | `~/.agents` |

As with `--home`, set to empty (`--codex-home ""`) there is none, and that
agent's folder is left alone.

OpenCode itself reads `$OPENCODE_CONFIG_DIR` on top of `~/.config/opencode`
rather than in its place, by its own documentation. Mindflayer takes one
folder for each agent for now, the first the table gives; whether a check has
to read both of OpenCode's is still open.

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
$ flayer gather git https://github.com/other/rules
https://github.com/other/rules at 4d5e6f7
  added  commit-style  Conventional commits, strictly
  added  pdf--forms    Fill in PDF forms

2 skills: 2 added, 0 updated, 0 unchanged
warning: /home/you/Projects/.mindflayer/skills/github.com-other-rules/pdf--forms/SKILL.md: `pdf--forms` is not usable as a name: it has to be lowercase letters and digits, joined by single hyphens

$ flayer gather list
commit-style  https://github.com/acme/skills  How this repo writes commit messages
commit-style  https://github.com/other/rules  Conventional commits, strictly
ddd-reviewer  https://github.com/acme/skills  Review a change against the DDD layering rules
pdf--forms    https://github.com/other/rules  Fill in PDF forms
```

A skill that cannot be read is a warning on stderr and does not stop the ones
beside it from being gathered. One that reads but breaks a rule of the [skill
format](#what-a-skill-looks-like) — a doubled hyphen in its name, like
`pdf--forms` above, or metadata that is not all strings — is gathered anyway,
with a warning saying what is wrong: leaving it behind would hide the problem
along with the skill. `flayer validate` says so again whenever you ask, naming
the source folder each skill on the shelf sits in:

```
$ flayer validate skills
commit-style (shelf/github.com-acme-skills): ok
commit-style (shelf/github.com-other-rules): ok
commit-style (collapse): ok
ddd-reviewer (shelf/github.com-acme-skills): ok
ddd-reviewer (tanukeys): ok
pdf--forms (shelf/github.com-other-rules): 1 problem
  - `pdf--forms` is not usable as a name: it has to be lowercase letters and digits, joined by single hyphens

6 skills checked, 1 invalid
```

Installing is where it has to be right, because a project's copy is what its
agents load:

```
$ flayer install -p collapse pdf--forms
error: `pdf--forms` cannot be installed: `pdf--forms` is not usable as a name: it has to be lowercase letters and digits, joined by single hyphens
```

A private repository is fetched with whatever your git credential helpers
already know, or, over ssh, with the keys ssh already uses (your ssh-agent's,
or one in `~/.ssh`). Mindflayer itself never stops to ask for a username or a
password, so a source that needs one nobody has fails, and says what to set
up:

```
$ flayer gather git https://github.com/acme/private-skills
error: https://github.com/acme/private-skills: needs credentials, and gathering never prompts for them: configure a git credential helper, or use ssh-agent for an ssh URL
```

ssh is a program of its own and can still ask on the terminal: for the
passphrase of a key your ssh-agent does not hold, for a password when the
server accepts one, or whether to trust a host it has not seen before.

### What a workspace keeps

```
.mindflayer/
  flayer.toml     the projects it manages, and the workspaces it loads
  mindflayer.db   what was gathered and from where, what was installed where,
                  and every action taken
  cache/<source>/ the clone, kept so a gather can be looked at afterwards
  skills/<source>/<name>/SKILL.md
  templates/      templates for every project it manages, if you add any
```

`mindflayer.db` is SQLite, and it is the one thing Mindflayer writes that is
not meant to be read by hand — it answers questions a file cannot, like which
of two identically named skills came from which repository, at which revision.
The first gather, install or uninstall makes it; a command that only reads
never does.
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
│                    │┃      Ship the service to staging  [acme/skills]  ┃
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

The screen offers the workspace's own skills too, first and marked
`[workspace]`: that is how something written once for every project reaches
the ones that want it.

### Without the screen

```bash
mind install commit-style                  # into the project you are in
flayer install -p collapse commit-style    # into one the workspace manages
mind install deploy --from workspace       # when two places offer one name
mind uninstall commit-style
```

The same rule holds: a skill somebody wrote is never overwritten or removed,
and says so. `--from` takes `workspace`, a shelf source's URL, or a loaded workspace's path
or name, and is only
needed when two places offer the same name — which of the two lands in the
project's one folder of that name is not something to guess.

### Built out of sight

An install never writes beside the copy it replaces, where an agent reading the
skills folder would find it. It builds the copy in `.mind/tmp`, a folder that
ignores itself, and moves it into place once it is whole, so the skills folder
holds the old copy, the new one or, for the instant between two renames of an
update, neither, and never part of either. Two installs at once each build in a
folder of their own there. One that is stopped halfway leaves its work there
too, and the next install clears it and says so, as it does the hidden
`.<name>.mindflayer-new` an older version left beside a skill:

```
$ mind install commit-style
installed commit-style into alpha, from workspace
  cleared .mind/tmp/1901387-eb475fb4b6b73a14, left by an install that did not finish
```

## Loading another workspace

A flayer workspace can be a repository of skills in its own right: its own
skills and rules, kept for sharing. `flayer load <path>` makes one of those a
source for the workspace that manages you, without copying anything:

```
$ cd ~/skills-equipo              # a workspace with skills of its own
$ mf load .
loaded skills-equipo as skills-equipo: 2 skills to install
  into the workspace at /home/you, the default one

$ cd ~/code
$ mf install -p api deploy
installed deploy into api, from load:skills-equipo

$ mf load                         # what is loaded, and what each offers
skills-equipo  skills-equipo  2 skills to install
  in the workspace at /home/you, the default one
```

It goes into the workspace at or above where you run it that is not the one
being loaded — so `mf load .` from inside `skills-equipo` goes into the one
above it, or else [the one in your home](#the-workspace-in-your-home). The last
line always says which.

What it records is where the workspace is, in the `loaded` list of your
`flayer.toml`, and its skills are **read from there every time**: edit one in
`skills-equipo` and the next `mf install` of it updates the copy in `api`, with
nothing to load again. `flayer gather list` and the install screen show loaded
skills beside the shelf's, as coming from `load:<path>`, which is also what
`--from` takes — as do the path you gave `load`, from wherever you type it, and
the workspace's name. Only a loaded workspace's own are offered, never what it
loads in turn. `flayer unload <path>` forgets it; what was already installed
from it stays where it is. `mind load <path>` does the same from inside a
project, into the workspace above it.

A loaded workspace is usually somebody else's, so what it offers is read with
suspicion: a skill whose declared name is not one plain folder name (`..`,
`a/b`, an absolute path) is not offered, a symlink pointing out of the loaded
workspace is not followed, and nothing it keeps as its own is ever overwritten
or removed from a project that shares its folder. Each of these is a
`warning:` line, like a skill that cannot be read or a loaded workspace that
has gone.

## The workspace's own skills and rules

A workspace keeps skills and rules of its own, beside the projects it manages:

```bash
cd ~/Projects
flayer add skill commit-style "How every repo here writes commits"
flayer add rule git/no-force-push "Never force-push a shared branch"
flayer list
```

```
workspace  skill  commit-style       How every repo here writes commits
collapse   skill  deploy             Ship the service to staging
workspace  rule   git/no-force-push  Never force-push a shared branch
```

They live in `skills/` and `rules/` in the workspace's directory by default,
and `flayer init --skills DIR --rules DIR` says otherwise, as `mind init` does
for a project. Nothing is made there until the first one is added: a workspace
sits among your repositories and puts nothing beside them unasked.

A project holds what is in its own folders, because that is what its agents
read, so the workspace's skills reach a project by being installed into it —
`mind install`, `flayer install`, or the install screen — and are managed there
like anything else Mindflayer installed. Every command that changes an
artifact works on the workspace's own the same way it works on a project's:
`flayer edit`, `flayer rename`, `flayer remove`, and `-p <project>` for one of
its projects instead.

## The TUI

`flayer` on its own opens a screen over the whole workspace: its own artifacts
on the first row, marked `◆`, then every project it manages, and on the right
what the one under the cursor holds.

```
 mindflayer  Projects  · 3 on the shelf  · /home/you/Projects
┏ Projects (3) ━━━━━━━━━━━━━━━━┓┌ collapse ──────────────────────────────────────────────────────────────┐
┃  ◆ Projects  2               ┃│>   skill commit-style       How this repo writes commits               │
┃> collapse  2                 ┃│    rule  git/no-force-push  Never force-push a shared branch           │
┃  tanukeys  1  ✗ 1            ┃│                                                                        │
┃  ! old-api                   ┃│                                                                        │
┃                              ┃│                                                                        │
┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛└────────────────────────────────────────────────────────────────────────┘
q quit  ? keys  → open  a add  m minds  l link  u unlink  v validate  g gather  L load  s shelf  i install
every change is a command, printed when you quit — ? lists them
```

`mind` on its own opens the same screen from inside a project: on that project,
if a workspace manages it, or on the project alone if none does — where `l`
links it to the workspace above.

Every key runs a command you could have typed, through the same parser:

| Key | Does | Runs |
|---|---|---|
| `a` | add a skill or a rule | `mind -C <project> add <kind> <name> <description>` — on the `◆` row, `flayer add …` |
| `enter` | show an artifact | `mind -C <project> show <kind:name>` |
| `e` | edit it: the TUI steps aside for your editor | `mind -C <project> edit <kind:name>` |
| `r` | rename it | `mind -C <project> rename <kind:name> <new>` |
| `d` | delete it, once you say yes | `mind -C <project> remove --yes <kind:name>` |
| `m` | tick and untick every project under the workspace | `mind -C <path> init`, `flayer link <path>...`, `flayer unlink <path>...` |
| `l` | link a project | `flayer link <path>` |
| `u` | unlink the project | `flayer unlink <path>` |
| `v` | validate everything | `flayer validate` |
| `g` | gather from a git repository | `flayer gather git <url>` |
| `L` | load another workspace's own skills; enter on a loaded one unloads it | `flayer load <path>`, `flayer unload <path>` |
| `s` | what is on the shelf | `flayer gather list` |
| `i` | install from the workspace's own, a loaded workspace, or the shelf | `flayer install` |
| `R` | read the disk again | nothing: it changes nothing |

`m`, or `flayer minds` straight from the shell, opens a list of every
project the workspace links and every repository under it that it does not,
ticked where it does. Tick and untick, then `a` applies all of it in one go —
after saying which repositories `mind init` will write into. `link` and
`unlink` are all or nothing: if one path is refused, none is changed.

That is the rule it is built by: **nothing the TUI does is out of reach of a
command.** A form shows the line it is about to run as you type, and when the
TUI closes it prints every line that changed something, with what each said —
so a session can be read afterwards, and repeated in a script:

```
$ flayer link tanukeys
linked tanukeys as tanukeys
$ mind -C tanukeys add skill deploy 'Ship the service to staging'
created skill deploy at /home/you/Projects/tanukeys/skills/deploy/SKILL.md
```

Opened where there is no workspace and no default one either (`--home ""`), it
offers to create one there, then goes straight to linking; with a default, it
opens that one and says so — empty, and marked not made yet, until the first
change you make from it, which runs `flayer init` there first. The link form
lists what `flayer scan` finds under the workspace — mind projects, and git
repositories that are not one yet — and picking a repository asks before
running `mind init` in it, because that writes into it. A project whose
directory has gone is listed with a `!`, where `u` drops it; an artifact
`validate` would reject is marked `✗`, with the reason underneath.

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
license: MIT
compatibility: Needs git 2.30 or later
metadata:
  author: platform-team
  version: "1.2"
allowed-tools: Read Grep Bash(git log *)
---

Use `<area>: <imperative summary>`, imperative and in English.
```

The front matter is the [Agent Skills](https://agentskills.io/specification)
format, which Claude Code, Codex and OpenCode all read:

| Key | | Rule |
|---|---|---|
| `name` | required | lowercase letters and digits joined by single hyphens, at most 64 characters, the same as its directory |
| `description` | required | 1 to 1024 characters |
| `license` | optional | a license's name, or a file the skill bundles |
| `compatibility` | optional | what it needs to run, in 1 to 500 characters |
| `metadata` | optional | strings mapped to strings |
| `allowed-tools` | optional | separated by spaces; commas still work, and a YAML list does too |

Anything else in the front matter — an agent's own keys, like Claude Code's
`paths` — is carried along untouched, and so is a `metadata` entry or a
`compatibility` that is not a string: `validate` says it is ignored, and it
stays in the file.

The name and the description are the rules Mindflayer will not break when it
writes a skill, because agents skip a skill that does: `mind add`, `rename` and
`install` refuse one that breaks either. Everything else is reported by
`validate`, and gathering is lenient about all of it.

```
$ mind show commit-style
commit-style
/home/you/Projects/collapse/skills/commit-style/SKILL.md

How this repo writes commit messages and branch names

allowed-tools: Read, Grep, Bash(git log *)
license: MIT
compatibility: Needs git 2.30 or later
metadata:
  author: platform-team
  version: 1.2

Use `<area>: <imperative summary>`, imperative and in English.
```

A rule is simpler, because it usually declares nothing at all. `rules/git/no-force-push.md`:

```markdown
# Never force-push a shared branch

Use `--force-with-lease`, which refuses when someone else has pushed.
```

Its name is `git/no-force-push`, from where the file sits. Listings show its
opening line, which is why leading with a heading or a one-line summary is
worth doing. Every name, declared or derived, is checked segment by segment:
lowercase letters and digits joined by single hyphens, each segment at most 64
characters.

A rule may open with front matter, as Claude Code's rules do to say which files
they are about. `rules/api/errors.md`:

```markdown
---
description: How errors leave the API layer
paths:
  - "src/api/**/*.{ts,tsx}"
---

# Return errors as values

Never throw across a handler boundary; return a `Result`.
```

| Key | | Rule |
|---|---|---|
| `paths` | optional | glob patterns, as a list or one on its own; quote a pattern that opens with `*`, which YAML reads as an alias |
| `description` | optional | a string |

The fences only open front matter when what they hold is a YAML mapping, so a
rule that opens with a `---` horizontal rule and a plain sentence reads as it
always did. A line shaped like `key: value` is a mapping, though, even when it
is prose such as `Note: ...`, so it is front matter and left out of what an
agent is given. Front matter that does not parse — prose opening with `**`
does not either — is read as content, fences and all, and `validate` warns
about it. It warns too about a `paths` or a `description` it cannot read,
which is ignored and left in the file; any other key passes through untouched.
Listings show the opening line below the front matter, and `show` prints what
it declares above that:

```
$ mind list rules
api/errors         Return errors as values
git/no-force-push  Never force-push a shared branch

$ mind show api/errors
api/errors
/home/you/Projects/collapse/rules/api/errors.md

How errors leave the API layer

paths:
  - src/api/**/*.{ts,tsx}

# Return errors as values

Never throw across a handler boundary; return a `Result`.
```

### Creating one

```bash
mind add skill deploy "Ship the service to staging"
mind add rule git/no-force-push "Never force-push a shared branch"
```

Each writes the least that passes `validate` — a skill's front matter and a
heading, a rule's opening line — in the directory the project keeps that kind
in. The name is checked before anything is written, by the rule `validate`
applies; a skill cannot be filed in a folder; and nothing already there is
written over, not even a folder that is not a skill. The description is one
line, because the rest belongs in the file.

A skill made this way is its author's, not Mindflayer's: `flayer install`
treats it like anything else it did not put there, and never overwrites or
removes it.

#### From a template

```bash
mind add skill deploy "Ship the service" --template full
```

`full` ships with Mindflayer: a manifest with sections to fill in, and the
`scripts/` and `references/` folders the skill format sets aside. Your own go in
`.mind/templates/` for one project, or in the workspace's
`.mindflayer/templates/` for every project it manages — a folder holding a
`SKILL.md` for a skill, a markdown file for a rule. `mind templates` lists what
is available and where each comes from; of two with one name, the closer wins.
`flayer add -p` finds the workspace's wherever the project sits; `mind add`
finds those of the workspace above the project, or else the one in your home.

A template is copied whole. In a skill's manifest, `name` and `description` are
set by `mind add`, whatever the template had for them, and `{{name}}` and
`{{description}}` in the body are filled in. Other files are copied as they are.
A rule template is filled in throughout, front matter included, and a
`description` its front matter declares is set by `mind add` in the same way.
One without `{{description}}` gets the description as a heading on top of its
body, below any front matter, which stays front matter. Front matter that
`validate` would warn about is refused, and nothing is written.

### Changing one

```bash
mind edit deploy              # $VISUAL, then $EDITOR, then vi
mind rename deploy ship       # the folder and the `name` inside, together
mind remove ship              # says what would be deleted, deletes nothing
mind remove ship --yes        # deletes it
```

All three find an artifact by what it declares, or else by where its name puts
it — so a skill whose front matter does not parse, which no listing shows, can
still be opened and fixed. `edit` checks the file once the editor closes, and
says what it broke. `rename` rewrites the `name` line and nothing else: the
comments, the key order and the description stay as they were. `remove`
without `--yes` is a dry run, because it is the one thing here that cannot be
taken back.

Changing something `flayer install` put there makes it yours. Removing it,
renaming it or editing it drops the workspace's record that Mindflayer
installed it, so the install screen shows it as somebody's and never
overwrites or deletes it — and a skill you write later under the same name does
not inherit a record that was about something else. The record dropped is the
one held by the workspace the command works in: `flayer`'s, wherever the
project sits, or for `mind` the workspace above the project, or else the one in
your home. Another workspace that links the same project keeps its own (an
[open question](docs/architecture.md#open-questions)).

## Roadmap

What comes next is planned as
[milestones](https://github.com/otsobide/mindflayer/milestones), each ending
with something usable on real repositories, and each issue in them is one pull
request. [#15](https://github.com/otsobide/mindflayer/issues/15) tracks the
whole plan, and the decisions it rests on are recorded in
[docs/architecture.md](docs/architecture.md#decisions).

## Repository layout

Every unit of the product is an app under `apps/`, so adding a front end means
adding a directory rather than reshaping the tree:

```
apps/core    mindflayer-core   — projects, workspaces, artifacts.
                                 No I/O beyond files, and git when gathering.
apps/cli     mindflayer-cli    — the `mind` and `flayer` binaries, and the TUI.
                                 Parsing and rendering only; all of it goes
                                 through one parser.
apps/server  mindflayer-server — the local server behind the web panel, still
                                 unused by the binaries. Transport only: it
                                 moves JSON between the browser and a front
                                 end, and holds no rules.
apps/web     mindflayer-web    — the web panel the server embeds: Vue and
                                 Nuxt UI, built with Vite. It asks through
                                 the server and draws; no rules here either.
```

See [docs/architecture.md](docs/architecture.md) for why the split is where it
is.

## Development

A root `Makefile` delegates to each app, and CI invokes the same targets.
`make help` lists the root ones; each app's own, such as `cli/release`, are in
`apps/<app>/Makefile`:

```bash
make test              # every Rust suite; the panel's is make web/test
make build             # debug build of every crate
make fmt               # cargo fmt --all
make lint              # clippy across the workspace
make run ARGS="list"   # run the CLI without installing it

make core/test         # one app: make <app>/<target>
make cli/run ARGS="flayer list"
```

Tests live in their own files, named for what they test and ending in
`_test.rs`: `apps/<app>/tests/<area>_test.rs` for each area of the command
surface, with what they share in `tests/common/`, and a module's unit tests in
`src/<module>_test.rs` beside it. No test touches your home: the CLI suites
run every command with `--home ""`. Nor do they touch your agents' folders,
which no command reads yet, and the commands they run themselves give every
agent-home option empty. The server's suites never leave 127.0.0.1, and run
against a fake front end rather than the CLI.

```bash
cargo test -p mindflayer-cli --test tui_test          # one area
cargo test -p mindflayer-core --lib workspace         # one module's unit tests
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
make dev/link BINDIR=.          # `./mind`, `./flayer` and `./mf`, right here
```

The repository root is a fine answer if you would rather not put a
work-in-progress binary on your PATH at all: `.gitignore` already covers all
three names.

## Developing the panel

`apps/web` is the web panel, in Vue 3.5 and Nuxt UI 4 on Vite, written in
TypeScript. So far it is the frame the views will fill: a sidebar, a header
and an empty overview, dark unless you switch it to light, which the next
launch remembers. It needs Node 24 (`apps/web/.node-version`) and pnpm, at
the version `package.json` pins in `packageManager`, which `corepack enable`
provides. Without corepack, name any other way to run it in `PNPM`:

```bash
make web/test          # vitest
make web/lint          # ESLint, its style included; make web/fmt fixes that
make web/build         # check the types, then build apps/web/dist
make web/types         # check the types alone (vue-tsc)
make web/clean         # remove dist/ and the tools' caches

make web/test PNPM='npx --yes pnpm@12.15.0'
```

Every target that runs a tool first installs what `pnpm-lock.yaml` pins,
when `node_modules` is missing or older than `package.json`, the lockfile or
`pnpm-workspace.yaml`. `make test` leaves the panel out for now, so the Rust
suites still need no Node; the panel joins it once CI installs Node
([#71](https://github.com/otsobide/mindflayer/issues/71)). Its tests are
`apps/web/tests/<area>.test.ts`.

`make web/build` writes `apps/web/dist`, which the server embeds as it
compiles. A server compiled before any build answers every page with one
saying to run it, and goes on doing so after the build until
`mindflayer-server` compiles again: which is built first is an [open
question](docs/architecture.md#open-questions) until
[#68](https://github.com/otsobide/mindflayer/issues/68) settles it.
`make web/e2e` has no browser suite to run yet, and says so
([#75](https://github.com/otsobide/mindflayer/issues/75)).

`make web/dev` serves the panel on `http://127.0.0.1:5173` and reloads it as
you edit. It passes `/api` on to `flayer ui --dev --port 7878`, the command
[#68](https://github.com/otsobide/mindflayer/issues/68) adds, and the launch
link too: open the link that command prints with its port changed to 5173,
and the token is traded for its cookie where the panel runs.

Three rules keep the panel working under the server's content security
policy, which lets nothing in from another host and nothing inline:

- **The transport is the only way out.** Views ask `useTransport()` and
  never `fetch`: ESLint refuses `fetch`, `EventSource`, `WebSocket` and
  `XMLHttpRequest` everywhere but `src/api/http.ts`.
- **Nothing comes from another host.** The fonts are Geist and Geist Mono
  from Fontsource, the icons Lucide's, bundled when a component names them
  as `i-lucide-<name>`. A test builds the panel and fails on any URL that
  names another host.
- **Nothing inline.** No `<script>` or `<style>` written into the page, which
  is why the palette is declared in `src/assets/css/main.css` (from the
  mapping in `src/theme.ts`) rather than injected, as Nuxt UI would.

A test also checks that every package installed carries a licence
GPL-3.0-only can take in.

## License

GPL-3.0-only. See [LICENSE](LICENSE).
