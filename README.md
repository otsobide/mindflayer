# 🧠 Mindflayer

Manage the **skills** your coding agents use, the way git manages code: a
project carries a `.mind` directory holding its skills, and a workspace above
it carries a `.mindflayer` that orchestrates several such projects at once.

One shared Rust engine (`mindflayer-core`) behind every front end. The first
front end is the `mind` CLI; the layout leaves room for a desktop app later
without moving the engine.

> **MVP scope.** Mindflayer is being built incrementally. Today it creates the
> two kinds of directory and reads the skills inside them. Creating, moving and
> syncing skills between projects is the next step, not a shipped feature.

## The two levels

| | Marker | Holds | Think of it as |
|---|---|---|---|
| **Mind project** | `.mind/mind.toml` | skills, in `.mind/skills/<name>/SKILL.md` | a repository's `.git` |
| **Flayer workspace** | `.mindflayer/flayer.toml` | references to mind projects | the directory your repos sit in |

A mind project is meant to be committed: `.mind` travels with the code it
describes, so whoever clones the repository gets its skills. A flayer workspace
is the level above, where several projects are managed together.

Both are found the way git finds a repository: by walking up from wherever you
are until the marker file appears.

## Getting started

From a fresh clone, one command builds the binary and puts it on your PATH:

```bash
make dev/link                  # `mind` now points at target/debug/mind
```

It is a **symlink**, not a copy, so every later `make build` updates the `mind`
you are running without reinstalling anything. `make dev/unlink` takes it back
off. If you only want to use the tool rather than work on it, `make install`
puts a real copy in cargo's bin directory instead.

Then:

```bash
cd ~/Projects/collapse
mind init                      # a .mind here, with an empty .mind/skills

cd ~/Projects
mind init flayer               # a .mindflayer, to manage several of them
```

`init` never overwrites an existing marker. Run it twice and it says so and
changes nothing, so it is safe in a script.

## Commands

```bash
mind init [mind|flayer]    # create a project (default) or a workspace
mind list                  # every skill in scope
mind show <name>           # one skill: metadata, path, instructions
mind validate [<name>]     # check skills against what an agent requires

mind ls                    # alias for list
mind -C <dir> ...          # work in <dir> instead of the current directory
```

**What "in scope" means.** From inside a mind project you get that project's
skills. From inside a flayer workspace you get every project it references, and
the one you are standing in if it is not registered yet.

```
$ mind list
collapse  commit-style  How this repo writes commit messages and branch names
tanukeys  ddd-reviewer  Review a change against the DDD layering rules

$ mind validate
commit-style (collapse): ok
ddd-reviewer (tanukeys): 1 problem
  - `name` is `ddd-reviewer` but the directory is `ddd-review`; they have to match

2 skills checked, 1 invalid
```

`validate` exits non-zero when anything is wrong, so it works in CI. A skill
that cannot be read at all is a warning on stderr and does not hide the ones
next to it.

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

`name` and `description` are required; `name` has to match the directory, be
kebab-case and stay under 64 characters, and `description` under 1024. Those
are the rules `mind validate` checks. `allowed-tools` accepts either a comma
separated string or a YAML list. Anything else in the front matter is carried
along untouched.

## Repository layout

Every unit of the product is an app under `apps/`, so adding a front end means
adding a directory rather than reshaping the tree:

```
apps/core   mindflayer-core — projects, workspaces, skills. No I/O beyond files.
apps/cli    mindflayer-cli  — the `mind` binary. Parsing and rendering only.
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
make cli/run ARGS="init flayer"
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
