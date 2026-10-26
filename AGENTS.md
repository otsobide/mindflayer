# AGENTS.md

Guidance for coding agents working in this repository.

Mindflayer manages the skills and rules coding agents read, the way git manages
code. A **mind project** (`.mind/mind.toml`) holds artifacts beside its code; a
**flayer workspace** (`.mindflayer/flayer.toml`) links mind projects, holds
artifacts of its own, gathers skills from git onto a shelf and installs them
into projects. `README.md` is the user-facing surface. `docs/architecture.md`
records why each mechanism is the way it is: read the section for whatever you
are about to change before changing it, and keep it true afterwards. Its
"Decisions" section holds what the owner already chose for the roadmap, and the
one choice still pending: read it before changing a format, installing, MCP,
credentials or the server, and carry a decision out rather than reopen it.

## Commands

The root `Makefile` is what CI runs, so prefer it to raw cargo. `make help`
lists everything.

```bash
make test                 # every suite
make fmt/check            # CI runs this first
make lint                 # clippy --workspace --all-targets
make run ARGS="flayer list"
make core/test            # one app: make <app>/<target>
```

One suite, one module or one test:

```bash
cargo test -p mindflayer-cli --test tui_test                 # apps/cli/tests/tui_test.rs
cargo test -p mindflayer-core --test install_test <substring>
cargo test -p mindflayer-core --lib workspace                # src/workspace_test.rs
```

`make dev/link`, `make dev/unlink` and `make install` write into `~/.cargo/bin`
(or `BINDIR`), outside the repository. Don't run them unless asked.

## Layout

Two crates in one Cargo workspace, each an app under `apps/` with its own
`Makefile`:

- `apps/core` (`mindflayer-core`): the engine. Projects, workspaces, kinds,
  the catalog, gathering (`gix`), the SQLite ledger, installing, creating and
  changing artifacts. Its only I/O is files, and it never reads the
  environment.
- `apps/cli` (`mindflayer-cli`): the binaries `mind` (`src/main.rs`) and
  `flayer` (`src/bin/flayer.rs`), and the TUI. `src/lib.rs` only re-exports
  the public API. The parsers are in `src/parse.rs`. `src/commands.rs`
  dispatches them and finds what a command acts on, and each family of
  commands has a file under `src/commands/`: `project` (init, link, unlink,
  scan), `workspace` (load, unload), `artifact` (add, edit, rename, remove,
  templates), `read` (list, show, validate), `gather` and `install`.
  `src/render.rs` holds `Outcome`, `Failure` and the table helpers,
  `src/error.rs` holds `CliError`, and `src/homes.rs` decides the default
  home and the agent homes. `src/tui/` and `src/install/` are the two
  screens.

## Invariants that hold the design together

These are easy to break from a single file without noticing.

- **Rules live in core.** The CLI parses, asks core and renders. Name limits,
  front matter, where a kind lives and sort order are all core's, so a future
  front end cannot disagree with this one.
- **Two binaries, one parser.** `flayer <cmd>` is `mind flayer <cmd>`, and
  both reach the same function. `mind add` and `flayer add` (with or without
  `-p`) are one implementation given a different holder
  (`FlayerWorkspace::own` makes the workspace look like a `MindProject`).
- **The TUI only runs commands.** A key produces a `Task`, and
  `Task::commands` turns it into argument lines that are parsed by
  `Cli`/`FlayerCli` and run through `run`/`run_flayer_cli`. A new feature is
  written as a command first; the TUI gains it by producing that command's
  words. Never add a TUI-only code path.
- **Output is built, then printed.** Commands return an `Outcome` (stdout
  text, stderr lines, ok), so tests assert on the exact text a user sees.
- **Failures are collected, not raised.** One unreadable artifact or stale
  registry entry is a `warning:` on stderr and a non-zero exit, never a reason
  to hide the rest. An absence (no `skills/` folder yet) is not a failure.
- **Never overwrite what somebody wrote.**
  - New markers are written from string templates so their comments survive.
    Edits go through `toml_edit` (`edit_array`) and a temp file plus a rename.
  - Creation uses `create_new` or `create_dir`.
  - `rename` rewrites only the `name:` line (`frontmatter::with_key`).
  - `remove` is a dry run without `--yes`.
- **Mindflayer only manages what it installed.** The ledger's record of an
  install is what allows overwriting and deleting. An unrecorded artifact is
  `Standing::Foreign` and is never touched. `edit`, `rename` and `remove`
  drop the record (`install::disown`).
