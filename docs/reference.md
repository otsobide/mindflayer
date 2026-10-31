# Reference

Mindflayer manages what your coding agents read, the way git manages code: a
project carries a `.mind` directory beside its **skills** and **rules**, and
a workspace above it carries a `.mindflayer` that orchestrates several such
projects at once.

One shared Rust engine (`mindflayer-core`) behind every front end: the CLI
(`mind` for a project, `flayer` for a workspace), the TUI it opens, and a
[control panel](#control-panel) in your browser, served by the same binary.
The layout leaves room for a desktop app later without moving the engine.

This page is all of it, in detail: every command with what it prints, the
formats Mindflayer reads and writes, the TUI, the panel and upgrading. The
[README](../README.md) is the short tour; [development.md](development.md)
is for working on Mindflayer itself, and [architecture.md](architecture.md)
says why each part is the way it is.

## Contents

- [The two levels](#the-two-levels)
- [The two kinds](#the-two-kinds)
- [Getting started](#getting-started)
- [Try it with the examples](#try-it-with-the-examples)
- [Commands](#commands)
- [Gathering skills from elsewhere](#gathering-skills-from-elsewhere)
- [Installing into a project](#installing-into-a-project)
- [Loading another workspace](#loading-another-workspace)
- [What each project has](#what-each-project-has)
- [Machine-readable output](#machine-readable-output)
- [Following a project that moved](#following-a-project-that-moved)
- [Keeping projects up to date](#keeping-projects-up-to-date)
- [A repository that already has .claude/skills](#a-repository-that-already-has-claudeskills)
- [The workspace's own skills and rules](#the-workspaces-own-skills-and-rules)
- [The TUI](#the-tui)
- [Control panel](#control-panel)
- [Where a project keeps its artifacts](#where-a-project-keeps-its-artifacts)
- [What a skill looks like](#what-a-skill-looks-like)
- [Upgrading](#upgrading)

## The two levels

| | Marker | Holds | Think of it as |
|---|---|---|---|
| **Mind project** | `.mind/settings.toml` | the agents that work in it, and its artifacts, in the directories the marker names | a repository's `.git` |
| **Flayer workspace** | `.mindflayer/flayer.toml` | references to mind projects, and artifacts of its own it shares with them | the directory your repos sit in |

A mind project's `.mind` belongs to the checkout it is in: a short settings
file that keeps itself out of git, the way a Python virtualenv does. What the
project writes itself, its skills and rules, sits beside its code and is
committed with it, so whoever clones the repository gets them. A flayer
workspace is the level above, where several projects are managed together,
and where the skills and rules that belong to all of them live, once, to be
installed into whichever needs them.

Every command that reads or changes artifacts works at both levels: `mind
<cmd>` on the project you are in, `flayer <cmd>` on the workspace: its own
artifacts, or one of its projects with `-p`. What only a workspace has
(`scan`, `gather`, `minds`) is `flayer`'s, and `mind flayer <cmd>` reaches it
from inside a project.

### The workspace in your home

A workspace is found the way `.git` is, by walking up from where you are. When
nothing up there has a `.mindflayer`, `flayer` uses **the one in your home**,
`~/.mindflayer`, so projects scattered anywhere can still be managed together
without choosing a directory first. `flayer init` somewhere else makes a
workspace there, and from inside it that one wins. The TUI says when it is
showing the default one, and `flayer which` says which workspace a command
would use, and why.

The first command that writes to it makes it: `flayer link`, `add`, `gather
git`, `load <path>`, `install -p PROJECT <SKILL>`. One that only reads (`load`
and `install` on their own among them) leaves your home alone, answers as an
empty workspace would, and says where the workspace will be:

```
$ flayer list
you manages no projects yet
  link one with `flayer link <path>`
hint: no workspace here or above, and the default one is not made yet; the first command that writes to it, such as `flayer link <path>`, makes it in /home/you/.mindflayer
```

Made, it puts nothing in your home but `.mindflayer`. Its own skills and rules
go in its library inside it, where no agent looks, rather than in a `~/skills`
and `~/rules` above every project you have:

```
$ flayer add skill commit-style "How every repo here writes commits"
created skill commit-style at /home/you/.mindflayer/library/skills/commit-style/SKILL.md
```

One an older Mindflayer made keeps them in `~/skills` and `~/rules` until you
[move them into its library](#upgrading).

`--home DIR`, or `$MINDFLAYER_HOME`, puts the default somewhere other than your
home; set to empty (`--home ""`) there is no default, and a command outside
every workspace says so instead.

### Choosing a workspace

Walking up is what happens when nothing says otherwise. `-W DIR`
(`--workspace`) names the workspace a command uses, by its directory or its
`.mindflayer`, from wherever you are; `$MINDFLAYER_WORKSPACE`, which `flayer
activate` sets ([several flayers](#several-flayers)), does the same for every
command run in a shell, the way an activated Python virtualenv does.
Either one wins over a workspace above you, so you can keep several flayers
for different purposes and pick one:

```
$ cd ~/Projects/collapse                   # under the ~/Projects workspace
$ flayer -W ~/flayers/writing list         # a workspace beside it instead
prose-style  How every essay here is written

$ export MINDFLAYER_WORKSPACE=~/flayers/writing
$ flayer list                              # the same, for every command
prose-style  How every essay here is written
$ mind link                                # this project, into that one
linked collapse as ../../Projects/collapse
```

`mind` commands use it too: `mind install` installs from it, and a change to
an installed skill drops its record there, which is how a project managed by a
workspace somewhere else is worked on from inside it. What `-W` or the
variable names has to be a workspace already. Anything else is refused,
never passed over for the one above and never made into one:

```
$ flayer -W ~/notes list
error: -W names a workspace that cannot be used: /home/you/notes: not a flayer workspace (no .mindflayer/flayer.toml)
```

`-W ""` names none and leaves `$MINDFLAYER_WORKSPACE` out too, and an empty
variable counts as unset. `--home` still says only where the default lives.

### Several flayers

A flayer can be made anywhere and switched to for a whole shell, the way a
Python virtualenv is. `flayer init DIR` makes one in `DIR`, which the
workspace in your home remembers ([the flayers this machine
knows](#the-flayers-this-machine-knows)), and `flayer activate DIR` prints the
shell code that activates it, for `eval` to run:

```
$ flayer init ~/flayers/writing
initialized flayer workspace `writing` in /home/you/flayers/writing/.mindflayer
  remembered as flayers/writing by the default workspace
$ cd ~/Projects/collapse
$ flayer which
Projects at /home/you/Projects, found walking up from /home/you/Projects/collapse
$ eval "$(flayer activate ~/flayers/writing)"
(flayer:writing) $ flayer which
writing at /home/you/flayers/writing, activated by $MINDFLAYER_WORKSPACE
(flayer:writing) $ source .venv/bin/activate
(.venv) (flayer:writing) $ deactivate      # the virtualenv's; the flayer stays
(flayer:writing) $ flayer-deactivate
$ flayer which
Projects at /home/you/Projects, found walking up from /home/you/Projects/collapse
```

Activating sets `$MINDFLAYER_WORKSPACE` and puts `(flayer:<name>)` in front
of your prompt; `flayer-deactivate` puts both back as they were, a variable
you had set yourself included. Activating another flayer swaps one for the
other, and `flayer activate` on its own activates the one a command run there
would use. `flayer which` says which workspace that is and why: named by
`-W`, activated, found walking up, or the default one.

It is `flayer-deactivate` and never `deactivate`, so it gets along with a
virtualenv activated before it or after it: in bash and zsh, each one's
deactivate takes out only what its own activate put in, whichever runs first.
In the prompt, the name shows each of `` \ $ ` % ! `` as `_`, since a prompt
would act on them.

The code is written for the shell `$SHELL` names, or for the one `--shell`
does (`bash`, `zsh`, `fish` or `pwsh`). fish and PowerShell run it their own
way:

```
flayer activate --shell fish ~/flayers/writing | source
flayer activate --shell pwsh ~/flayers/writing | Out-String | Invoke-Expression
```

In those two, a virtualenv activated before the flayer and deactivated before
it takes the flayer's prefix with it: the flayer stays active, as `flayer
which` says, but the prompt no longer shows it for the rest of that shell.

Activating writes nothing anywhere, and what cannot be activated prints no
code, so `eval` runs nothing and the shell is left as it was:

```
$ eval "$(flayer activate ~/notes)"
error: /home/you/notes: not a flayer workspace (no .mindflayer/flayer.toml)
```

### The flayers this machine knows

The workspace in your home keeps a list of the other flayers on this machine,
so they can all be seen in one place and one picked to activate. `flayer init`
adds each one it makes, once the workspace in your home is made, and `flayer
workspaces` lists them, with a `*` before the one a command run where you are
would use:

```
$ cd ~/Projects/collapse
$ flayer workspaces
  you       /home/you                  no projects; the default one
  writing   /home/you/flayers/writing  no projects
* Projects  /home/you/Projects         3 projects, 1 stale; not remembered

  `flayer init <path>` remembers the one in use, and changes nothing else in it
$ flayer init ~/Projects
flayer workspace `Projects` already initialized in /home/you/Projects/.mindflayer
  remembered as Projects by the default workspace
```

Each one says how many projects it links, and how many of those are stale. A
flayer made before the workspace in your home was, or by an older Mindflayer,
is not on the list until `flayer init` runs on it again, which changes nothing
else in it; the one in use is listed all the same, marked as not remembered.
Activating a flayer does not remember it, since activating writes nothing. One
that has gone is listed as `cannot be opened`, with a warning, and `flayer
workspaces forget` takes it off the list, under every spelling it was
remembered by, leaving the flayer itself, if it is still there, as it is:

```
$ flayer workspaces forget ~/flayers/writing
forgot flayers/writing
```

The list is a `known` array in `~/.mindflayer/flayer.toml`, which you can read
and edit by hand, and which an older Mindflayer reads past and keeps. Listing
never makes the workspace in your home; until something does, it knows no
flayer, and with `--home ""` there is none to know them. `--json` prints the
list in the envelope every reading command's `--json` shares, and `w` shows it
in [the TUI](#the-tui). The [control panel](#control-panel)'s **Loads** page
lists them too, and loads any of them into the workspace it shows.

## The two kinds

| | Lives in | Shape | Declares |
|---|---|---|---|
| **Skill** | `skills/<name>/SKILL.md` | a directory each, so it can carry scripts and references beside its instructions | front matter in the [Agent Skills](https://agentskills.io/specification) format: `name`, `description`, and optionally `license`, `compatibility`, `metadata` and `allowed-tools` |
| **Rule** | `rules/<name>.md` | one markdown file each | usually nothing (it is context, and its name is its filename), though it may open with front matter: `paths` and `description`, both optional |

Folders under `rules/` group and mean nothing else, so
`rules/git/no-force-push.md` is the rule `git/no-force-push`. Skills are flat,
because a skill's directory already belongs to it.

Which agents a rule reaches depends on its `paths`. A rule that states none is
about every file, and reaches Claude Code, Codex and OpenCode alike, through a
marked block in the project's `AGENTS.md`, the one file all three read. Claude
reads `AGENTS.md` only where no `CLAUDE.md` or `CLAUDE.local.md` is in the
project or above it, so where the project's own `CLAUDE.md` is in the way,
and does not import `AGENTS.md` already, Mindflayer adds an `@AGENTS.md` line
to it, between markers, and takes it out with the last rule; where it may not
edit one (a `CLAUDE.md` above the project, a `CLAUDE.local.md`, or one git
tracks in a local project), Claude gets the rule as a rule file of its own
instead, so it reads each rule once. Mindflayer never creates a `CLAUDE.md`.
A rule that states `paths` is about those files only, and only Claude Code
honours that: it goes to Claude alone, as
`.claude/rules/mindflayer/<name>.md`, which Claude loads once it reads a file
the patterns match, and never into `AGENTS.md`, where every agent would read
it for every file. Codex and OpenCode read no `.claude/rules`, so a
path-scoped rule does not reach them, and Mindflayer says so for each one it
writes. A rule goes there when it is [installed](#installing-into-a-project),
as a skill does.

Those are the defaults, and a project can say otherwise: see [where a project
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

They are **symlinks**, not copies, so every later build updates the binaries
you are running without reinstalling anything: `make cli/build`, which needs
only Rust, or `make build`, which builds the panel first and so needs Node 24
and pnpm as well. `make dev/unlink` takes them back off. If you only want to
use the tools rather than work on them, `make install` puts real copies in the
same place. It builds [the control panel](#control-panel) into them first
where Node 24 and pnpm run ([developing the panel](development.md#developing-the-panel) has
which pnpm, and `PNPM` for one corepack has not put on your PATH). Without
them it warns and makes the copies anyway, with the panel `apps/web/dist`
holds from an earlier build, or none, and then their `flayer ui` serves only a
page saying the panel was not built.

That place is `~/.cargo/bin`, which is on your PATH if `rustup` put it there
and is **not** if your Rust came from a system package. `make dev/link` says so
when it is missing; `BINDIR` is how you point somewhere else, and
[Development](development.md#development) has the rest.

Then make a repository a mind project, and give the folder it sits in a
workspace to manage it, and others like it, from:

```
$ cd ~/Projects/collapse
$ mind init
initialized mind project `collapse` in /home/you/Projects/collapse/.mind
$ mind add skill deploy "Ship the service to staging"
created skill deploy at /home/you/Projects/collapse/skills/deploy/SKILL.md
$ git status --short
?? skills/

$ cd ~/Projects
$ flayer init
initialized flayer workspace `Projects` in /home/you/Projects/.mindflayer
$ flayer link collapse
linked collapse as collapse
$ flayer list
deploy  Ship the service to staging
```

`mind init` makes a `.mind` holding `settings.toml`, which says which agents
work in the project and where it keeps its own skills and rules, and a
`.gitignore` holding `*`, which is why `git status` shows none of it. The
`skills/` and `rules/` it makes beside the code are the project's, to commit
with it. Every agent is named unless `--agents claude,codex` names fewer, and
`--shared` leaves the `.gitignore` out, for a team that commits its `.mind`:
[where a project keeps its artifacts](#where-a-project-keeps-its-artifacts)
has the rest.

Or run `flayer` on its own and do all of that from [the TUI](#the-tui): it
opens the workspace at or above where you are (or, with none, the one in your
home, and says so) and lists the repositories under it so linking is a pick
rather than a path to remember. To give `~/Projects` a workspace of its own,
run `flayer init` there first.

`init` never overwrites an existing marker. Run it twice and it says so and
changes nothing, so it is safe in a script.

## Try it with the examples

`examples/` in this repository is a workspace of its own, holding two skills
and two rules written to be used as they are, so there is something to
install before you have gathered anything. With the repository cloned in
`~/src/mindflayer`, [name it](#choosing-a-workspace) to see what it holds:

```
$ flayer -W ~/src/mindflayer/examples list
skill  code-review     Reviews a change for bugs, safety, tests and scope, most severe first.…
skill  commit-style    Writes commit messages as "area: imperative summary". Use when writing,…
rule   git/commits     Commit one coherent step at a time
rule   style/comments  Comments say why, not what
```

[Load it](#loading-another-workspace) into the workspace that manages your
projects, and install from it:

```
$ cd ~/Projects
$ flayer load ~/src/mindflayer/examples
loaded examples as ../src/mindflayer/examples: 2 skills and 2 rules to install
  into the workspace at /home/you/Projects

$ flayer install -p collapse commit-style
installed commit-style into collapse, from load:../src/mindflayer/examples
$ flayer install -p collapse git/commits
installed rule git/commits into collapse, from load:../src/mindflayer/examples
```

`collapse/.claude/skills/commit-style` now holds the skill and the
`references/examples.md` it carries, for Claude Code, and
`collapse/.agents/skills/commit-style` the same for Codex and OpenCode.
`collapse/AGENTS.md` holds the rule, in a block of Mindflayer's that all
three read, and all of it is kept out of git. `style/comments` installs the
same way, but it states which files it is about, so it goes to Claude Code
alone, and the install says so. `flayer -W ~/src/mindflayer/examples
validate` says all four are ok.

## Commands

The surface is split the way the model is. `mind` acts on the project you are
standing in; `flayer` acts on the workspace above it.

```bash
mind                       # the TUI, on this project
mind init [--agents AGENTS] [--shared] [--skills DIR] [--rules DIR]
                           # create a .mind here: for which agents, kept out of
                           # git unless shared, and where its artifacts go
mind init --new-id         # give the project here an id of its own: for a
                           # copy made with its .mind
mind add <KIND> <NAME> <DESCRIPTION> [--template NAME]
                           # create a skill or a rule that validates as written
mind edit <NAME> [--editor CMD]
                           # open it in $VISUAL or $EDITOR, then check it
mind edit <NAME> --contents FILE|- [--if-unchanged SHA256]
                           # replace what it holds with a file, or standard
                           # input, checked before anything is written
mind rename <NAME> <NEW>   # folder and declared name together; `mv` for short
mind remove <NAME> --yes   # delete it; without --yes, say what would go; `rm`
mind templates [--json]    # what `add --template` can start from here
mind list [KIND] [--json]  # this project's artifacts; `mind list rules` filters
mind show <NAME> [--json]  # one artifact; `rule:deploy` when a name is ambiguous
mind validate [KIND|NAME] [--json]
                           # check a kind, one artifact, or everything
mind status [--json]       # what it holds, from where, for which agents
mind link / mind unlink    # this project, into or out of the workspace above
mind load / mind unload <path>...
                           # another workspace's skills and rules, into the
                           # one above
mind install <NAME> [--from ORIGIN] [--force]
                           # put a skill or a rule in: the workspace's own, a
                           # loaded workspace's, or the shelf's; --force
                           # writes over a copy changed since it was installed
mind uninstall <NAME>|--orphaned [--force]
                           # take one Mindflayer installed back out, or every
                           # one nothing offers any more
mind update [NAME...] [--force] [--yes]
                           # bring what was installed up to date with where
                           # it came from, and put back copies gone; without
                           # --yes, say what would change
mind agents [add|remove] <AGENT>... [--yes]
                           # which agents work here, carrying what is
                           # installed along; without --yes, say what would
mind share local|shared [--edit-tracked] [--yes]
                           # keep installs to this checkout, or leave them
                           # for the team to commit; without --yes, the same
mind targets set <KIND> <DIR>... [--yes]
mind targets reset <KIND> [--yes]
                           # install a kind in folders of the project's own,
                           # or back where its agents read; the same again
mind migrate [--agents AGENTS] [--local|--shared] [--yes]
                           # move a project an older Mindflayer made to a
                           # settings.toml, and its installs where its
                           # agents read

flayer                     # the TUI: everything below, a key each
flayer ui [--port PORT] [--no-open] [--dev]
                           # the control panel, in a browser, until Ctrl-C;
                           # mind ui is the same
flayer init [PATH] [--skills DIR] [--rules DIR]
                           # create a .mindflayer here, or in PATH
flayer activate [DIR] [--shell SHELL]
                           # the code that activates a workspace: eval "$(...)"
flayer which               # the workspace a command here would use, and why
flayer workspaces [--json] # the flayers this machine knows, the one in use
                           # marked
flayer workspaces forget <path>
                           # take one off that list
flayer add|edit|rename|remove|templates [-p PROJECT] ...
                           # the same as mind's: the workspace's own by
                           # default, one of its projects with -p
flayer remove <NAME> --everywhere [--yes]
                           # one of its own, out of every project it was
                           # installed into first; without --yes, the same
flayer list|show|validate [-p PROJECT] [--json] ...
                           # the workspace's own and every project's, or one;
                           # validate checks the shelf too
flayer status [-p PROJECT] [--json]
                           # what every project holds, or one: current,
                           # outdated, edited, missing, orphaned, foreign or
                           # authored, for which agents, and from where
flayer scan [DIR...] [--depth N] [--suggest] [--json]
                           # the repositories under it, or under each DIR,
                           # which are linked, and which moved; --suggest
                           # lists what is most worth linking first
flayer link <path>...      # register mind projects with it
flayer unlink <path>...    # drop them
flayer relink <old> <new>  # follow a project that moved, keeping what was
                           # installed into it
flayer minds [DIR...]      # a screen for linking and unlinking many at once
flayer load <path>...      # offer another workspace's own skills and rules,
                           # read live
flayer unload <path>...    # stop offering them

flayer gather git <URL>    # collect skills from a repository onto the shelf
flayer gather list [--json]
                           # what is on the shelf or loaded, and where from
flayer install             # a screen for putting skills and rules into its
                           # projects
flayer install (-p PROJECT)...|--all-projects <NAME> [--from ORIGIN] [--force]
                           # one, into the projects named or every one,
                           # without the screen
flayer uninstall (-p PROJECT)...|--all-projects <NAME>|--orphaned [--force]
flayer update (-p PROJECT)...|--all [NAME...] [--force] [--yes]
                           # mind update, for the projects named or every one
flayer adopt -p PROJECT <NAME> [--yes]
                           # a skill a project keeps by hand in an agent's
                           # folder, into the library and managed from there;
                           # without --yes, say what would happen
flayer agents|share|targets -p PROJECT ...
                           # mind's agents, share and targets, for one of its
                           # projects
flayer migrate --library [--yes]
                           # move its own skills and rules into its library;
                           # without --yes, say what would move
flayer migrate -p PROJECT|--all-projects [--agents AGENTS] [--local] [--yes]
                           # mind migrate, for one of its projects or every one

mind flayer <cmd>          # the long way round; `flayer <cmd>` is the shortcut
mind ls / flayer ls        # alias for list; `flayer gather ls` for gather list
mind -C <dir> ...          # work in <dir> instead of the current directory
mind -W <dir> ...          # the workspace to use, from anywhere; "" for none
                           # named; $MINDFLAYER_WORKSPACE otherwise
mind --home <dir> ...      # where the default workspace lives; "" for none
mind --claude-config-dir|--codex-home|--opencode-config-dir|--agents-dir <dir> ...
                           # where each agent's own folder is; "" for none
mind tui / flayer tui      # the TUIs, spelled out
```

Both find what they act on the way git does, by walking up from wherever you
are, unless [a workspace is named](#choosing-a-workspace). Both levels answer
a different question, and that is the point:

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
when only one kind is in play, the project column when only one project is:
the same rule in both cases.

A name is bare until it needs qualifying. When one name belongs to two kinds,
`mind show deploy` shows both and `mind show rule:deploy` picks one; `mind
install deploy` and `mind uninstall deploy` refuse it, saying which to type.

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
together, unless that route would not actually resolve, in which case the
absolute path is stored instead. Linking the same project twice changes
nothing and says so, naming the spelling already in the file. `unlink` removes
every entry pointing at the project, and still works on one whose directory has
moved away, which is exactly the entry worth removing.

`flayer scan` finds what there is to tell it about: every mind project and git
repository under the workspace, three levels down, and whether each is linked
yet. It links nothing.

```
$ cd ~/Projects && flayer scan
collapse  linked
notes     not linked  git repository, not a mind project yet
tanukeys  not linked  mind project

  link one with `flayer link <path>`
  a repository needs `mind -C <path> init` before it can be linked
```

`--suggest` asks which to link first. It lists what is not linked before
what is, ranked by what each holds, and says why on each row: a mind project
whose settings give it an id, which the workspace can follow when it moves;
then one without (marked by an older `mind.toml`, or by a marker that would
not open); then a git repository holding something an agent reads at its
root, named on the row; then any other repository. What counts as an agent's
is what Claude Code, Codex and OpenCode read there: `AGENTS.md`,
`AGENTS.override.md`, `CLAUDE.md`, `CLAUDE.local.md`, `.mcp.json`,
`opencode.json`, `.claude`, `.agents`, `.codex` and `.opencode`
([where each agent reads](agents.md#what-says-an-agent-works-in-a-project)).
It still links nothing and makes nothing a project: a suggestion is a line
to type, or a row to tick on [the minds screen](#the-tui), which lists them
in this order.

```
$ flayer scan --suggest
tanukeys  not linked  mind project with an id
notes     not linked  git repository with CLAUDE.md
collapse  linked

  link one with `flayer link <path>`
  a repository needs `mind -C <path> init` before it can be linked
```

Name a folder, or several, and it looks under those instead; `--depth` says
how many levels below each. Every path is spelled the way `flayer link` takes
it from where you ran the scan, so a line of the report is a line to type: a
route from here, or the whole path of one outside.

```
$ flayer scan ~/Work
/home/you/Work/acme/api  not linked  git repository, not a mind project yet
/home/you/Work/acme/web  not linked  mind project

  link one with `flayer link <path>`
  a repository needs `mind -C <path> init` before it can be linked
```

A scan is bounded so that a folder at the top of your home does not turn
into a crawl of the disk. It goes three levels below each folder unless
`--depth` says otherwise. Hidden folders, symlinks, `node_modules` and
`target` are never entered, nor is a repository it finds, whose folders are
its own. Agent files are looked for only at the top of each one it finds,
by name: never opened, and never followed when they are links. However
deep it goes, it stops after listing 4096 folders and says so.
`flayer minds ~/Work` opens [the minds screen](#the-tui) on what is there,
and `--json` prints the report in the envelope
[`status --json`](#what-each-project-has) uses, with the id each project's
settings name it by, the reason each is worth linking (`id`, `mind`,
`agents` or `git`) and the agent files it holds. With `--suggest`, `found`
is in the suggested order:

```
$ flayer scan ~/Work --depth 2 --json
{
  "version": 2,
  "data": {
    "roots": [
      "/home/you/Work"
    ],
    "depth": 2,
    "found": [
      {
        "path": "/home/you/Work/acme/api",
        "route": "/home/you/Work/acme/api",
        "id": null,
        "mind": false,
        "linked": false,
        "moved": null,
        "reason": "git",
        "agent_files": []
      },
      {
        "path": "/home/you/Work/acme/web",
        "route": "/home/you/Work/acme/web",
        "id": "f4a29f5daaf5bc789c4b5d6b13174f48",
        "mind": true,
        "linked": false,
        "moved": null,
        "reason": "id",
        "agent_files": []
      }
    ],
    "untraced": [],
    "truncated": false
  },
  "warnings": [],
  "coded_warnings": []
}
```

```
$ mind validate
skill:commit-style: ok
rule:git/no-force-push: ok

1 skill and 1 rule checked, 0 invalid
```

Each kind is checked against what it actually declares. A skill is held to the
[Agent Skills rules](#what-a-skill-looks-like): its name has to match its
directory, its description has to fit, and so on; a rule's front matter is
optional, so the only things left to check are that its name is usable and that
the file says something beyond its front matter. `flayer validate` checks what
is on the [shelf](#gathering-skills-from-elsewhere) too.

`validate` exits non-zero when anything is wrong, so it works in CI. What an
agent reads past (a `metadata` entry that is not a string, a `compatibility`
over 500 characters, a rule's front matter that does not parse) is listed as a
warning instead, under an artifact that is still `ok`. An artifact that cannot
be read at all is a warning on stderr and does not hide the ones next to it.

### The agents' own folders

Each agent also reads a folder that belongs to no project: Claude Code its
configuration folder, Codex its home, OpenCode its configuration folder, and
more than one of them the skills in `~/.agents`. An install never puts a copy
inside one of them, since a skill there reaches every project; checks to come
will look in them too. Each is taken from its option, then from the variable
the agent itself reads, then from its folder in your home:

| Option | Then | Then |
|---|---|---|
| `--claude-config-dir` | `$CLAUDE_CONFIG_DIR` | `~/.claude` |
| `--codex-home` | `$CODEX_HOME` | `~/.codex` |
| `--opencode-config-dir` | `$OPENCODE_CONFIG_DIR` | `~/.config/opencode` |
| `--agents-dir` | | `~/.agents` |

As with `--home`, set to empty (`--codex-home ""`) there is none, and that
agent's folder is left alone.

Where each agent reads skills and rules, at project and user scope, and which
of those facts were checked on the real binaries, is recorded in
[agents.md](agents.md).

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
another one, and `--ref v2` takes a branch or a tag instead of the default. A
repository that is a flayer workspace made by `flayer init` keeps its own in
[its library](#the-workspaces-own-skills-and-rules), so it is gathered with
`--path .mindflayer/library/skills`, or [loaded](#loading-another-workspace)
from a clone instead.

With stderr on a terminal it says what it is doing while it does it
(`connecting`, `receiving objects 120/340`, `reading folders 3/40`) on one line
there, which it wipes before the report, so the report reads as above. With
stderr piped or redirected, it says nothing until the report.

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
format](#what-a-skill-looks-like) (a doubled hyphen in its name, like
`pdf--forms` above, or metadata that is not all strings) is gathered anyway,
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

A URL with a password or a token in it is refused before anything is fetched.
What is gathered is stored, listed and reported by its URL, and a token typed
into a command line is already where `ps` and your shell history show it. Give
the token to a git credential helper, and gather the URL without it:

```
$ flayer gather git https://me:ghp_t0ken@github.com/acme/private-skills
error: https://github.com/acme/private-skills: a password, a token or, over http(s), a user name in the URL is refused, because gathering stores and shows the URL: gather it without one, and configure a git credential helper that has it
```

The user ssh logs in as is not a credential, and is kept
(`git@github.com:acme/skills`, `ssh://git@github.com/acme/skills`). Over https
a user name on its own is refused too, because a token can stand in for one:
GitHub takes `https://<token>@github.com/...`, and nothing in the URL tells the
two apart. That includes the https URLs Bitbucket and Azure DevOps hand out
(`https://<user>@bitbucket.org/...`); gather those without the user name, and
let the credential helper supply it.

A workspace an older Mindflayer gathered into with a token in a URL is cleaned
up by the next command that writes to it: a gather, an install or an
uninstall, or an `add`, `edit`, `rename` or `remove` in a project it manages.
The token is taken out of `mindflayer.db` and out of the clone in
`cache/`, and the command says so once, on stderr, without failing because of
it:

```
$ flayer install -p collapse deploy
installed deploy into collapse, from https://github.com/acme/private-skills
note: https://github.com/acme/private-skills: the user name, password or token an older Mindflayer stored in this URL is taken out; gathering it again needs a git credential helper that has it
note: 2 action log entries: the user names, passwords or tokens an older Mindflayer stored in URLs in them are taken out
```

The install screen does it when you apply what you marked, not when it opens.
A command that only reads, such as `flayer gather list`, shows the URL without
the token and leaves the file as it is.

### What a workspace keeps

```
.mindflayer/
  flayer.toml     the projects it manages, the workspaces it loads, and where
                  its own skills and rules are
  flayer.toml.v2  flayer.toml as it was before `flayer migrate --library`
                  rewrote it, if it did
  library/skills/<name>/SKILL.md
  library/rules/<name>.md
                  its own, unless flayer.toml says otherwise
  mindflayer.db   what was gathered and from where, what was installed where,
                  and every action taken; a URL goes in without credentials,
                  and one an older Mindflayer stored is taken out by the next
                  command that writes
  mindflayer.db.v2.bak
                  the database as it was before this version of Mindflayer
                  updated it, if it did
  lock            held by a command while it changes the workspace
  cache/<source>/ the clone, kept so a gather can be looked at afterwards
  skills/<source>/<name>/SKILL.md
  templates/      templates for every project it manages, if you add any
```

`mindflayer.db` is SQLite, and it is the one thing Mindflayer writes that is
not meant to be read by hand: it answers questions a file cannot, like which
of two identically named skills came from which repository, at which revision.
The first gather, install or uninstall makes it; a command that only reads
never does.
When a new version of Mindflayer changes how the database is laid out, the
first command to open it updates it, after copying it aside as
`mindflayer.db.v<old version>.bak`. The copy is never removed for you, and it
is the way back: an older Mindflayer refuses the updated database, and reads
the copy put back in its place. It is the database as it was, so a token an
older Mindflayer stored in a URL stays in the copy after the next command that
writes takes it out of `mindflayer.db`.
Timestamps are Unix seconds, so `datetime(at, 'unixepoch')` renders them:

```sql
SELECT datetime(at, 'unixepoch'), action, outcome, detail FROM actions;
```

The TUI and any number of terminals can be open on one workspace, and the
commands that change it take turns. Linking, loading, gathering, installing,
uninstalling and migrating each hold `lock` while they write, with the
process's id in `lock.pid` beside it. One that finds the workspace held waits
up to ten seconds for the other to finish, then gives up and says who it
waited for:

```
$ flayer link tanukeys
error: /home/you/Projects/.mindflayer/lock: workspace busy: waited 10s for process 3234405 to finish writing to it; try again once it has
```

Reading never waits for `lock`. Listing, showing and the TUI's screens read
each file whole, as the last write to it left it, though a read made while a
gather or an install is under way can find some of what it changes changed
and the rest not yet. The database keeps a write-ahead log, so a read does not
wait for a write to it either, except on a filesystem that cannot keep one,
where it waits for the write to finish. A workspace on a network share is safe
used from one machine at a time: the lock and the database keep apart programs
on one machine, not programs on two.

## Installing into a project

`flayer install` opens a screen. Projects on the left, and on the right what
can be installed into the one under the cursor, one kind at a time, ticked
where that project already holds it:

```
┌ Projects ──────────┐┏ Skills in collapse ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓
│> collapse  (1)     │┃  [x] commit-style  (not installed by mindflayer) ┃
│  tanukeys          │┃      How this repo writes commits  [acme/skills] ┃
│                    │┃> [x] deploy  + install                           ┃
│                    │┃      Ship the service to staging  [acme/skills]  ┃
└────────────────────┘┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛
↑↓ skill  space mark  tab rules  ←/esc back  v by skill  a apply  q quit
1 to install, 0 to remove
```

Move down the projects and the right column follows. `tab` shows the rules in
it instead, and the skills again (`shift+tab` goes the other way). `→` or
`enter` goes into the column, `space` ticks and unticks, `←` or `esc` comes
back. Nothing touches a file until `a`, which asks first and then does
everything at once, of both kinds: installs and removals together.

What `a` runs is the commands you would have typed: a `flayer install` for
each skill or rule ticked, with a `-p` for every project ticked to take it
and `--from` for where the one ticked comes from, and a `flayer uninstall`
for each one unticked. It prints each with what it said, as the TUI prints a
session when it closes, so a session with the screen in it can be run again
without it:

```
$ flayer install -p collapse -p tanukeys --from load:../src/mindflayer/examples skill:commit-style
installed commit-style into collapse, from load:../src/mindflayer/examples
installed commit-style into tanukeys, from load:../src/mindflayer/examples
$ flayer uninstall -p collapse rule:git/commits
removed rule git/commits from collapse
```

One that fails says why where it ran, and the rest run all the same. All of
it is on stdout, warnings and errors included, each under the line it is
about.

Each project keeps its own ticks, so one pass across the list is one plan for
the whole workspace. The number beside a project is how many of its boxes you
have moved.

`v` turns the screen round, to put one skill or rule into several projects in
a pass: everything of the kind shown on the left, and on the right every
project, ticked where it holds the one under the cursor, which `→` goes into
and `space` ticks as before:

```
┌ Skills ──────────────────────────────────────────┐┏ Projects for deploy ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓
│  commit-style                                    │┃> [x] collapse  + install                         ┃
│      How this repo writes commits  [acme/skills] │┃  [ ] tanukeys                                    ┃
│> deploy  (1)                                     │┃                                                  ┃
│      Ship the service to staging  [acme/skills]  │┃                                                  ┃
└──────────────────────────────────────────────────┘┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛
↑↓ project  space mark  tab rules  ←/esc back  v by project  a apply  q quit
1 to install, 0 to remove
```

They are the same boxes either way round, so a tick made one way is there the
other, and `a` applies them all: a line for each skill or rule ticked and one
for each unticked, whichever way round the ticks were made. `v` again turns
it back, with the cursor on the same box, and `tab` still shows the other
kind. A project that cannot take the kind shown has no box there, and says
why.

**Unticking removes.** A skill is copied wherever the project's agents read it
([below](#where-a-project-keeps-its-artifacts)), or into the directory a
`mind.toml` names, and unticking it deletes those copies again. A rule is
written into the project's `AGENTS.md`, and into Claude's own rule files
where Claude needs it there ([the two kinds](#the-two-kinds)), and unticking
it takes it back out of both. With one exception, which is the rule the whole
command works by:

> Mindflayer only manages what it installed.

A skill already where an install would put it, that Mindflayer did not put
there, is shown ticked and marked `(not installed by mindflayer)`. It cannot be
unticked and it is never overwritten or deleted: somebody wrote it. The ledger
is what knows the difference. One in any of the folders an install would write
refuses the whole install, and the others are left alone too, so the agents
never read two different skills of one name. So is a rule whose section in
`AGENTS.md` Mindflayer did not write there: another workspace linking the
project did, or a hand that copied a marker.

What Mindflayer installed and somebody changed since is held the same way:
shown ticked and marked `(changed since it was installed)`, and neither
written over nor deleted from the screen, since the change is somebody's
work too. The ledger keeps a hash of what each install wrote, which is how
it tells: a copy edited by hand or with `mind edit`, a file added to it, or a
rule's section of `AGENTS.md` rewritten. Only `--force` on [the command
line](#without-the-screen) writes over it or takes it out.

An install with a copy gone since, deleted by hand or by a `git clean`,
opens unticked and marked `(a copy gone since it was installed)`: ticking it
puts every copy back. One that still has a copy left is taken out on [the
command line](#without-the-screen), with `uninstall`. One whose skill or rule
has changed where it came from since stays ticked, marked `(outdated; flayer
update refreshes it)`: [updating](#keeping-projects-up-to-date) brings it up
to date.

Two shelves can offer `commit-style`, but a project has one directory of that
name, so ticking one unticks the other rather than letting both apply and the
second win quietly. The one ticked when the screen opens is the one the
project's copy was installed from, as the workspace recorded it, even when
that copy is behind both places or edited since. A skill and a rule of one
name are two things, and keep a box each.

The screen offers the workspace's own skills and rules too, first and marked
`[workspace]`: that is how something written once for every project reaches
the ones that want it.

### Without the screen

```bash
mind install commit-style                  # into the project you are in
flayer install -p collapse commit-style    # into one the workspace manages
flayer install --all-projects commit-style # into every one it links
mind install deploy --from workspace       # when two places offer one name
mind install git/commits                   # a rule, by its route
mind install rule:deploy                   # when a skill and a rule share it
mind uninstall commit-style
```

The same rule holds: a skill or a rule somebody wrote is never overwritten or
removed, and says so. `--from` takes `workspace`, a shelf source's URL, or a
loaded workspace's path or name, and is only needed when two places offer the
same name: which of the two lands in the project's one folder of that name is
not something to guess.

`flayer install` takes `-p` once for each project, to install into several
at once, or `--all-projects` for every one the workspace links, and `flayer
uninstall` takes them the same way. Each project is said as an install into
it alone says it, in the order asked, with what its rules warn of after its
name (`warning: collapse: ...`), and one that refuses is said of it while the
others go ahead:

```
$ flayer install commit-style --all-projects
installed commit-style into collapse, from workspace
warning: tanukeys already has a commit-style at skills/commit-style that Mindflayer did not put there, so it was left alone
$ flayer uninstall commit-style --all-projects
removed commit-style from collapse
warning: commit-style at skills/commit-style in tanukeys was not installed by Mindflayer, so it was left alone
```

An uninstall from several passes over a project that has nothing of that
name, unless none of them has it, and a refusal every project shares, such as
a name nothing offers, is said once rather than once a project.

A copy changed since it was installed is left alone too, every other copy
with it, until `--force` says to write over it or take it out:

```
$ echo "Keep the subject under 50 characters." >> .agents/skills/commit-style/SKILL.md
$ mind install commit-style
warning: commit-style in collapse was changed since it was installed, at .agents/skills/commit-style, so it was left alone; --force writes over it
$ mind uninstall commit-style
warning: commit-style in collapse was changed since it was installed, at .agents/skills/commit-style, so it was left alone; --force takes it out
$ mind install commit-style --force
updated commit-style in collapse, from workspace
```

Forcing reaches only what Mindflayer installed: somebody's skill that it did
not put there stays out of reach whatever is asked.

A rule's line says it is one, since a route reads like a path. One about some
files only reaches Claude alone, and the install says so, as
[`status`](#what-each-project-has) does whenever asked; a name a skill and a
rule share needs its kind:

```
$ mind install git/commits
installed rule git/commits into collapse, from workspace
$ mind install style/comments
installed rule style/comments into collapse, from workspace
warning: rule `style/comments` states `paths`, so it reaches claude only: codex and opencode read no .claude/rules
$ mind install deploy
error: `deploy` is offered as a skill and as a rule, so say which: `skill:deploy` or `rule:deploy`
$ mind install rule:deploy
installed rule deploy into collapse, from workspace
```

A rule the project received is not one of its own, but rendered from where it
was installed from, so `mind edit`, `rename` and `remove` leave it and say
how to take it out:

```
$ mind remove git/commits --yes
error: `git/commits` is a rule collapse received, rendered from where it was installed from rather than kept in it, so it is not changed from here: uninstall it, or change it where it is offered and install it again
```

### Built out of sight

An install never writes beside the copy it replaces, where an agent reading the
skills folder would find it. It builds the copy in `.mind/tmp`, a folder that
ignores itself, and moves it into place once it is whole (every copy of it,
when the project's agents read two folders), so the skills folder holds the
old copy, the new one or, for the instant between two renames of an update,
neither, and never part of either. Two installs at once each build in a
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
being loaded, so `mf load .` from inside `skills-equipo` goes into the one
above it, or else [the one in your home](#the-workspace-in-your-home). The last
line always says which.

What it records is where the workspace is, in the `loaded` list of your
`flayer.toml`, and its skills and rules are **read from there every time**:
edit one in `skills-equipo` and the next `mf update -p api`, or `mf install`
of it, [updates the copy](#keeping-projects-up-to-date) in `api`, with
nothing to load again. `flayer gather list` and the install
screen show what it offers beside the shelf's, as coming from `load:<path>`,
which is also what `--from` takes, as do the path you gave `load`, from
wherever you type it, and the workspace's name. Only a loaded workspace's own
are offered, never what it loads in turn. `flayer unload <path>` forgets it;
what was already installed from it stays where it is. `mind load <path>` does
the same from inside a project, into the workspace above it, and the
[control panel](#control-panel) does both from its **Loads** page.

A loaded workspace is usually somebody else's, so what it offers is read with
suspicion: a skill whose declared name is not one plain folder name (`..`,
`a/b`, an absolute path) is not offered, and neither is a rule filed under a
route no rule may be named; a symlink pointing out of the loaded workspace is
not followed, and nothing it keeps as its own is ever overwritten or removed
from a project that shares its folder. Each of these is a `warning:` line,
like a skill that cannot be read or a loaded workspace that has gone.

## What each project has

`flayer status` says what every project the workspace manages holds: whether
Mindflayer put each thing there, which agents read it, and where it came from.
With [the examples loaded](#try-it-with-the-examples), and installed from
there, `commit-style` and `code-review` into `collapse` and `commit-style` into
`tanukeys`, a project an older Mindflayer made:

```
$ cd ~/Projects
$ flayer status
collapse  code-review   missing   codex,opencode         load:../src/mindflayer/examples
collapse  commit-style  current   claude,codex,opencode  load:../src/mindflayer/examples
collapse  deploy        authored  -
collapse  notes         foreign   claude,opencode
tanukeys  commit-style  current   -                      load:../src/mindflayer/examples
hint: tanukeys is marked by a mind.toml, which `mind migrate` moves to a settings.toml
```

Each artifact is in one of seven states, the first five for what Mindflayer
installed:

| State | Means |
|---|---|
| `current` | Mindflayer put every copy there, every one is still there as it was put, and where it came from offers the same. |
| `outdated` | the same, but where it came from offers something newer: [`update`](#keeping-projects-up-to-date) brings it up to date. |
| `edited` | a copy it put there holds something else now, edited by hand or with `mind edit`, or a skill renamed with `mind rename`; still Mindflayer's install, but no install or update writes over it and no uninstall takes it out without `--force`. |
| `missing` | a copy it put there is gone, deleted by hand or by a `git clean`, or no longer reads; `update` puts every copy back. Here Claude's copy of `code-review` was deleted. |
| `orphaned` | where it came from no longer offers it, deleted from the library or from a workspace that stopped being loaded; nothing can bring it up to date or put a copy back, so it stays as it is until it is uninstalled, by name or with every other one by `uninstall --orphaned`. |
| `foreign` | somebody else's, in a folder an agent reads; Mindflayer never overwrites or deletes it. Here `notes`, kept in `.claude/skills` by hand. |
| `authored` | the project's own, in a folder it authors in, such as `skills/`. |

Then the agents that read a copy of it, as `--agents` names them: `-` for
none, as for `deploy`, which `collapse` keeps in `skills/`, a folder no agent
reads, and for everything in a project whose `.mind` an older Mindflayer
made, which names no agents. Last, where what was installed came
from, as `--from` takes it: `workspace`, `load:<path>` or a shelf source's
address. A project an older Mindflayer made, marked by a `mind.toml` or a
`settings.json`, gets a hint after the report naming [`mind
migrate`](#a-project-made-before-settings), which moves it to a
`settings.toml`; the exit status stays the report's. `mind status` says the
same of the project you are in, and `flayer status -p tanukeys` of one; as in
`list`, the project column is there only when there is more than one project
to tell apart, and the kind column only when there is more than one kind.

A rule the project received is read from what was installed and from its
`AGENTS.md`, and its agents are the ones it reaches: all of the project's
through the block, Claude alone for one about some files, which goes to
Claude's own rule files and nowhere else. One some of the project's agents
read nowhere is said, as for `collapse` given `git/commits` and
`style/comments` [above](#without-the-screen):

```
$ mind status
git/commits     current  claude,codex,opencode  workspace
style/comments  current  claude                 workspace
warning: codex and opencode do not read rule `style/comments` in collapse
```

A linked project whose directory has gone is listed as `stale`, with the
reason on stderr, and the rest of the report still prints; the exit status is
non-zero, as for anything that could not be read. So is a project that
[installs where some of its agents do not read](#installing-somewhere-else):
a warning names the agents that read none of its folders, and `unreached` in
the JSON says the same; and one given a rule some of its agents read nowhere,
which `missed` in the JSON says.

`--json` prints the same report in the envelope every reading command's
`--json` shares ([machine-readable output](#machine-readable-output)): the
data, the warnings the text says on stderr, each again with its code, and a
version, which moves if a field ever goes or changes meaning. The names
in it are a contract, spelled as a command line spells them; the version is 2
since `installed` became `current`, `outdated` or `orphaned`, and `partial`
became `missing`:

```
$ flayer status -p tanukeys --json
{
  "version": 2,
  "data": {
    "projects": [
      {
        "name": "tanukeys",
        "entry": "tanukeys",
        "path": "/home/you/Projects/tanukeys",
        "marker": "mind.toml",
        "agents": [],
        "stale": null,
        "unreached": [],
        "missed": [],
        "artifacts": [
          {
            "kind": "skill",
            "name": "commit-style",
            "state": "current",
            "agents": [],
            "origin": "load:../src/mindflayer/examples"
          }
        ]
      }
    ]
  },
  "warnings": [],
  "coded_warnings": []
}
hint: tanukeys is marked by a mind.toml, which `mind migrate` moves to a settings.toml
```

## Machine-readable output

The commands that report what is there take `--json`, at either level:
`list`, `show`, `validate`, `templates` and `status`, and `flayer
workspaces`, `flayer scan` and `flayer gather list`. Each prints what it
read in one envelope, the same for every command, so a script or an agent
reads them all one way, and reads what a workspace holds, its own
artifacts and each project's, their templates and what it can install from
elsewhere, as every front end reads them. What it loads, the sources its
shelf is filled from and its history have no `--json` yet. Say `collapse`
holds the rule `git/no-force-push`, and a skill folder whose `SKILL.md` has
lost its front matter:

```
$ mind validate --json
{
  "version": 2,
  "data": {
    "artifacts": [
      {
        "kind": "rule",
        "name": "git/no-force-push",
        "path": "/home/you/Projects/collapse/rules/git/no-force-push.md",
        "project": "/home/you/Projects/collapse",
        "issues": [],
        "invalid": false,
        "copies_differ": false
      }
    ],
    "invalid": 0,
    "unreadable": 1,
    "differing": 0
  },
  "warnings": [
    "/home/you/Projects/collapse/skills/broken/SKILL.md: the file does not start with a `---` front matter fence"
  ],
  "coded_warnings": [
    {
      "code": "front_matter_missing",
      "message": "/home/you/Projects/collapse/skills/broken/SKILL.md: the file does not start with a `---` front matter fence"
    }
  ]
}
warning: /home/you/Projects/collapse/skills/broken/SKILL.md: the file does not start with a `---` front matter fence
```

`data` is what the command's text lists:

- `list`: each holder as every front end reads it, `library` for the
  workspace's own (`null` for one project's listing), with the `origin` a
  copy installed from it names and, in `holders`, each project holding a
  copy of one of them, by its entry, and `projects` for every entry the
  workspace links, a stale one saying why in `stale`. A
  project says where it authors each kind, its settings and templates, and
  every artifact it holds: its state, the agents that read it, where it came
  from, where each copy is, what `validate` says of it, and for a file that
  does not read, `unread`, with why. Its `tally` counts them, by kind and by
  state, and its `health` says how it stands as a whole, in the words the
  [control panel](#control-panel) sorts by: `ok`, `attention`, `broken` or
  `stale`. `mind list --json` reads the project against the workspace that
  links it, as `mind status` does, to say how each stands; when that
  workspace cannot be read, the project is read alone and its `failures`
  say why, with no warning the text does not say. `list rules --json` holds
  rules alone, and its `tally` and `health` are of what it holds: a skill
  that does not read leaves a project `ok` there, which `list --json` calls
  `broken`.
- `show`: each artifact the name matches, with its file, what its front
  matter declares and its `body`.
- `validate`: each artifact checked, its `issues` (each with its `code`,
  whether it is only a `warning`, and its `message`) and whether it is
  `invalid`, and the counts the verdict follows.
- `templates`: each template, its `origin` the folder or file it is, `null`
  for one built in.
- `gather list`: what can be installed from elsewhere, each with the `place`
  it comes from (`loaded` or `shelf`) and its `origin`, as `--from` takes it.
- `status`, `workspaces` and `scan`: the reports shown in [what each project
  has](#what-each-project-has), [the flayers this machine
  knows](#the-flayers-this-machine-knows) and [commands](#commands).

`warnings` holds what the text says on stderr as `warning:` lines, without
the prefix, and `coded_warnings` the same lines, in the same order, each
beside its `code`: a word for what went wrong that rewording the message
never changes, `null` for a line none is given to. Each is still said on
stderr too, as above, as the text says it, so piping the JSON into `jq`
leaves them on the terminal; a script reads them in the envelope and need
not read stderr, where a `hint:`, advice for whoever reads the terminal, is
said too and the envelope leaves it out. The exit status is the text's:
non-zero when something could not be read, as above, and for `validate`
when something does not pass. An error that stops a command is still a line
on stderr, with nothing on stdout.

The names in it are a contract. `version` is 2, and version 2 only grows
additively: a field added leaves it at 2, so a script written for it passes
over a name it does not know, and only a field dropped, renamed or holding
something else would move it.

## Following a project that moved

The workspace knows each project by its route from the workspace, so moving
the repository leaves its entry `stale`, and the record of everything
installed into it naming a place nothing is at. What moves with it is the
`id` in its `.mind/settings.toml`, random, so that only a copy shares it,
which the workspace keeps beside the route it last saw it at. `flayer scan`
finds the project by it, and `flayer relink` follows it there. Say
`collapse`, with `commit-style` and `git/commits` installed into it from the
examples as [above](#try-it-with-the-examples), is the one project the
workspace links, and moves into `apps/`:

```
$ cd ~/Projects
$ mkdir apps && mv collapse apps/
$ flayer status
collapse  stale
warning: /home/you/Projects/collapse: not a mind project (no .mind/settings.toml, .mind/settings.json or .mind/mind.toml)
$ flayer scan
apps/collapse  not linked  moved: collapse -> apps/collapse

  follow one that moved with `flayer relink <old> <new>`
$ flayer relink collapse apps/collapse
relinked collapse as apps/collapse, from collapse
  2 installs kept with it
$ flayer status
skill  commit-style  current  claude,codex,opencode  load:../src/mindflayer/examples
rule   git/commits   current  claude,codex,opencode  load:../src/mindflayer/examples
```

The entry is rewritten where it stands in `flayer.toml`, comments and all,
and every record moves with it, so what was installed is current again rather
than somebody's. In a local project, what keeps those copies out of git goes
along: lines of `info/exclude` naming the old place are written for the new
one, in the same repository or the one it moved into, and a repository that
moved whole has them right already. `M` in [the TUI](#the-tui), on a stale
project the scan found, does the same.

`flayer relink` refuses, with nothing changed: a directory that holds another
id than the one the workspace last saw at the entry, which is some other
project; an entry that still holds a project, even one whose marker would not
open, which has not moved; a project a `mind.toml` marks, which has no id to
be found by ([`mind migrate`](#a-project-made-before-settings) gives it one);
and records from before ids were kept, which nothing says were that
project's. A workspace learns each linked project's id when its ledger is
made or brought up to this version, and again every time it writes, so a
project moved before then is found by nobody. `flayer scan` names the entry
it left instead, in a hint to `flayer relink <entry> <path>`, for you to say
where it went: the entry keeps its place, where `flayer link` would leave it
stale beside a second one.

A repository copied with its `.mind` holds the same id as the one it was
copied from, and nothing can tell which of the two the records were made for.
So the copy is never taken for a move: wherever two projects holding one id
are read together, by `flayer scan` or `flayer status`, it is a warning
naming both, and a non-zero exit, and `flayer relink` refuses to follow it.
`mind init --new-id` in the copy gives it an id of its own and keeps
everything else, its name included:

```
$ cp -r apps/collapse collapse-copy
$ flayer scan
apps/collapse  linked
collapse-copy  not linked  mind project

  link one with `flayer link <path>`
warning: /home/you/Projects/collapse-copy and /home/you/Projects/apps/collapse are both project 4f9c2a7e1b3d5f60a8c4e2b6d0f1a3c5, copied with its .mind; `mind -C <copy> init --new-id` gives a copy an id of its own
$ mind -C collapse-copy init --new-id
gave collapse a new id, 0123456789abcdef0123456789abcdef (it was 4f9c2a7e1b3d5f60a8c4e2b6d0f1a3c5)
```

A local copy's lines in `info/exclude` follow its new id, and a workspace
that links it knows it, and what was installed into it, by the new one. A
project still marked by a `settings.json` moves to a `settings.toml` first,
with `mind migrate`.

## Keeping projects up to date

What was installed stays what it was when it went in, so a skill or a rule
changed where it is kept (the workspace's own, a workspace it loads, or the
shelf once a gather brings something newer) leaves every project it went into
behind it, and `status` says `outdated`. `flayer update` brings them up to
date from where each was installed from, every copy at once, and puts back
any copy that has gone, kept out of git again in a local project: one deleted
by hand, or an `AGENTS.md` Mindflayer made, which `git clean -X` takes. A
`git clean -dX` takes a local project's `.mind` along with its copies, so run
`mind init` there again first, with the agents and share mode it had, and the
update puts every copy back. Without `--yes` it says what it would do. Say a
pull of `~/src/mindflayer` has changed the examples' `commit-style`, and
somebody has written into `tanukeys`'s copy of it, with the rest as
[above](#what-each-project-has):

```
$ flayer status
collapse  code-review   missing   codex,opencode         load:../src/mindflayer/examples
collapse  commit-style  outdated  claude,codex,opencode  load:../src/mindflayer/examples
collapse  deploy        authored  -
collapse  notes         foreign   claude,opencode
tanukeys  commit-style  edited    -                      load:../src/mindflayer/examples
hint: tanukeys is marked by a mind.toml, which `mind migrate` moves to a settings.toml
$ flayer update --all
would put back code-review in collapse at .claude/skills/code-review, from load:../src/mindflayer/examples
would update commit-style in collapse, from load:../src/mindflayer/examples
nothing changed: add --yes to go ahead
warning: commit-style in tanukeys was changed since it was installed, at skills/commit-style, so it would be left alone; --force writes over it
$ flayer update --all --yes
put back code-review in collapse at .claude/skills/code-review, from load:../src/mindflayer/examples
updated commit-style in collapse, from load:../src/mindflayer/examples
warning: commit-style in tanukeys was changed since it was installed, at skills/commit-style, so it was left alone; --force writes over it
```

A copy somebody changed is never written over unforced, and neither is any
other copy of it, so the agents reading two folders never read two versions
of one skill. `--force` writes over it, saying where the change was:

```
$ flayer update -p tanukeys --force --yes
updated commit-style in tanukeys, writing over the change at skills/commit-style, from load:../src/mindflayer/examples
```

`-p` names a project, once for each, and `--all` every one the workspace
links; names after them update only those (`commit-style`, or
`rule:git/commits` when a skill and a rule share one), and one that names
nothing installed is a warning. `mind update` does the same in the project you
are in, and `u` in [the TUI](#the-tui) on a project, or on one artifact of
it. A project's rules are written into its `AGENTS.md` together, so updating
one brings its other outdated rules along, and says so, line by line. What
was installed from somewhere that no longer offers it is `orphaned`: there is
nothing to update it from, so it is said and left as it is, never a failure,
until it is uninstalled:

```
$ flayer unload ~/src/mindflayer/examples
unloaded ../src/mindflayer/examples
  from the workspace at /home/you/Projects
$ cd collapse && mind update
nothing to update in collapse
note: code-review in collapse is no longer offered by load:../src/mindflayer/examples, so it would be left as it is; uninstalling it takes it out
note: commit-style in collapse is no longer offered by load:../src/mindflayer/examples, so it would be left as it is; uninstalling it takes it out
```

An update copies what the shelf holds now: what a gathered source offers
moves when it is gathered again.

## A repository that already has .claude/skills

Many repositories keep skills written by hand in `.claude/skills` or
`.agents/skills`, committed with the code. Mindflayer lists each as
`foreign`, somebody else's, and never touches it: no update reaches it, and
an agent that reads the other folder never gets it. `flayer adopt` brings
one into the workspace's library, when you ask and never on its own, and
records the project's copies as installed from there, exactly as they are:
not a byte of the project is written. Say `collapse` is a local project that
keeps `notes` in `.claude/skills`, committed:

```
$ cd ~/Projects
$ flayer status -p collapse
notes  foreign  claude,opencode
$ flayer adopt -p collapse notes
would copy notes from collapse into the library, at /home/you/Projects/.mindflayer/library/skills/notes
would record notes in collapse as installed from workspace, at .claude/skills/notes
would record .agents/skills/notes missing, for `flayer update -p collapse` to put there
nothing adopted: add --yes to go ahead
note: git tracks .claude/skills/notes, so it stays in git though collapse is local: what an update writes there shows in `git status`
$ flayer adopt -p collapse notes --yes
copied notes from collapse into the library, at /home/you/Projects/.mindflayer/library/skills/notes
recorded notes in collapse as installed from workspace, at .claude/skills/notes
recorded .agents/skills/notes missing: `flayer update -p collapse` puts it there
note: git tracks .claude/skills/notes, so it stays in git though collapse is local: what an update writes there shows in `git status`
$ flayer update -p collapse --yes
put back notes in collapse at .agents/skills/notes, from workspace
$ flayer status -p collapse
notes  current  claude,codex,opencode  workspace
```

From then on `notes` is the workspace's own, managed in `collapse` like
anything installed there: current while nobody changes it, `edited` once
somebody does, and brought up to date by `flayer update` when the library
copy changes. The update above gave Codex, which reads only
`.agents/skills`, the copy it had never had; adopting itself writes nothing
into a project, so a folder an install would put a copy in that holds none
is recorded `missing` until then. Every other project can now be given it with
`flayer install -p <project> notes`.

A copy git tracks stays in git, even in a local project: a line in
`info/exclude` would keep none of it out, and would hide every file added
to it later. An update writes over it there, for the team to see and
commit. One git does not track is kept out of git in a local project, as
anything installed there is, and a local project no git work tree holds is
told, before and after, that nothing keeps it out.

Without `--yes` it writes nothing. It refuses, with nothing written: a name
the library holds already, saying both paths; copies in `.claude/skills`
and `.agents/skills` that differ, since which is the skill cannot be told; a
skill an install would refuse, its name not matching its folder or its
description empty or too long; one holding a link, or a `.claude-plugin`
folder at its top, which no install copies; one Mindflayer installed
already, or the project's own, in the folder it authors in; and a rule. If
its `SKILL.md` says where `gh skill` got it, under `metadata` keys starting
`github-`, the dry run and the adoption show it in a line under the
first, and nothing more is made of it: no other tool's lockfile is read or
written. `A` in [the TUI](#the-tui), on a `foreign` row, asks and then runs
the same line.

## The workspace's own skills and rules

A workspace keeps skills and rules of its own, for the projects it manages:

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

They live in the workspace's library, `.mindflayer/library/skills` and
`.mindflayer/library/rules`, inside `.mindflayer` where no agent looks: they
reach an agent by being installed into its project, never by sitting in a
directory above it. `flayer init` writes both into `flayer.toml`:

```toml
[directories]
skills = ".mindflayer/library/skills"
rules = ".mindflayer/library/rules"
```

`flayer init --skills DIR --rules DIR` says otherwise, as `mind init` does for
a project. Any folder inside the workspace will do except the shelf and the
cache, which are reserved for gathering, a folder inside them, or one holding
them, so `--skills .mindflayer/skills` and `--rules .mindflayer` are refused. A
workspace made before the library keeps them in `skills/` and `rules/` beside
its projects, because that is what its `flayer.toml` says, until `flayer
migrate --library` [moves them](#upgrading). Either way nothing is made until
the first one is added.

A project holds what is in its own folders, because that is what its agents
read, so the workspace's skills and rules reach a project by being installed
into it (`mind install`, `flayer install`, or the install screen) and are
managed there like anything else Mindflayer installed. Every command that
changes an artifact works on the workspace's own the same way it works on a
project's: `flayer edit`, `flayer rename`, `flayer remove`, and `-p <project>`
for one of its projects instead.

### Removing one everywhere

Removing one of the workspace's own takes it out of the library, and only
there: what was installed from it stays in each project, `orphaned`, since
nothing offers it any more. `flayer remove` says so, in the dry run and once
it is done:

```
$ flayer remove commit-style
warning: commit-style is installed in collapse and tanukeys, which removing it alone would leave orphaned there; --everywhere takes it out of them too
error: nothing removed: this deletes /home/you/Projects/.mindflayer/library/skills/commit-style and the 1 file in it; add --yes to go ahead
```

`--everywhere` takes it out of every project the workspace links first, as
`flayer uninstall` would, and then out of the library. Without `--yes` it
says what each project holds of it and how that stands, and writes nothing.
Here a copy in `tanukeys` was changed since it was installed:

```
$ flayer remove commit-style --everywhere
would remove commit-style from collapse, where it is current
would leave commit-style in tanukeys, where it is edited, at .agents/skills/commit-style
would remove skill commit-style from /home/you/Projects/.mindflayer/library/skills/commit-style and the 1 file in it
nothing removed: add --yes to go ahead
$ flayer remove commit-style --everywhere --yes
removed commit-style from collapse
left commit-style in tanukeys, where it is edited, at .agents/skills/commit-style
removed skill commit-style from /home/you/Projects/.mindflayer/library/skills/commit-style
note: nothing offers commit-style in tanukeys any more; `flayer uninstall -p tanukeys commit-style --force` takes it out
```

A copy that is current, outdated or missing one of its copies is
uninstalled. An edited one is left, every copy of it, as an uninstall leaves
it unforced: it is somebody's work, still Mindflayer's install, and offered
by nothing once the library copy is gone. Somebody's own skill of that name,
which Mindflayer never installed, is left too, and said as `foreign`. Only
the projects the workspace links are looked at. A rule goes the same way,
out of every project's `AGENTS.md`, and `d` on the `◆` row of [the
TUI](#the-tui) asks with a second yes, `e`, for everywhere.

What is orphaned already, by a removal without `--everywhere` or a workspace
that stopped being loaded, `uninstall --orphaned` takes out: of the projects
named, of every one with `--all-projects`, or of the one `mind` runs in. An
edited one stays until `--force`:

```
$ flayer remove commit-style --yes
removed skill commit-style from /home/you/Projects/.mindflayer/library/skills/commit-style
warning: commit-style is still installed in collapse and tanukeys, orphaned now that nothing offers it; `flayer uninstall --orphaned --all-projects` takes out what nothing offers
$ flayer uninstall --orphaned --all-projects
removed commit-style from collapse
warning: commit-style in tanukeys was changed since it was installed, at .agents/skills/commit-style, so it was left alone; --force takes it out
```

A copy from a loaded workspace that cannot be opened is not taken for
orphaned, though `status` says it is: what that workspace offers cannot be
told until it is back. Nor is one whose artifact is still there but cannot be
read, its front matter broken: that is a `warning:` to mend, never a reason
to take its copies out.

## The TUI

`flayer` on its own opens a screen over the whole workspace: its own artifacts
on the first row, marked `◆`, then every project it manages, and on the right
what the one under the cursor holds.

```
 mindflayer  Projects  · found above · 3 on the shelf  · /home/you/Projects
┏ Projects (3) ━━━━━━━━━━━━━━━━┓┌ collapse ──────────────────────────────────────────────────────────────┐
┃  ◆ Projects  2               ┃│>   skill commit-style       current  CC CX OC  How this repo writes com│
┃> collapse  2                 ┃│    rule  git/no-force-push  authored           Never force-push a share│
┃  tanukeys  1  ✗ 1            ┃│                                                                        │
┃  ! old-api                   ┃│                                                                        │
┃                              ┃│                                                                        │
┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛└────────────────────────────────────────────────────────────────────────┘
q quit  ? keys  → open  a add  u update  m minds  l link  U unlink  v validate  g gather  L load  s shelf
every change is a command, printed when you quit; ? lists them
```

Each of a project's rows says what [`flayer status`](#what-each-project-has)
says of it: its state, then a badge for each agent that reads it, in a column
of its own (`CC` for Claude Code, `CX` for Codex, `OC` for OpenCode), blank
where that agent does not. The workspace's own are no project's, and their
row says neither.

`mind` on its own opens the same screen from inside a project: on that project,
if a workspace manages it, or on the project alone if none does, where `l`
links it to the workspace above.

Every key runs a command you could have typed, through the same parser:

| Key | Does | Runs |
|---|---|---|
| `a` | add a skill or a rule | `mind -C <project> add <kind> <name> <description>`; on the `◆` row, `flayer add …` |
| `enter` | show an artifact | `mind -C <project> show <kind:name>` |
| `e` | edit it: the TUI steps aside for your editor | `mind -C <project> edit <kind:name>` |
| `r` | rename it | `mind -C <project> rename <kind:name> <new>` |
| `d` | delete it, once you say yes; on the `◆` row, `e` says yes and takes it out of every project too | `mind -C <project> remove --yes <kind:name>`; on the `◆` row, `flayer remove --yes <kind:name>`, or with `e`, `flayer remove --everywhere --yes <kind:name>` |
| `m` | tick and untick every project under the workspace | `mind -C <path> init`, `flayer link <path>...`, `flayer unlink <path>...` |
| `u` | bring what is installed in the project up to date, or only the artifact under the cursor; on the `◆` row, in every project | `mind -C <project> update --yes [<kind:name>]`; on the `◆` row, `flayer update --all --yes [<kind:name>]` |
| `l` | link a project | `flayer link <path>` |
| `U` | unlink the project | `flayer unlink <path>` |
| `M` | on a stale project the scan found moved, follow it there | `flayer relink <old> <new>` |
| `A` | on a skill somebody keeps in a project's agents' folders, `foreign`, adopt it into the library, once you say yes | `flayer adopt -p <project> --yes <kind:name>` |
| `t` | toggle the project's agents and share mode, then apply | `mind -C <project> agents --yes <agent>...`, `mind -C <project> share --yes local\|shared` |
| `T` | type the folders the project installs skills in, or clear them for where its agents read | `mind -C <project> targets set --yes skills <dir>...`, `mind -C <project> targets reset --yes skills` |
| `v` | validate everything | `flayer validate` |
| `g` | gather from a git repository, showing how far it has got; `esc` stops it, until it starts placing | `flayer gather git <url>` |
| `L` | load another workspace's own skills and rules; enter on a loaded one unloads it | `flayer load <path>`, `flayer unload <path>` |
| `s` | what is on the shelf | `flayer gather list` |
| `w` | the flayers this machine knows, the one in use marked | `flayer workspaces` |
| `i` | install skills and rules from the workspace's own, a loaded workspace, or the shelf; `tab` switches between them, and `v` turns the screen round to every project for one of them | `flayer install` opens it; `a` runs `flayer install -p <project>... --from <origin> <kind:name>` and `flayer uninstall -p <project>... <kind:name>` |
| `R` | read the disk again | nothing: it changes nothing |

A project whose own commands would reach another workspace (linked from
outside the workspace's tree, or with a nearer workspace above it) runs `mind
-W . -C <project> …` instead, so the line acts on the workspace on screen
rather than on the one the project would find by itself. `flayer -W <dir>` opens the screen on the
workspace it names, and every line it runs uses that one; so does an
activated workspace. The header says which it is showing and why: `named
with -W`, `activated`, `found above`, or `default workspace`. It also counts
the projects an older Mindflayer made, still marked by a `mind.toml` or a
`settings.json`, and names the command that moves them, `flayer migrate
--all-projects`, or `mind migrate` for a project on its own
([upgrading](#a-project-made-before-settings)).

`m`, or `flayer minds` straight from the shell, opens a list of every
project the workspace links and every repository under it that it does not,
ticked where it does, in the order `flayer scan --suggest` gives: what is not
linked first, most worth linking first, each saying why, then what is.
`flayer minds ~/Work` looks for those under that folder instead, as
`flayer scan ~/Work` does, and says below the list what it could not read.
Tick and untick, then `a` applies all of it in one go, after saying which
repositories `mind init` will write into. `link` and `unlink` are all or
nothing: if one path is refused, none is changed.

That is the rule it is built by: **nothing the TUI does is out of reach of a
command.** A form shows the line it is about to run as you type, and when the
TUI closes it prints every line that changed something, with what each said,
so a session can be read afterwards, and repeated in a script:

```
$ flayer link tanukeys
linked tanukeys as tanukeys
$ mind -C tanukeys add skill deploy 'Ship the service to staging'
created skill deploy at /home/you/Projects/tanukeys/skills/deploy/SKILL.md
```

Opened where there is no workspace and no default one either (`--home ""`), it
offers to create one there, then goes straight to linking; with a default, it
opens that one and says so. It stays empty, and marked not made yet, until the
first change you make from it, which runs `flayer init` there first. The link
form lists what `flayer scan --suggest` finds under the workspace, or under
the folders `flayer minds` was given (mind projects, and git repositories that
are not one yet), in its order and with its reasons, and picking a repository
asks before running `mind init` in it, because that writes into it. A project
whose directory has gone is listed with a `!`, where `U` drops it, or, when
the scan found it [moved](#following-a-project-that-moved),
`M` follows it; an artifact `validate` would reject is marked `✗`, with the
reason underneath.

## Control panel

`flayer ui` serves a control panel for the workspace from the binary itself,
and opens it in your browser. It runs until Ctrl-C:

```
$ flayer ui
http://127.0.0.1:40185/?token=bf6934705762815d0a708aceb59cb5f9f4fd65466d0f726e0b7a8d3749053513
note: serving the workspace at /home/you/Projects on 127.0.0.1:40185 until Ctrl-C
```

The link is the first line on stdout, and the only one while it serves, for
a script to read; what it prints once it stops (below) follows it there.
`--no-open` prints it without opening a browser, and a browser that cannot
be opened, as on a machine with no display, is a warning, not a failure.
`mind ui` is the same command from inside a project. Either serves the
workspace a command run there would use, decided once as it starts, from
`-C`, `--home`, `-W` and the agent homes, until you switch to another this
machine knows from the header (below): nothing else the browser sends
changes which one.

It listens on 127.0.0.1 only, on a port the system picks, or the one `--port`
names. The link carries a token made for this launch, which the browser
trades for a cookie on its first request, so the token leaves the address
bar. A request without it gets a 401, and a page on another site, or on
another port of your machine, cannot use the cookie to change anything
([what keeps other pages
out](architecture.md#what-keeps-other-pages-out)).

**Everything the panel changes, it changes by running a command**, as the
TUI does. It sends a task; the server turns it into the command lines you
could have typed, shows them before they run, and runs them through the same
parser, one at a time. A command that would open a screen, an editor or a
shell, or read standard input, is refused, and so is a path outside what
the workspace allows, each with a reason and a code. The first Ctrl-C lets
whatever is running finish, a second stops at once, and on the way out it
prints every command that changed something, with what each said, as the
TUI does when it closes:

```
$ flayer -W /home/you/Projects install -p /home/you/Projects/collapse skill:commit-style
installed commit-style into collapse, from workspace
```

**Every change is previewed first.** Whatever you ask the panel to change,
it asks the server what that is before anything runs, and shows you the
lines, exactly as the server will run them and as you could type them in
the workspace's folder: nothing runs until you press Run. Each names the
workspace with `-W` and its path, so it says which workspace it acts on,
and acts on that one whatever your shell has activated. A line that takes
something away makes that a question to confirm, saying what each line
takes in the CLI's own words: `remove --yes`, which deletes an artifact,
`uninstall` and `remove --everywhere`, which take installed copies out of
projects, `unlink` and `unload`, which make the workspace forget a project
or a loaded workspace, and `--force`, which writes over or takes out a copy
changed since it was installed. Which lines those are is the CLI's to say,
not the panel's, and Cancel has the focus, so an Enter pressed by habit
runs nothing. Once a run is over and the pages show what it changed, a toast
says how it went; when it failed, or said a warning, the drawer of what it
said opens, every warning, note and error under the line that said it, with
its code. The panel links projects, from the folder you type above the
projects table or from what **Discover** finds, relinks one that moved,
unlinks one from its page, changes a project's agents and share mode,
migrates a project an older Mindflayer made, installs and uninstalls
skills and rules from **Assign**, brings what is installed up to date and
takes out what nothing offers from **Updates**, removes one of the
library's own everywhere, its dry run first, edits, adds and renames the
library's own, adopts a skill somebody keeps in a project's agents'
folders, loads and unloads other workspaces from **Loads**, and switches
to another workspace from the header, and installs, updates all and links
from the command palette (below) too; everything else is still a command
away.

**The transcript**, in every page's header, lists every task the session
ran, from this tab or any other, each line with what it printed, and
**Copy as script** copies those lines, one to a line, exactly as they ran:
pasted into a shell in the workspace's folder, they do it all again. Each
names the workspace it ran against with `-W`, before a switch or after it,
and reads the rest of its words from the folder it is typed in, as any
line does: the entry an `unlink`, `unload` or `relink` names, or the file
a save reads. Across a switch, paste the lines run before it in that
workspace's folder and those after it in the other's. What `flayer ui`
prints when it stops is the same, but for the tasks that only read, such as
a `validate`.

**The overview** says which skills and rules your projects have at a
glance. It names the workspace, where it is and how it was chosen, and counts
what it holds: the projects it links and the entries that open none, each
folder once, the artifacts of each kind in its projects and in its library,
the installs that are outdated or edited, and what is on the shelf. A count
worth a look links to the projects it is about. Below, everything that could
not be read is listed under the project or the load it is about. A workspace
with nothing linked, nothing of its own or nothing gathered names the
commands that would change that.

**Projects** lists every entry the workspace links: its name, path, agents
and share mode, how many of each kind it holds, how many of what it was
given are `outdated`, a badge linking to **Updates**, its health, and why an
entry that opens nothing does not. The health is one word for the whole project:
`ok`; `attention` when something asks for a command (an install outdated,
edited, missing or orphaned, something one of its agents never reads, a
folder its targets name that no install uses, copies of one name that
disagree, a warning from `validate`); `broken` when something does not read
or does not pass `validate`; and `stale`. Click a column to sort by it, type
to filter by name or path, or show one health alone. The address keeps all
of it, so a link or a reload opens the table as you left it.

**A project's page** groups what it holds by kind. Each artifact says how it
stands, which of the project's agents read it (one that does not is crossed
out), where it came from, where its copies are, and what `validate` says of
it or why its file does not read. What some of its agents never read is a
warning above. A stale entry names `flayer relink` and `flayer unlink`, and a
kind the project holds none of names `flayer install -p` and `flayer add -p`.
Every command the panel names says `-W` and the workspace's path, so it acts
on the workspace the panel shows wherever you type it, for a project linked
from outside that workspace too; a workspace not made yet names `flayer init`
first, since `-W` never makes the one it names. **Unlink**, at the top,
asks first, and says that the workspace only forgets the project: nothing on
disk is deleted. A row that reads `foreign`, somebody's own in a folder an
agent reads, offers **Adopt**, `flayer adopt -p <project> --yes
<kind:name>` ([a repository that already has .claude/skills](#a-repository-that-already-has-claudeskills)), shown before it runs;
what Mindflayer will not adopt, a rule or a skill an install could not have
written, the line refuses, saying why.

Below what it holds, **Agents and sharing** are the project's settings as
toggles: a box for each agent that works in it, `local` or `shared`, and
whether a file git tracks may be edited to reach an agent. **Apply** runs
the lines they amount to, shown first, `mind agents --yes` naming the agents
ticked, those the project names already keeping their order, then `mind
share --yes`, each only when what it sets changes, as the TUI's toggles do;
what is installed is carried to where the agents now read. A project an
older Mindflayer made, marked by a `mind.toml` or a `settings.json`, has no
toggles until it moves to a `settings.toml`, and offers that move instead
([a project made before settings](#a-project-made-before-settings)): **See
what migrating does** runs the dry run, `flayer migrate -p <project>`, and
shows what it would write, move and leave, and only then **Migrate** runs it
with `--yes`. One a `mind.toml` marks takes every agent and stays shared, as
it was committed, and one a `settings.json` marks keeps the agents and the
share mode that file names; the toggles change either once it has moved.

**Discover** finds the projects worth linking. Type the folders your
repositories are kept in, one a line, each an absolute path, and how many
levels down to look, empty for three, and press **Scan**: the panel asks for
what `flayer scan --suggest` would find there, the workspace's own folder
offered to start from. It scans nothing until you ask, and nothing again by
itself while you look, since a scan reads far more of the disk than the
panel watches; a folder is only read. What is not linked comes first, most
worth linking first, each with why in the command's words: a mind project
with an id, one without, a git repository with the files that show an agent
works in it, any other git repository. Tick the ones you want and press
**Link what is marked**: the panel shows the lines first, a `mind -C <path>
init` for each repository that is not a mind project yet, which it names
before you ask since it writes into the repository, then one `flayer link`
of them all, the batch the TUI's minds screen runs for the same marks. The
panel names each folder by its absolute path, where the minds screen names
one inside the workspace by its route from there, so the lines read
differently and act on the same folders.
A project that moved, which the scan tells by its id, the one the workspace
last saw where the entry it left points, offers **Relink** instead, `flayer
relink <old> <new>`, which keeps what was installed into it; an entry whose id the workspace never
learned is named with the `flayer relink` that would follow it once you know
where it went. Below are the projects it found linked already. The folders
and the depth stay in the address, so a reload scans the same again.

**The library** has a page for each kind, `skill` and `rule`, listing the
workspace's own with what each is for, which projects hold a copy of it and
how each copy stands, and what `validate` says of it; type to filter by name
or summary. A kind it holds none of names `flayer add`.
**New skill** and **New rule**, at the top, open a page that adds one: its
name, its description and what it starts from, nothing or one of the
kind's templates ([from a template](#from-a-template)), judged as you type by what
`flayer add` would refuse, in its words, and by what `validate` would say
of it once written. **Add** runs `flayer add [--template <template>] <kind>
<name> <description>`, shown first, and opens what it added.

**One of the library's own** opens from its name. It shows what its front
matter declares, as a table, every key in the order written; its body,
rendered as markdown; every file it holds, each of which opens as the text it
is; what `validate` says of it; and the projects holding a copy, with the
state, agents and targets of each, or the `flayer install` that would put it
in one. What a skill holds is somebody else's, so the panel shows it and
never runs it: HTML in a `SKILL.md` is shown as the markup it is, an image is
named rather than loaded, and only a link to another site or a mail address
stays a link, opened in a tab of its own. No file is ever served as itself,
and a link in the skill that leads out of the folder the library keeps its
kind in is listed with why and never followed.

**Edit**, at the top, opens the file it is read from, its `SKILL.md` or a
rule's file, in an editor in the page, line numbers and markdown with its
front matter coloured, Ctrl-S to save and Tab to leave it. As you stop
typing, the text is judged as `flayer edit --contents` would judge it: why
it would refuse it, in its words (a name other than the skill's folder,
which `rename` changes, front matter that does not read, or a rule the
write is held to), and what `validate` would say of it once written, word
for word as `flayer validate` then prints it. **Save** runs, shown first:

```
$ flayer -W /home/you/Projects edit --contents .mindflayer/tmp/edit-f67e051f60a9675b.md --if-unchanged 8b7db6a4fb1255506c9f5ec0fa7e343f5a9cb6b047a69f44b7aae72f4317439d skill:review
edited skill review: ok
```

The browser sends the text, never a path: the file the line names is the
text, put in the workspace's `.mindflayer/tmp` for the line to read, named
by the first digits of its SHA-256, and taken away once the line has run,
whatever came of it. `--if-unchanged` is the SHA-256 of what the file held
when the editor read it, so a change made to it since, in an editor or from
a terminal, is never written over: the page says the file changed, and the
save is refused with `edit_stale`, the file left as the change left it,
until **Read it again** starts from what it holds now. A copy installed
from it then reads as outdated, for **Updates** to bring along.

Below its files, **Rename it** runs `flayer rename <kind:name> <new name>`,
shown first, and opens it under the name it has then.

At the bottom, **See what removing it everywhere does** runs the dry run of
`flayer remove --everywhere <kind:name>`, which deletes nothing, and shows
what it said: each project's copy that would go, with how it stands, a copy
changed since it was installed that would stay, somebody's own of the name,
which is never touched, and the library's copy. Only then does **Remove
everywhere** run the same line with `--yes`, a confirmation since it deletes
the artifact and uninstalls its copies, and go back to the library. The
panel never removes the library's copy alone, which would leave each copy
installed from it orphaned.

**Loads** lists the workspaces this one loads ([loading another
workspace](#loading-another-workspace)), each with its entry, where it is,
what `--from` takes for what it offers (`load:../team`), why one does not
open, and what it offers, each skill and rule by its name and what it is
for, with a link to **Assign** showing those alone. **Unload** runs `flayer
unload <entry>`, a confirmation, since the workspace then stops offering
them; what was installed from it stays where it is. Below are the
workspaces this machine knows ([the flayers this machine
knows](#the-flayers-this-machine-knows)), the default first, each saying
whether it is the default, this workspace, loaded already and by which
entry, or one that does not open, with how many projects it links. One the
workspace could load, which is none of this one, one it loads already and
one that does not open, offers **Load**, `flayer load <path>`; any other is
loaded by its absolute path, typed below, which the line refuses, saying
why, when no workspace is there or it is this one. Each is shown first, and
is the line the TUI's load form runs for the same workspace.

**The header switches workspace.** The header of every page names the
workspace the panel acts on and how it was chosen: `named with -W`,
`activated`, `found above` or `default workspace`, as the TUI's header says
it. Press it for every workspace you may switch to: those this machine
knows ([the flayers this machine knows](#the-flayers-this-machine-knows)),
the default first, and the one the panel was launched on, each with where
it is, whether it is the default or the one in use, which is checked, how
many projects it links, or why it does not open. Choose one and the panel
acts on it from then on, without a restart: every page shows it, starting
from its overview, and every line previewed names it with `-W`, so its
header then says `named with -W`, but for the one it was launched on,
which keeps how it was chosen then. Nothing is written: a switch is no
command, so the transcript has no line for it, and each line in it still
names the workspace it ran against. Only a workspace on that list can be
switched to, matched by its path as listed: the server refuses any other
path a script sends (`switch_unlisted`), and one that does not open, gone
since the list was read, with why (`not_a_workspace`), never making it. A
task shown in any tab is let go once the panel switched, since its lines
name the workspace before, in a tab hidden then as soon as it is shown
again, and so are the boxes marked on **Assign**; the
switch waits for whatever is running to finish first. Launched with `-W`,
the panel does not read `$MINDFLAYER_WORKSPACE`, as no command does once
`-W` is given, so an activated workspace is listed then only if this
machine knows it.

**The shelf** lists everything gathered, filtered by kind or by name,
summary or origin, each with the commit it was taken at, when, and its
origin as `--from` takes it: the address it came from, with `#` and the
branch when one was asked for. **Sources** lists where the shelf is filled
from, each with that same origin, the branch, the folder harvested, the
commit last gathered and what it supplied, and below them what happened
lately: the latest gathers, installs, uninstalls, relinks and adoptions the
ledger logged, newest first, with every URL in them shown without
credentials. An empty shelf names `flayer gather git`.

**Assign** is a matrix of everything there is to install against every
project: a row for each skill and rule the library, a loaded workspace or
the shelf offers, under a heading for where it comes from, its origin as
`--from` takes it, and a column for each project, or turned round, **by
project**, with the origins over the columns. A box is ticked where the
project holds that offer, as `flayer install` would leave it: of two
places offering one name, the one its install came from, and every one
when somebody else's copy of the name is there. An icon over a box says
how the name stands in the project (`current`, `outdated`, `edited`,
`missing`, `orphaned`, `foreign`), and a box of a kind the project can take
none of, as a project naming no agent, is closed and says why. Type to
filter the offers by name, summary or origin, show one kind or one origin,
or the projects one agent works in; the address keeps all of it, and which
way round it is.

Press a box, or move to it with the arrow keys and press Space, to mark it
the other way round, and again to take the mark away. Whatever is marked
is planned at once, and the plan shows below: the lines that carry it out,
one `flayer install -p ... --from <origin> <kind:name>` for each offer
ticked, with every project ticking it, and one `flayer uninstall -p ...
<kind:name>` for each name unticked, an offer ticked in place of another of
its name replacing it rather than taking it out first; and what it holds
back, each on its box and listed with its code: a box whose name is
somebody's copy (`foreign`) or one changed since it was installed
(`edited`), which only `--force` on the command line reaches, and one name
asked of a project from two places (`wanted_twice`). **Apply** shows the
lines, a confirmation when one uninstalls, and runs them as one batch, the
install screen's; **Clear the marks** forgets them. A matrix of hundreds
of offers and projects draws only what fits on screen.

**Updates** is what `flayer update --all` would do, before it is told
`--yes`: every installation an update says something of, under its state,
`outdated`, `edited`, `missing` and `orphaned`, each counted, with its
project, its origin as `--from` takes it, and what an update does with it:
writes it again from its origin (`refresh`), puts back the copies named
(`put_back`), or leaves it, saying why in core's words and by a code, a copy
changed since it was installed (`edited`), an origin that offers it no more
(`no_longer_offered`), or one installed before origins were kept that two
places offer (`origin_unknown`), which `install --from` settles. An entry
that opens no project is said above. **Update all** runs `flayer update
--all --yes`, each project offers its own `mind -C <project> update --yes`
where an update writes something, and each row its change: **Update**, the
same line naming it; **Force update** for an edited copy, the line with
`--force`, a confirmation since it writes over the change; and
**Uninstall** for one nothing offers any more, `flayer uninstall -p
<project> <kind:name>`, beside **Uninstall every orphaned one**, `flayer
uninstall --all-projects --orphaned`. One nothing offers that was changed
since it was installed, as removing it everywhere leaves one, stands as
`edited`, and neither uninstall takes it out: its row offers **Force
uninstall**, the line with `--force`, a confirmation since the change goes
with it. Each line is shown first, as everywhere in the panel, and the
page is drawn again from what it changed: an edited copy an update left is
still listed, with why.

**The command palette** opens over any page on Ctrl+K, or Cmd+K on a Mac,
and from **Search** at the top of the sidebar. Type to find any page, a
project, one of the library's own, anything on the shelf and the sources it
came from: a word finds what holds it, a letter out of place forgiven, and
each word more narrows what the others found, the group holding the nearest
first. A search lists its twelve nearest, and with nothing typed each group
lists its first twelve, the projects to install into among them: type
enough to narrow it to the rest. Beside each is what it is, a project's
health, a skill's or a rule's kind, and its words are shown as written,
never as markup. Choosing one goes where its link on a page goes. It
changes things too, each as the page offering the same change does:

- **Install** one skill or rule, from wherever it is offered, then one of
  the projects whose box of it on **Assign** is neither ticked nor closed.
  That one box is planned, as ticking it would plan it, and what the plan
  says runs, `flayer install -p <project> --from <origin> <kind:name>`,
  replacing another of its name the project holds; what holds it back, a
  copy somebody wrote (`foreign`) say, is said with its code, and nothing
  runs.
- **Update all**, `flayer update --all --yes`, the button of **Updates**,
  offered whatever an update would do, which its line then says.
- **Link**, offered last for whatever is typed, `flayer link <path>`, the
  line of the form above the projects table, which refuses, saying why,
  anything but the absolute path of a folder.

Each shows its lines before it runs, as every change does. Escape, or Ctrl+K
again, closes the palette, and the focus goes back where it was; Backspace,
with nothing typed, goes back from the projects to install into.

**`?` lists the keyboard shortcuts**, every key the panel answers:

| Keys | Where | What they do |
|---|---|---|
| Ctrl+K, or Cmd+K on a Mac | anywhere | open or close the command palette |
| `?` | anywhere | show or hide the keyboard shortcuts |
| ↑ and ↓, Enter, Backspace, Esc or Ctrl+K | the command palette | move, choose, go back from the projects to install into, close |
| Tab, Enter, Esc | a change asked about | move between Cancel and Run, press the one focused, cancel with nothing run |
| the arrows; Page Up and Page Down | **Assign**'s matrix | move one box; ten rows |
| Home and End; Ctrl+Home and Ctrl+End | **Assign**'s matrix | the first or the last box of the row; of the matrix |
| Space or Enter | **Assign**'s matrix | mark the box the other way round, or take its mark away |
| Ctrl+S, or Cmd+S on a Mac | the editor | save, its line shown first |
| Tab | the editor | leave the editor |

Neither Ctrl+K nor `?` is taken while a field or the editor has the focus,
whose keys they stay (`?` is a character typed there), but for the
palette's own search, which Ctrl+K closes, nor while a change is shown or
runs, whose question is answered first. Where the browser gives Ctrl+K a
use of its own, such as its search bar, the panel takes it while its page
has the focus outside a field, or the palette is open.

**It follows the disk.** A change made from a terminal, the TUI or an
editor shows in the panel within a moment, without a reload: a skill edited,
a project linked, an install. The server watches what the workspace's
snapshot reads, its marker, its ledger, its library and its shelf, and in
each project its marker, the files its agents read and the folders it keeps
skills and rules in, each folder on its own and never a whole repository,
and tells every open tab when something there changed, which then reads the
workspace again. When the system will not let it watch everything, which on
Linux is the limit `fs.inotify.max_user_watches` sets, each page says so
above what it shows, with a button that reads everything again.

Every page works from the keyboard, its keys listed on `?`, and says every
state in words, never by colour alone. The API under it is there for a script holding the token as a
Bearer token: `POST /api/query` for a read (`{"type": "snapshot"}` answers
what the panel draws, `{"type": "files", ...}` what one artifact holds,
`{"type": "contents", ...}` one file of it, as text, `{"type": "scan",
"roots": ["/home/you/Projects"], "depth": null}` what Discover lists, and
`{"type": "plan", "selection": [{"project": "alpha", "reference":
"skill:deploy", "origin": "workspace", "wanted": true}]}` what boxes of a
project and an offer, ticked or not, amount to: the tasks that carry them
out, beside what clashes and what is locked, with nothing run, and
`{"type": "workspaces"}` what the header may switch to), `POST
/api/commands/preview` for the lines a task is and what of them the panel
asks you to confirm (`{"lines": [...], "confirm": [...]}`), `POST
/api/commands` to run them, which asks nobody, `POST /api/switch` with
`{"path": "/home/you/Notes"}` to switch to a workspace that list holds,
answered with its row, `GET
/api/transcript` for everything the session ran, and `GET /api/events` for
what happens as it happens, as server-sent events: `resync`, to read
everything again, first and whenever a reader fell behind; right after it
`watching`, or `unwatched` with a `reason` when the server cannot watch
everything, and either again whenever that changes; `changed` when something
watched changed; `command_started` and `command_finished` around each
run; and `switched` once the panel acts on another workspace, to read
everything again.

The panel is built into the binaries by a cargo feature, `ui`, which is on by
default. `cargo build -p mindflayer-cli --no-default-features` leaves it out,
and with it the server and what only the panel needs (axum, the embedded
files, the browser opener); `ui` is then no command.

## Where a project keeps its artifacts

`.mind/settings.toml` is the marker and the configuration; the artifacts
themselves sit beside the code, because these are files the **agents** read
and an agent does not know what a `.mind` is. `mind init` writes it:

```toml
# Mindflayer mind project: what Mindflayer needs to know about the project
# this .mind is in, and nothing else. No agent reads it.
#
# `name` is what the project is shown as, and `agents` the agents that work
# in it (claude, codex, opencode): what is installed into the project is
# copied where each of them reads. `mind agents` changes them.
#
# `share` is "local" to keep .mind, and what is installed, out of git
# for this checkout alone, or "shared" to leave both for the team to
# commit. `edit_tracked` lets a file git tracks, such as AGENTS.md, be
# edited to reach an agent in a local project. `mind share` changes both.
#
# `directories` says where the project keeps the skills and rules it writes
# itself, relative to the directory holding .mind, to commit with its
# code; a kind left out has no folder. A `[targets]` table, when there,
# names the folders a kind is installed in instead of where the agents
# read: `mind targets` writes it.
#
# `id` names the project for good, wherever it moves, and `version` is how
# this file is laid out: change neither.
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

It is TOML, as the workspace's `flayer.toml` is, and its comments say what
each key does. `agents` are the agents that work in the project, and so
where what is installed into it goes (`--agents` names them,
comma-separated); `share` says whether that is kept to this checkout or left
for the team to commit; `edit_tracked` whether a file the team tracks may be
edited to reach an agent; `directories` where the project keeps the skills
and rules it writes itself; and the `id` names the project wherever it
moves. A project that installs somewhere other than where its agents read
says where in `targets` too ([installing somewhere
else](#installing-somewhere-else)). Edit it by hand if you like:
`mind agents`, `mind share` and `mind targets` change only the keys they set,
and keep your comments, your layout and any key Mindflayer does not know.
Point the directories wherever the agents already look, and Mindflayer
follows:

```bash
mind init --skills .claude/skills
mind init --skills docs/skills --rules docs/rules
```

Both are relative to the project, and have to stay inside it: the artifacts
are the project's own, so `--skills /etc/skills` is refused rather than made.
So is a folder inside `.mind`, such as `--skills .mind/skills`: a local
project's `.mind` keeps itself out of git (below), and would take the
project's own skills with it.
`init` never overwrites an existing marker, so a second `mind init --skills
elsewhere` changes nothing and says so: moving a project's artifacts is not
something `init` should do behind your back.

The folders are written out even when they are the defaults, so the answer to
"where do this project's skills go" is in the file rather than in somebody's
memory. A kind `directories` leaves out has no folder at all, rather than the
default.

`.mind` keeps itself out of git, as a Python virtualenv does: `mind init`
writes a `.mind/.gitignore` holding `*`, so `git status` never offers the
settings and your own `.gitignore` needs no line for them. The skills and
rules beside the code are not kept out: they are the project's, to commit
with it. `mind init --shared` writes no `.gitignore` and sets `share` to
`shared`, for a team that commits its `.mind`, and what is installed into the
project with it, so that every clone gets the same.

A project an older Mindflayer made is marked by `.mind/mind.toml` instead,
which names the same folders in a table and was committed with the project,
or by `.mind/settings.json`, which holds these same keys in JSON. Mindflayer
still reads either the way it was written, and `mind init` leaves it as it
is. When a project holds more than one, it reads `settings.toml`, then
`settings.json`, then `mind.toml`. `mind migrate` moves it to a
`settings.toml` when you ask, and until then its settings are not changed
([upgrading](#a-project-made-before-settings)).

A project is listed from the folders its agents read as well (one an older
Mindflayer made, from the one folder its `mind.toml` names):
`.claude/skills` for Claude, `.agents/skills` for Codex and OpenCode. A skill
there is listed once, whoever put it there, and once with the project's own of
that name, which it reads as; when it sits in both folders with different
contents `list` and `validate` warn, since the agents would not all load the
same skill. Mindflayer changes only what sits in the folders the project
authors in, so `mind edit`, `rename` and `remove` reach the project's own copy
and leave a skill in an agent's folder alone.

What is installed into a project goes where its agents read, never into the
folder it authors in (one an older Mindflayer made takes it in that folder):
one copy in `.claude/skills` for Claude, one in
`.agents/skills` that Codex and OpenCode share, the same bytes, put in place
together or not at all, and recorded as one installation. A local project
keeps them out of git with a block of its own in the repository's
`.git/info/exclude`, never in your `.gitignore`, written before the copies
land and taken out after they go; a shared one leaves them for the team to
commit:

```
$ flayer install -p collapse commit-style
installed commit-style into collapse, from workspace
$ tail -5 collapse/.git/info/exclude
# mindflayer:begin
# project 4f9c2a7e1b3d5f60a8c4e2b6d0f1a3c5
/.agents/skills/commit-style/
/.claude/skills/commit-style/
# mindflayer:end
```

Outside a git repository a local install says nothing keeps its copies out
of git, as a `note:`. A skill's own `.claude-plugin` folder goes into no copy,
with a warning, since Claude would load the skill as a plugin, hooks and MCP
servers included. And a project at your home is refused an install, since its
`.claude/skills` and `.agents/skills` are the ones every project reads.

A rule goes into the block in the project's `AGENTS.md` instead ([the two
kinds](#the-two-kinds)), rendered there with every other rule the project
has. An `AGENTS.md` Mindflayer makes for it is kept out of git by the same
block in a local project, and goes with the last rule; one that was there is
yours, and keeps everything outside the block. A local project writes into
one git tracks only with `edit_tracked` (`mind share local --edit-tracked`):
without it, Claude gets each rule from a file of its own in
`.claude/rules/mindflayer`, and the install warns that Codex and OpenCode read
none of them. A project an older Mindflayer made is given a copy of the rule
in the folder its `mind.toml` names, filed by its route.

### Changing its agents, or how it shares

When a project starts using Codex, stops using Claude, or moves between local
and shared, what is installed into it follows. `mind agents` changes the
agents it names (`mind agents claude codex` names exactly those, `add` and
`remove` change the ones it names), and says first what that does:

```
$ mind agents add codex
would set agents to claude,codex in collapse, where they were claude
would copy commit-style to .agents/skills/commit-style, for codex, kept out of git
nothing changed: add --yes to go ahead
$ mind agents add codex --yes
set agents to claude,codex in collapse, where they were claude
copied commit-style to .agents/skills/commit-style, for codex, kept out of git
$ mind agents remove claude --yes
set agents to codex in collapse, where they were claude,codex
removed commit-style from .claude/skills/commit-style
left .claude/skills/notes where it is: Mindflayer did not install it
```

Every installation is brought to where the agents the project names now
read: a folder a new agent reads gets a copy of one already there, the same
bytes, and a folder no agent it names reads any more loses Mindflayer's
copy. Only a copy nobody has changed since it was installed is taken out:
one that was edited is left where it is and becomes yours, and somebody
else's skill is never touched, wherever it is. One where a new copy would go
is left alone with a warning, since that agent then reads it rather than
what was installed; one in a folder no agent reads any more is left and
said, as `notes` is above. Running the same command again changes nothing
once it is done. A project has to name at least one agent, so removing the
last is refused, and so is a project an older Mindflayer made, marked by a
`mind.toml` or a `settings.json`, which `mind migrate` moves to a
`settings.toml` first. Its rules follow later, at the next install or
uninstall of one, which renders every rule the project has for the agents
and the share mode it has then.

`mind share` sets the share mode, and `edit_tracked` with it, off unless
`--edit-tracked` is given:

```
$ mind share shared --yes
set share to shared in collapse, where it was local
removed .mind/.gitignore, so .mind can be committed
stopped keeping /.agents/skills/commit-style/ out of git
```

Turning shared removes `.mind/.gitignore`, if it is still the one
Mindflayer wrote, and the project's whole group in `.git/info/exclude`, so
the settings and the copies are the team's to commit; turning local writes
both back, an entry for each copy installed that git does not track: one the
team committed stays in git. `flayer agents -p collapse ...` and
`flayer share -p collapse ...` do the same from the workspace, and `t` in
[the TUI](#the-tui) toggles them for the project under the cursor. A project
no workspace links still changes its settings, which are its own, and is told
that nothing installed in it was carried along.

### Installing somewhere else

The folders each agent reads are the default, since that is where every
agent finds a skill without being told. A project that keeps one folder of
skills for every agent says so in its settings, and what is installed into it
goes there instead, one copy for all of them:

```
$ mind targets set skills .skills
would set the targets of skills to .skills in collapse, where they were the folders its agents read
would copy commit-style to .skills/commit-style, for claude, codex and opencode, kept out of git
would remove commit-style from .agents/skills/commit-style
would remove commit-style from .claude/skills/commit-style
nothing changed: add --yes to go ahead
note: claude, codex and opencode would read none of the folders collapse would install skills in (.skills), so none of them would see what is installed there until pointed at one
$ mind targets set skills .skills --yes
set the targets of skills to .skills in collapse, where they were the folders its agents read
copied commit-style to .skills/commit-style, for claude, codex and opencode, kept out of git
removed commit-style from .agents/skills/commit-style
removed commit-style from .claude/skills/commit-style
note: claude, codex and opencode read none of the folders collapse installs skills in (.skills), so none of them sees what is installed there until pointed at one
```

The settings then end in a `[targets]` table holding `skills = [".skills"]`,
and name as many folders as you give it, each relative to the project. A
folder that leads out of the project, an absolute one, or one inside `.mind`
or `.git` is refused, and one written there by hand is given no copy and said
by `status`, by an install and in the TUI. Copies move as they do for [a
change of agents](#changing-its-agents-or-how-it-shares): only Mindflayer's
untouched ones, never over somebody's, and a dry run first.

No agent reads `.skills` until it is pointed there, so `status` warns until
it is. A link from a folder the agent reads is one way, and one the report
sees:

```
$ mind status
commit-style  current  -  workspace
warning: claude, codex and opencode read none of the folders collapse installs skills in (.skills), so none of them sees what is installed there until pointed at one
$ rm -r .claude/skills .agents/skills
$ ln -s ../.skills .claude/skills && ln -s ../.skills .agents/skills
$ mind status
commit-style  current  claude,codex,opencode  workspace
```

Every Mindflayer that reads a `settings.toml` reads `targets` too, so
setting them changes no version: a build from before them finds no project
in a `settings.toml` at all, rather than installing into the standard
folders behind your back. `mind targets reset` sends the kind back to where
each agent reads, and takes the table out. With the links taken out again,
and somebody's `notes` kept in `.skills`, it leaves `notes` where it is:

```
$ mind targets reset skills --yes
set the targets of skills to the folders its agents read in collapse, where they were .skills
copied commit-style to .agents/skills/commit-style, for codex and opencode, kept out of git
copied commit-style to .claude/skills/commit-style, for claude, kept out of git
removed commit-style from .skills/commit-style
left .skills/notes where it is: Mindflayer did not install it
```

`flayer targets -p collapse ...` does the same from the workspace, and `T` in
[the TUI](#the-tui) types the folders for the project under the cursor.

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

Anything else in the front matter (an agent's own keys, like Claude Code's
`paths`) is carried along untouched, and so is a `metadata` entry or a
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
agent is given. Front matter that does not parse (prose opening with `**`
does not either) is read as content, fences and all, and `validate` warns
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

Each writes the least that passes `validate` (a skill's front matter and a
heading, a rule's opening line) in the directory the project keeps that kind
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
`.mindflayer/templates/` for every project it manages: a folder holding a
`SKILL.md` for a skill, a markdown file for a rule. `mind templates` lists what
is available and where each comes from; of two with one name, the closer wins.
`flayer add -p` finds the workspace's wherever the project sits; `mind add`
finds those of [the workspace named or
activated](#choosing-a-workspace), or else the one above the project, or else
the one in your home.

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
mind edit deploy --contents draft.md
                              # what draft.md holds, checked first
mind rename deploy ship       # the folder and the `name` inside, together
mind remove ship              # says what would be deleted, deletes nothing
mind remove ship --yes        # deletes it
```

All three find an artifact by what it declares, or else by where its name puts
it, so a skill whose front matter does not parse, which no listing shows, can
still be opened and fixed. `edit` checks the file once the editor closes, and
says what it broke. `rename` rewrites the `name` line and nothing else: the
comments, the key order and the description stay as they were. `remove`
without `--yes` is a dry run, because it is the one thing here that cannot be
taken back.

With `--contents`, `edit` opens no editor: what the file holds, or standard
input with `-`, takes the place of the skill's `SKILL.md` or the rule's
file, the rest of a skill's folder left as it is. It is checked first, so
what would break the artifact is never written: a skill's front matter has
to read, with a name and a description `add` would take, and the name has
to be its folder's, since `rename` is what changes the two together; a rule
has to say something, and its front matter, if it has any, to read whole.
`--if-unchanged` takes the SHA-256 of what the file held when the contents
were made from it, its 64 hex digits alone, which `sha256sum` prints
before the file's name (`sha256sum FILE | cut -d' ' -f1` prints them
alone), and refuses to write over a change made since; a value that is no
SHA-256 is refused as one:

```
$ mind edit deploy --contents draft.md
edited skill deploy: ok
$ mind edit deploy --contents renamed.md
error: the contents given for `deploy` call it `ship`: a skill's name is its folder's too, so it is changed with `mind rename`, which moves both, and nothing was written
```

The file is replaced through a temporary renamed onto it, and only where
the project keeps that kind, inside the project: a `SKILL.md` that links
out of its skill's folder or the skills folder is refused rather than
written through, and so is one under a folder linked out of the project,
as `.claude` can be. The workspace's own are held to the folder it keeps
them in, which may be linked elsewhere.

What `flayer install` put in an agent's folder is not changed from the
project: `edit`, `rename` and `remove` refuse a skill found only where its
agents read, which `mind uninstall` takes out. Where a copy sits in a folder
the project writes in too (one whose `--skills` is an agent's folder, or a
project an older Mindflayer made, whose one folder takes the copy), the
change reaches it. Edited or renamed, it stays Mindflayer's install, and
[`status`](#what-each-project-has) says it is edited: no install writes over
it and no uninstall takes it out unless forced. A rule renamed in a project
a `mind.toml` marks is moved as it is, so it still holds what was installed,
and `uninstall` takes it out unforced, as `rename` says. Removed, it is uninstalled,
so a skill you write later under the same name does not inherit a record
that was about something else. In `app`, made with `mind init --skills
.claude/skills` and linked:

```
$ mind install commit-style
installed commit-style into app, from workspace
$ mind rename commit-style commits
renamed skill commit-style to commits at /home/you/Projects/app/.claude/skills/commits
  it was installed by `flayer install`, and stays so under its new name; renamed, it is left alone by `flayer install` and `flayer uninstall` unless they are forced
$ mind status
commit-style  current  codex,opencode   workspace
commits       edited   claude,opencode  workspace
$ mind remove commits --yes
removed skill commits from /home/you/Projects/app/.claude/skills/commits
  it had been installed, and is uninstalled with it; `flayer install` can put it back
```

The copy in `.agents/skills`, which the rename did not reach, is still the
install as it was. The record changed is the one held by the workspace the
command works in: `flayer`'s, wherever the project sits, or for `mind` the
one named or activated, or else the workspace above the project, or else the
one in your home. Another workspace that links the same project keeps its
own (an [open question](architecture.md#open-questions)).

## Upgrading

A newer Mindflayer reads what an older one wrote exactly as it was written. It
changes a marker only when you tell it to, and the database only after
copying it aside ([what a workspace keeps](#what-a-workspace-keeps)).

### A workspace made before the library

`flayer init` used to keep a workspace's own skills and rules in `skills/` and
`rules/` beside its projects: `~/skills` and `~/rules`, for the workspace in
your home. Such a workspace still works, because its `flayer.toml` names those
folders, and keeps them there until you move them into its library. On its own
the command changes nothing and says what `--yes` would do:

```
$ flayer migrate --library
would move /home/you/skills/commit-style to /home/you/.mindflayer/library/skills/commit-style
would move /home/you/rules/git/no-force-push.md to /home/you/.mindflayer/library/rules/git/no-force-push.md
would point skills and rules in /home/you/.mindflayer/flayer.toml at the library, keeping it as it is in /home/you/.mindflayer/flayer.toml.v2
nothing moved: add --yes to go ahead

$ flayer migrate --library --yes
moved /home/you/skills/commit-style to /home/you/.mindflayer/library/skills/commit-style
moved /home/you/rules/git/no-force-push.md to /home/you/.mindflayer/library/rules/git/no-force-push.md
pointed skills and rules in /home/you/.mindflayer/flayer.toml at the library, kept as it was in /home/you/.mindflayer/flayer.toml.v2
```

Each artifact is moved as it is, a skill whose front matter is broken
included, and a folder it leaves empty goes with it, so your home keeps no
empty `~/skills`. Only the values in `[directories]` change; the comments and
everything else in `flayer.toml` stay as they were. Run again, it has nothing
to do and writes nothing:

```
$ flayer migrate --library --yes
nothing to move: the workspace keeps its own in its library already
```

What you installed into a project from what moved stays installed, and
current: an install records that it came from `workspace`, not from which
folder, so `flayer install` still finds the project's copy the same as the
skill it came from.

Nothing moves while anything is in the way, and every reason is said at once:
something already where an artifact would go, a `flayer.toml.v2` an earlier
migration kept, a link whose target is relative to where it is, or a folder
that cannot be read:

```
$ flayer migrate --library --yes
error: nothing moved: /home/you/.mindflayer/library/skills/commit-style: already there, so /home/you/skills/commit-style cannot be moved onto it
```

Nothing moves either while a folder it would empty is not the workspace's:
one `flayer.toml` names outside the workspace or on its shelf, which `flayer
init` would have refused, or one that is a link, or is reached through one, as
`~/skills -> dotfiles/skills` is, which keeps its skills in your dotfiles on
purpose.

A project's artifacts are never moved. When your home is itself a project
keeping its skills in `~/skills` too, those are the project's: they stay where
they are, the command says so, and the workspace's own skills start empty in
its library. If you never linked your home, the workspace counted those skills
as its own until then, and the line says it no longer does: one you installed
from there stays installed and recorded, so `flayer uninstall` still takes it
out, but nothing offers it to `flayer install` any more. A move that fails puts back what had moved, and leaves
`flayer.toml` as it was. To undo a migration that worked, put `flayer.toml.v2`
in place of `flayer.toml` and move the artifacts back.

### A project made before settings

A project an older Mindflayer made is marked by `.mind/mind.toml`, which
names the folders it authors in and nothing else: no agents, so whatever was
installed into it went into its `skills/` beside its own, and no share mode,
since its `.mind` was committed by design. It keeps working that way until
you move it to a `settings.toml`. On its own the command changes nothing and
says what `--yes` would do:

```
$ mind migrate
would write .mind/settings.toml for collapse: agents claude,codex,opencode, share shared
would move .mind/mind.toml to .mind/tmp/mind.toml.v2, as it is
would copy commit-style to .agents/skills/commit-style, for codex and opencode
would copy commit-style to .claude/skills/commit-style, for claude
would remove commit-style from skills/commit-style
would leave skills/deploy where it is: the project's own
nothing changed: add --yes to go ahead
$ mind migrate --yes
wrote .mind/settings.toml for collapse: agents claude,codex,opencode, share shared
moved .mind/mind.toml to .mind/tmp/mind.toml.v2, as it was
copied commit-style to .agents/skills/commit-style, for codex and opencode
copied commit-style to .claude/skills/commit-style, for claude
removed commit-style from skills/commit-style
left skills/deploy where it is: the project's own
```

The settings keep the name and the folders `mind.toml` gave, name every
agent unless `--agents` says fewer, and leave the project shared, as it was
committed, unless `--local`. `mind.toml` goes to `.mind/tmp/mind.toml.v2`,
which the folder's own `.gitignore` keeps out of git even in a shared
project. What the workspace installed is copied where each agent reads, the
same bytes, and taken out of `skills/`, but only while it still holds what
was installed: a copy you edited stays where it is, becomes yours and is
said, as what you wrote yourself is. A copy already where an agent reads,
in a project whose `mind.toml` kept its skills in `.claude/skills`, stays
where it is, and somebody's skill where a new copy would go is never
written over. Run again, it has nothing to do:

```
$ mind migrate --yes
nothing to migrate: collapse is marked by its settings.toml already
```

What `mind migrate` carries along is what the workspace it finds records,
walking up from the project. In a project linked from a workspace beside
it, which walking up never reaches, it carries nothing and says so in a
`hint:`; `-W` names that workspace, as for any `mind` command.

`flayer migrate -p tanukeys` does the same from the workspace, and `flayer
migrate --all-projects` does it for every project the workspace links that a
`mind.toml` or a `settings.json` marks, each on its own: without `--yes` it
lists every one and writes nothing, and with it one that cannot be moved is
said and the others go ahead. `flayer migrate` alone says which of the three
to ask for, [the TUI](#the-tui)'s header counts the projects still marked
by either, naming the command, and [`status`](#what-each-project-has) gives
each one a hint.

`--local` keeps `.mind` and the copies out of git, as `mind init` does for
a new project. A `.gitignore` keeps out only what git does not track yet,
though, and a `.mind` that was committed stays tracked, so the command says
how to stop that and leaves it to you:

```
$ flayer migrate -p tanukeys --local --agents claude --yes
wrote .mind/settings.toml for tanukeys: agents claude, share local
moved .mind/mind.toml to .mind/tmp/mind.toml.v2, as it was
wrote .mind/.gitignore, keeping .mind out of git
copied commit-style to .claude/skills/commit-style, for claude, kept out of git
removed commit-style from skills/commit-style
hint: git tracks /home/you/Projects/tanukeys/.mind, and a .gitignore keeps out only what git does not track; `git rm -r --cached .mind` in /home/you/Projects/tanukeys stops that, and leaves the files where they are
```

A project is moved whole or not at all: one that something stops part way,
a folder that cannot be written, say, has what was done put back, and is
still marked by its `mind.toml`. Nothing is moved while there is a reason
not to, every reason said at once: a `mind.toml.v2` an earlier migration
kept, or a project from the first Mindflayer that keeps its skills inside
`.mind`, where settings cannot name them. Move them out and name their
folder in `mind.toml`, and such a project moves. Nor while the workspace
that installed into it cannot be read, since what it records is what says
which copies to carry.

A project a `.mind/settings.json` marks has its settings already, in the
JSON Mindflayer wrote them in before TOML. It is read as it was written,
installs go where its agents read, and nothing changes its settings until it
moves: `mind agents`, `mind share` and `mind targets` point at
`mind migrate`, which writes the same settings as a `settings.toml` and
keeps the `settings.json` as it was in `.mind/tmp`:

```
$ mind migrate
would write .mind/settings.toml for collapse: agents claude,codex,opencode, share local
would move .mind/settings.json to .mind/tmp/settings.json.v3, as it is
nothing changed: add --yes to go ahead
$ mind migrate --yes
wrote .mind/settings.toml for collapse: agents claude,codex,opencode, share local
moved .mind/settings.json to .mind/tmp/settings.json.v3, as it was
```

No copy moves, since the agents, the share mode and the folders are the
ones the file named, so the workspace's records are not needed for it.
`--agents`, `--local` and `--shared` are what a `mind.toml` is given, and a
key Mindflayer does not know stays in the copy kept. A `settings.json.v3` an
earlier migration kept refuses the move, as a `mind.toml.v2` does.
