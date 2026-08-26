# 🧠 Mindflayer

Manage what your coding agents read, the way git manages code: a project
carries a `.mind` directory holding its **skills** and **rules**, and a
workspace above it carries a `.mindflayer` that orchestrates several such
projects at once.

One shared Rust engine (`mindflayer-core`) behind every front end. The first
front end is the CLI — `mind` for a project, `flayer` for a workspace — and the
layout leaves room for a desktop app later without moving the engine.

> **MVP scope.** Mindflayer is being built incrementally. Today it creates the
> two kinds of directory, registers projects with a workspace, and reads the
> skills inside them. Creating and moving skills is the next step, not a
> shipped feature.

## The two levels

| | Marker | Holds | Think of it as |
|---|---|---|---|
| **Mind project** | `.mind/mind.toml` | its artifacts, under `.mind/` | a repository's `.git` |
| **Flayer workspace** | `.mindflayer/flayer.toml` | references to mind projects | the directory your repos sit in |

A mind project is meant to be committed: `.mind` travels with the code it
describes, so whoever clones the repository gets its skills and rules. A flayer
workspace is the level above, where several projects are managed together.

## The two kinds

| | Lives in | Shape | Declares |
|---|---|---|---|
| **Skill** | `.mind/skills/<name>/SKILL.md` | a directory each, so it can carry scripts and references beside its instructions | front matter: `name`, `description`, optional `allowed-tools` and `license` |
| **Rule** | `.mind/rules/<name>.md` | one markdown file each | nothing — it is context, and its name is its filename |

Folders under `rules/` group and mean nothing else, so
`.mind/rules/git/no-force-push.md` is the rule `git/no-force-push`. Skills are
flat, because a skill's directory already belongs to it.

Both are found the way git finds a repository: by walking up from wherever you
are until the marker file appears.

## Getting started

From a fresh clone, one command builds the binary and puts it on your PATH:

```bash
make dev/link                  # `mind` and `flayer` now point at target/debug
```

They are **symlinks**, not copies, so every later `make build` updates the
binaries you are running without reinstalling anything. `make dev/unlink` takes
them back off. If you only want to use the tools rather than work on them,
`make install` puts real copies in cargo's bin directory instead.

Then:

```bash
cd ~/Projects/collapse
mind init                      # a .mind here, with an empty .mind/skills

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
mind init                  # create a .mind here
mind list [KIND]           # this project's artifacts; `mind list rules` filters
mind show <NAME>           # one artifact; `rule:deploy` when a name is ambiguous
mind validate [KIND|NAME]  # check a kind, one artifact, or everything

flayer init                # create a .mindflayer here
flayer link <path>         # register a mind project with it
flayer unlink <path>       # drop one
flayer list [KIND]         # every artifact across every registered project
flayer show <NAME>
flayer validate [KIND|NAME]

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

$ flayer list                              # the workspace above it
collapse  skill  commit-style       How this repo writes commit messages
tanukeys  rule   git/no-force-push  Never force-push a shared branch
```

A column appears only when it tells you something. The kind column is absent
when only one kind is in play, the project column when only one project is —
the same rule in both cases.

A name is bare until it needs qualifying. When one name belongs to two kinds,
`mind show deploy` shows both and `mind show rule:deploy` picks one.

The qualifier is a **colon**, not a slash, because a rule's name is already a
route: `.mind/rules/skills/naming.md` is the rule `skills/naming`, and a slash
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
skill/commit-style: ok
rule/git/no-force-push: ok

1 skill and 1 rule checked, 0 invalid
```

Each kind is checked against what it actually declares. A skill's name has to
match its directory and its description has to fit; a rule declares nothing, so
the only things left to check are that its name is usable and that the file is
not empty.

`validate` exits non-zero when anything is wrong, so it works in CI. An
artifact that cannot be read at all is a warning on stderr and does not hide
the ones next to it.

## What a skill looks like

`.mind/skills/commit-style/SKILL.md`:

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

A rule is simpler, because it declares nothing at all. `.mind/rules/git/no-force-push.md`:

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
if it is missing. Note that `make clean` deletes `target/`, which leaves the
`dev/link` symlink dangling until the next build.

`BINDIR` says where the symlink goes, if `~/.cargo/bin` is not where you want
it:

```bash
make dev/link BINDIR=~/.local/bin
```

## License

GPL-3.0-only. See [LICENSE](LICENSE).