- **Loaded and gathered content is untrusted.**
  - An installed name must be one folder name (`install::is_folder_name`).
  - Symlinks are followed only inside the source (`copy::tree`'s `within`).
  - A project's artifact directories must stay inside the project.
- **A gather never prompts for a credential itself.** Every connection `gix`
  lets a clone configure gets the callback in `gather/credentials.rs`, which
  runs the git-config credential helpers with `gix`'s prompt and askpass
  turned off. A credential none of them has is `GitError::Credentials`, never
  a question on a terminal the TUI owns or a server lacks. Anything new that
  connects to a remote takes the same callback. Two gaps are open questions in
  the architecture doc: what the system ssh asks on the terminal, and `gix`'s
  retry after a remote branch named `HEAD`, a connection the callback cannot
  reach.
- **Formats are versioned.** Each marker carries its own `version`
  (`MIND_FORMAT_VERSION` for `mind.toml`, `FLAYER_FORMAT_VERSION` for
  `flayer.toml`, so bumping one never makes older builds refuse the other; see
  `DIRECTORIES_VERSION` for how a v1 `mind.toml` is still read) and the ledger
  uses SQLite's `user_version`. Anything from a newer version is refused. A
  format change must keep reading old files the way they were written.
- **Paths are lexical and portable.** Route arithmetic lives in
  `apps/core/src/paths.rs`. Registry entries are stored relative to the
  workspace with forward slashes, and `link` verifies a route before storing
  it. Directory walking is where Windows differs, which is why CI has a
  macOS/Windows matrix.
- **Home comes from the front end.** The order is `--home`, then
  `$MINDFLAYER_HOME`, then the user's home; empty means none. Only commands
  that write may create `~/.mindflayer` (`locate_or_default`). Reads use
  `default_at`, which never creates it.
- **So does the workspace a change consults.** `install::disown` and template
  lookup take it as a parameter, the one the command resolved, and core never
  walks up from the project to find one: a walk misses a workspace that links
  the project from beside it.
- **So do the agent homes.** `homes::agent_homes` takes each from its option
  (`--claude-config-dir`, `--codex-home`, `--opencode-config-dir`,
  `--agents-dir`), then `$CLAUDE_CONFIG_DIR`, `$CODEX_HOME` or
  `$OPENCODE_CONFIG_DIR`, then `~/.claude`, `~/.codex`, `~/.config/opencode`
  or `~/.agents`; empty means none. Core takes them as `AgentHomes` and never
  resolves one.
- **`Kind` is a closed enum.** To add a kind, follow "Adding a third kind" in
  the architecture doc; the compiler's exhaustive matches are the checklist.

## Tests

- Integration suites are `apps/<app>/tests/<area>_test.rs`, with shared
  helpers in `tests/common/mod.rs` (`use common::*;`). Unit tests sit in
  `src/<module>_test.rs`, wired in with
  `#[cfg(test)] #[path = "<module>_test.rs"] mod tests;`.
- No test reads the real home or the real agent homes, which `HOME`,
  `MINDFLAYER_HOME`, `CLAUDE_CONFIG_DIR`, `CODEX_HOME` and
  `OPENCODE_CONFIG_DIR` lead to. In-process tests pin options: CLI tests call
  `common::mind` / `common::flayer`, which parse through the real parsers and
  append `-C <dir>`, `--home=` and the four agent-home options empty
  (`common::PINNED`), so no test can reach or create the real home workspace
  or read a real agent folder. A test that spawns a binary clears those
  variables and sets `HOME` (`USERPROFILE` on Windows) to a temp dir. Not to
  empty: the standard library then falls back to the account's real home.
  Keep it that way.
- Git sources are built in-process with `gix` (`common::repository`): no
  network and no `git` binary.
- Screens are tested without a terminal. Drive `state` with key events, or
  call `tui::step`, and render `ui` into `ratatui::backend::TestBackend`. Only
  the event loops in `tui.rs` and `install.rs` go untested, so keep logic out
  of them.
- Fixtures use `tempfile::TempDir`. Test names are sentences
  (`init_flayer_is_the_same_as_init`).

## Workflow

- Gitflow is enforced by required checks. PRs into `dev` come from
  `feature/*`, `hotfix/*` or `release/*` (or `main`, for a back-merge). PRs
  into `main` come only from `dev`. PRs are merged with merge commits.
- CI runs `fmt and clippy`, then `test (core)`, then `test (cli)`, and builds
  each app after its tests pass. macOS and Windows run on pushes to
  `dev`/`main`, on PRs from `dev`, `main` or `release/*`, and on PRs labelled
  `full-matrix`.
- Commit messages: `<area>: <imperative summary>`, where area is `core`,
  `cli`, `docs`, `build`, `ci` or `chore` (`core, cli:` when a change spans
  both crates).
- When behaviour changes, update the README (its examples are literal output,
  so re-run them) and `docs/architecture.md`, including its "Open questions".

## Style

Comments and doc comments explain *why* a choice was made, often naming the
alternative that was rejected. They rarely narrate what the code does. Match
that density and voice in code and docs alike.
